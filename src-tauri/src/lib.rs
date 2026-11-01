//! Thin Tauri shell: wires windows, commands and events to the `app` crate.
//!
//! M1.5: a menu-bar app with a tray menu (Settings and Quit) that runs the
//! pipeline with the real hotkey and the stand-ins for transcription and
//! paste. M2.2: it records from the real microphone, and
//! a dictation error (such as a missing microphone permission) shows as the
//! tray's title and tooltip until the next recording. A second launch
//! forwards its flags to the running app and exits (M1.7:
//! `dictation_platform::single_instance`, which replaced the
//! single-instance plugin). Debug builds act on the test flags (`--press`,
//! `--release`, `--debug-audio-file`, `--debug-save-audio`; see [`flags`]);
//! release builds ignore them and have no way to inject key events or
//! audio.
//!
//! M3.4: Settings… shows the `main` window, which holds the settings UI; its
//! close button hides it instead of destroying it, so the tray can show it
//! again.

// Only the build script and the capability tests need the list.
#[cfg(test)]
mod command_names;
pub mod commands;
mod flags;
mod logging;

use std::path::Path;
use std::sync::Mutex;

use anyhow::Context as _;
use dictation_core::coordinator::Phase;
use dictation_core::hotkey_spec::HotkeySpec;
use dictation_pipeline::debug_audio::SavingAudio;
#[cfg(not(debug_assertions))]
use dictation_pipeline::hotkey::PermissionTolerantHotkey;
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{DiscardingInserter, GrantedPermissions, MicrophoneOnly};
use dictation_pipeline::test_hooks::Injector;
#[cfg(debug_assertions)]
use dictation_pipeline::test_hooks::SyntheticHotkey;
use dictation_platform::audio::{FileAudioSource, SystemMicrophone};
use dictation_platform::clock::SystemClock;
use dictation_platform::hotkey::SystemHotkey;
use dictation_platform::permissions::SystemPermissions;
use dictation_platform::single_instance::{self, Instance, Launch, Message};
use dictation_ports::{AudioSource, Permission, PermissionError, PermissionStatus, Permissions};
use dictation_transcribe::mock::MockTranscriber;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, RunEvent, WindowEvent};
use tokio::sync::mpsc::UnboundedReceiver;

const QUIT_ID: &str = "quit";
const SETTINGS_ID: &str = "settings";
/// The one window: it holds the settings UI (`src/Settings.tsx`).
const SETTINGS_WINDOW: &str = "main";
const TRAY_ID: &str = "main";
const TOOLTIP: &str = "dictation-app";
/// Where a launch's flags came from, for the log.
const THIS_LAUNCH: &str = "this launch";
const SECOND_LAUNCH: &str = "a second launch";

/// The running pipeline, taken out on Quit to shut it down.
struct RunningPipeline(Mutex<Option<Pipeline>>);

/// This process's hold on being the one running instance, released on
/// Quit once the pipeline has stopped, so a new launch can't start a second
/// pipeline while this one shuts down.
struct SingleInstance(Mutex<Option<Instance>>);

/// Debug builds: the real hotkey plus injected test flags; a missing
/// Accessibility grant (or no adapter) leaves only the test flags.
#[cfg(debug_assertions)]
type ShellHotkey = SyntheticHotkey<SystemHotkey>;
/// Release builds: only the real hotkey. A missing Accessibility grant is
/// logged and not fatal; any other hotkey error stops startup.
#[cfg(not(debug_assertions))]
type ShellHotkey = PermissionTolerantHotkey<SystemHotkey>;

/// The permissions the pipeline checks on each press: the real microphone
/// permission (Accessibility isn't needed while paste is a stand-in), or
/// none when a debug audio file stands in for the microphone.
enum ShellPermissions {
    Microphone(MicrophoneOnly<SystemPermissions>),
    None(GrantedPermissions),
}

impl Permissions for ShellPermissions {
    fn check(&self, permission: Permission) -> PermissionStatus {
        match self {
            Self::Microphone(permissions) => permissions.check(permission),
            Self::None(permissions) => permissions.check(permission),
        }
    }

    fn request(&self, permission: Permission) -> Result<(), PermissionError> {
        match self {
            Self::Microphone(permissions) => permissions.request(permission),
            Self::None(permissions) => permissions.request(permission),
        }
    }
}

