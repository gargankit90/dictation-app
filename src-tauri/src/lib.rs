//! Thin Tauri shell: wires windows, commands and events to the `app` crate.
//!
//! M1.5: a menu-bar app with a tray menu (Settings, disabled for now, and
//! Quit) that runs the pipeline with the real hotkey and the stand-ins for
//! audio, transcription and paste. A second launch forwards its flags to
//! the running app through the single-instance plugin and exits. Debug
//! builds act on the `--press`/`--release` test flags (see [`flags`]);
//! release builds ignore them and have no way to inject key events.

mod flags;
mod logging;

use std::sync::Mutex;

use anyhow::Context as _;
use dictation_core::hotkey_spec::HotkeySpec;
#[cfg(not(debug_assertions))]
use dictation_pipeline::hotkey::PermissionTolerantHotkey;
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{DiscardingInserter, GrantedPermissions, SilentAudio};
use dictation_pipeline::test_hooks::Injector;
#[cfg(debug_assertions)]
use dictation_pipeline::test_hooks::SyntheticHotkey;
use dictation_platform::clock::SystemClock;
use dictation_platform::hotkey::SystemHotkey;
use dictation_transcribe::mock::MockTranscriber;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, RunEvent};
use tokio::sync::mpsc::UnboundedReceiver;

const QUIT_ID: &str = "quit";
const SETTINGS_ID: &str = "settings";
/// Where a launch's flags came from, for the log.
const THIS_LAUNCH: &str = "this launch";
const SECOND_LAUNCH: &str = "a second launch";

/// The running pipeline, taken out on Quit to shut it down.
struct RunningPipeline(Mutex<Option<Pipeline>>);

/// Debug builds: the real hotkey plus injected test flags; a missing
/// Accessibility grant (or no adapter) leaves only the test flags.
#[cfg(debug_assertions)]
type ShellHotkey = SyntheticHotkey<SystemHotkey>;
/// Release builds: only the real hotkey. A missing Accessibility grant is
/// logged and not fatal; any other hotkey error stops startup.
#[cfg(not(debug_assertions))]
type ShellHotkey = PermissionTolerantHotkey<SystemHotkey>;

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
    let (hotkey, injector) = shell_hotkey();
    let forward = injector.clone();

    #[allow(clippy::expect_used)] // No UI exists yet to report a startup failure to.
    let app = tauri::Builder::default()
        // First, so a second launch exits before setting anything else up.
        .plugin(tauri_plugin_single_instance::init(
            move |_app, argv, _cwd| {
                handle_flags(
                    forward.as_ref(),
                    flags::parse(&argv, flags::TEST_FLAGS),
                    SECOND_LAUNCH,
                );
            },
        ))
        .setup(move |app| {
            setup(app, hotkey, injector.as_ref())?;
            Ok(())
        })
        .build(tauri::generate_context!())
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

    let ports = Ports {
        hotkey,
        audio: SilentAudio::default(),
        transcriber,
        inserter: DiscardingInserter,
        permissions: GrantedPermissions,
        clock: SystemClock::new(),
    };
    // Pipeline::spawn needs to run inside the tokio runtime Tauri owns.
    let (pipeline, ui_events) =
        tauri::async_runtime::block_on(async { Pipeline::spawn(ports, Config::default()) })
            .context("could not start the pipeline")?;
    app.manage(RunningPipeline(Mutex::new(Some(pipeline))));
    tauri::async_runtime::spawn(log_ui_events(ui_events));
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
    let settings = MenuItem::with_id(app, SETTINGS_ID, "Settings…", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit dictation-app", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("dictation-app")
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id() == QUIT_ID {
                tracing::info!("quit from the tray");
                app.exit(0);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Logs each state change; never the transcript.
async fn log_ui_events(mut events: UnboundedReceiver<UiEvent>) {
    while let Some(event) = events.recv().await {
        match event {
            UiEvent::StateChanged(phase) => tracing::info!(?phase, "state changed"),
            UiEvent::Error(error) => tracing::info!(%error, "shown to the user"),
        }
    }
}

fn shut_down(app: &AppHandle) {
    let Some(state) = app.try_state::<RunningPipeline>() else {
        return;
    };
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