/// The audio source for this launch's flags: the microphone, or (debug
/// builds, `--debug-audio-file`) a WAV file; with `--debug-save-audio`
/// (debug builds) every recording is also saved under `recordings`.
fn shell_audio(
    launch: &flags::LaunchFlags,
    recordings: &Path,
) -> (Box<dyn AudioSource>, ShellPermissions) {
    let (audio, permissions): (Box<dyn AudioSource>, _) = match &launch.audio_file {
        Some(path) => {
            tracing::info!(path = %path.display(), "test hook: recording a file, not the microphone");
            (
                Box::new(FileAudioSource::new(path)),
                ShellPermissions::None(GrantedPermissions),
            )
        }
        None => (
            Box::new(SystemMicrophone::new()),
            ShellPermissions::Microphone(MicrophoneOnly(SystemPermissions::new())),
        ),
    };
    if launch.save_audio {
        tracing::info!(dir = %recordings.display(), "debug mode: saving every recording");
        return (Box::new(SavingAudio::new(audio, recordings)), permissions);
    }
    (audio, permissions)
}

/// The hotkey for this build, and the injector for the test flags (debug
/// builds only).
fn shell_hotkey() -> (ShellHotkey, Option<Injector>) {
    let system = SystemHotkey::new(HotkeySpec::default());
    #[cfg(debug_assertions)]
    {
        let (hotkey, injector) = SyntheticHotkey::new(system);
        (hotkey, Some(injector))
    }
    #[cfg(not(debug_assertions))]
    {
        (PermissionTolerantHotkey::new(system), None)
    }
}

/// Builds and runs the Tauri application.
///
/// # Panics
///
/// Panics if Tauri fails to start, which leaves the app with nothing to show.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    // First, so a second launch exits before setting anything else up.
    let config = single_instance::Config::new(&context.config().identifier);
    let mut instance = match single_instance::acquire(&config, &Message::current()) {
        Ok(Launch::Primary(instance)) => instance,
        Ok(Launch::Forwarded) => return,
        Err(error) => exit_early(&format!(
            "could not hand this launch to the running app: {error}"
        )),
    };

    let (hotkey, injector) = shell_hotkey();
    let forward = injector.clone();
    let listening = instance.listen(move |message| {
        let launch = flags::parse(&message.args, flags::TEST_FLAGS);
        if launch.configure_audio() {
            tracing::warn!(
                from = SECOND_LAUNCH,
                "ignoring the debug audio flags; they only work on the first launch"
            );
        }
        handle_flags(forward.as_ref(), launch, SECOND_LAUNCH);
    });
    if let Err(error) = listening {
        exit_early(&format!("could not listen for later launches: {error}"));
    }

    #[allow(clippy::expect_used)] // No UI exists yet to report a startup failure to.
    let app = tauri::Builder::default()
        .invoke_handler(commands::builder().invoke_handler())
        .on_window_event(|window, event| {
            // Closing the settings window hides it, so the tray can show it
            // again; destroying it would leave the tray nothing to show.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(error) = window.hide() {
                    tracing::warn!(%error, "could not hide the settings window");
                }
            }
        })
        .setup(move |app| {
            app.manage(SingleInstance(Mutex::new(Some(instance))));
            setup(app, hotkey, injector.as_ref())?;
            Ok(())
        })
        .build(context)
        .expect("error while building the Tauri application");

    app.run(|app, event| {
        if let RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            // Closing the last window keeps the menu-bar app running; only
            // Quit (which passes an exit code) ends it.
            api.prevent_exit();
        } else if let RunEvent::Exit = event {
            shut_down(app);
        }
    });
}

/// Ends a launch that failed before logging started: stderr is all there
/// is.
fn exit_early(reason: &str) -> ! {
    #[allow(clippy::print_stderr)] // Logging isn't set up yet.
    {
        eprintln!("dictation-app: {reason}");
    }
    std::process::exit(1);
}

fn setup(app: &mut App, hotkey: ShellHotkey, injector: Option<&Injector>) -> anyhow::Result<()> {
    let log_dir = app.path().app_log_dir().context("no log directory")?;
    logging::init(&log_dir).context("could not start logging")?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), log_dir = %log_dir.display(), "starting");

    // A menu-bar app: no Dock icon. LSUIElement (src-tauri/Info.plist) alone
    // isn't enough, since Tauri applies its own Regular policy at launch.
    // The one cfg(target_os) outside `platform`: the Tauri API exists only
    // on macOS and must be called on the App, before the event loop runs.
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    build_tray(app).context("could not create the tray icon")?;

    let argv: Vec<String> = std::env::args().collect();
    let launch = flags::parse(&argv, flags::TEST_FLAGS);
    // Only one engine so far; an enum that delegates comes with the second.
    let flags::TranscriberChoice::Mock = launch.transcriber.unwrap_or_default();
    let transcriber = MockTranscriber::default();
    tracing::info!(
        transcriber = flags::TranscriberChoice::Mock.name(),
        "engine"
    );

    let recordings = app
        .path()
        .app_data_dir()
        .context("no data directory")?
        .join("debug-recordings");
    let (audio, permissions) = shell_audio(&launch, &recordings);
    let ports = Ports {
        hotkey,
        audio,
        transcriber,
        inserter: DiscardingInserter,
        permissions,
        clock: SystemClock::new(),
    };
    // Pipeline::spawn needs to run inside the tokio runtime Tauri owns.
    let (pipeline, ui_events) =
        tauri::async_runtime::block_on(async { Pipeline::spawn(ports, Config::default()) })
            .context("could not start the pipeline")?;
    app.manage(RunningPipeline(Mutex::new(Some(pipeline))));

    // Settings and API keys for the commands (M3.3), used by the settings UI (M3.4).
    let config_dir = app.path().app_config_dir().context("no config directory")?;
    let config = dictation_pipeline::config::Config::new(
        &config_dir,
        dictation_platform::secrets::KeychainSecretStore::new(),
    );
    tracing::info!(path = %config.path().display(), "settings file");
    app.manage(commands::AppConfig(std::sync::Arc::new(config)));
    tauri::async_runtime::spawn(show_ui_events(app.handle().clone(), ui_events));
    tracing::info!(hotkey = %HotkeySpec::default(), "ready");

    handle_flags(injector, launch, THIS_LAUNCH);
    Ok(())
}

/// Acts on a launch's flags: injects the test flags (debug builds only) and
/// warns about the rest.
fn handle_flags(injector: Option<&Injector>, flags: flags::LaunchFlags, from: &str) {
    if from == SECOND_LAUNCH && flags.transcriber.is_some() {
        tracing::warn!(
            from,
            "--transcriber only applies when the app starts; ignored"
        );
    }
    for engine in &flags.unknown_engines {
        tracing::warn!(
            %engine,
            known = %flags::TranscriberChoice::known_names(),
            default = flags::TranscriberChoice::default().name(),
            from,
            "unknown --transcriber engine; using the default"
        );
    }
    for flag in &flags.ignored {
        tracing::warn!(%flag, from, "ignoring an unknown flag");
    }
    for event in flags.events {
        let delivered = injector.is_some_and(|injector| injector.inject(event));
        tracing::info!(?event, from, delivered, "test hook");
    }
}

fn build_tray(app: &App) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, SETTINGS_ID, "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit dictation-app", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &quit])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TOOLTIP)
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id() == QUIT_ID {
                tracing::info!("quit from the tray");
                app.exit(0);
            } else if event.id() == SETTINGS_ID {
                show_settings(app);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Shows the settings window in front, even when it's minimized or hidden.
fn show_settings(app: &AppHandle) {
    let Some(window) = app.get_webview_window(SETTINGS_WINDOW) else {
        tracing::warn!("the settings window doesn't exist");
        return;
    };
    let shown = window
        .unminimize()
        .and_then(|()| window.show())
        .and_then(|()| window.set_focus());
    if let Err(error) = shown {
        tracing::warn!(%error, "could not show the settings window");
    }
}

/// Logs each state change (never the transcript) and shows a dictation
/// error as the tray's title and tooltip until the next recording starts.
/// The overlay replaces this in the UI milestones.
async fn show_ui_events(app: AppHandle, mut events: UnboundedReceiver<UiEvent>) {
    while let Some(event) = events.recv().await {
        match event {
            UiEvent::StateChanged(phase) => {
                tracing::info!(?phase, "state changed");
                if phase == Phase::Recording {
                    show_in_tray(&app, None);
                }
            }
            UiEvent::Error(error) => {
                tracing::info!(%error, "shown to the user");
                show_in_tray(&app, Some(&error.to_string()));
            }
        }
    }
}

/// Puts `error` in the tray's title (next to the icon) and tooltip, or
/// clears it.
fn show_in_tray(app: &AppHandle, error: Option<&str>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let tooltip = error.map_or_else(|| TOOLTIP.to_owned(), |error| format!("{TOOLTIP}: {error}"));
    if let Err(error) = tray
        .set_title(error)
        .and_then(|()| tray.set_tooltip(Some(tooltip)))
    {
        tracing::warn!(%error, "could not update the tray");
    }
}

fn shut_down(app: &AppHandle) {
    if let Some(state) = app.try_state::<RunningPipeline>() {
        let pipeline = state
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(pipeline) = pipeline {
            tauri::async_runtime::block_on(pipeline.shutdown());
            tracing::info!("stopped");
        }
    }
    // Last, and explicitly: Tauri exits the process without running
    // destructors, which would leave the socket behind (harmless, but the
    // next launch would have to clean it up).
    if let Some(state) = app.try_state::<SingleInstance>() {
        drop(
            state
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take(),
        );
    }
}
