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
//! `--release`, `--debug-audio-file`, `--debug-save-audio`, `--mock-text`,
//! `--mock-text-sequence`, `--debug-set-api-key-stdin`,
//! `--debug-delete-api-key`; see [`flags`]; the key flags run in the
//! launching process and are never forwarded);
//! release builds ignore them and have no way to inject key events or
//! audio.
//!
//! M4.4: dictation uses the real provider. The engine is built from the saved
//! settings and the keychain at startup and rebuilt whenever they're saved
//! ([`engine`]); `--transcriber mock` keeps the mock.
//!
//! M5.3: logs go to a size-bounded rotating file ([`logging`]), the tray's
//! "Reveal log" opens its folder, and the settings screen has a "Launch at
//! login" toggle backed by the OS's own login item (`commands::LoginItem`).
//!
//! M3.4: Settings… shows the `main` window, which holds the settings UI; its
//! close button hides it instead of destroying it, so the tray can show it
//! again.
//!
//! M5.1: the `overlay` window shows Recording, Transcribing and errors
//! without taking focus ([`overlay`]).

// Only the build script and the capability tests need the list.
#[cfg(test)]
mod command_names;
pub mod commands;
pub mod engine;
mod flags;
mod logging;
mod overlay;
mod updater;

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use dictation_core::coordinator::Phase;
use dictation_core::hotkey_spec::HotkeySpec;
use dictation_pipeline::debug_audio::SavingAudio;
#[cfg(not(debug_assertions))]
use dictation_pipeline::hotkey::PermissionTolerantHotkey;
use dictation_pipeline::pipeline::{Config, CursorReader, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{GrantedPermissions, MicrophoneOnly};
use dictation_pipeline::test_hooks::Injector;
#[cfg(debug_assertions)]
use dictation_pipeline::test_hooks::SyntheticHotkey;
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::audio::{FileAudioSource, SystemMicrophone};
use dictation_platform::clock::SystemClock;
use dictation_platform::cursor::SystemTextBeforeCursor;
use dictation_platform::hotkey::SystemHotkey;
use dictation_platform::insert::SystemInserter;
use dictation_platform::launch::SystemLaunchAtLogin;
use dictation_platform::permissions::SystemPermissions;
use dictation_platform::single_instance::{self, Instance, Launch, Message, Rejected};
use dictation_ports::{AudioSource, Permission, PermissionError, PermissionStatus, Permissions};
use dictation_transcribe::engine::{Engine, Transcribers};
use dictation_transcribe::mock::MockTranscriber;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, RunEvent, WindowEvent};
use tokio::sync::mpsc::UnboundedSender;

const QUIT_ID: &str = "quit";
const SETTINGS_ID: &str = "settings";
const REVEAL_LOG_ID: &str = "reveal-log";
/// The one window: it holds the settings UI (`src/Settings.tsx`).
const SETTINGS_WINDOW: &str = "main";
const TRAY_ID: &str = "main";
const TOOLTIP: &str = "dictation-app";
/// Where a launch's flags came from, for the log.
const THIS_LAUNCH: &str = "this launch";
const SECOND_LAUNCH: &str = "a second launch";

/// The app's log folder, for the tray's "Reveal log".
struct LogDir(std::path::PathBuf);

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
/// permission (Accessibility isn't checked on press: the paste reports it
/// as `InsertError::PermissionDenied`, so the test flags work without it), or
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

/// The commands that run in a process of their own (debug builds): the API
/// key commands store the Groq API key from stdin and/or remove it; the
/// launch-at-login commands set or print the login item. Each says what it
/// did on stderr. `None` when `argv` has none, so the app starts normally;
/// otherwise the exit code. The key is never logged or printed.
fn run_key_commands(argv: &[String], mut stdin: impl std::io::Read) -> Option<i32> {
    let launch = flags::parse(argv, flags::TEST_FLAGS);
    if launch.key_commands.is_empty() && launch.launch_commands.is_empty() {
        return None;
    }
    let login = SystemLaunchAtLogin::new();
    for command in launch.launch_commands {
        match dictation_pipeline::test_hooks::run_launch_command(command, &login) {
            Ok(message) => eprintln!("{message}"),
            Err(error) => {
                eprintln!("{error}");
                return Some(1);
            }
        }
    }
    if launch.key_commands.is_empty() {
        return Some(0);
    }
    // No settings are read or written; only the keychain is used.
    let config = dictation_pipeline::config::Config::new(
        std::env::temp_dir(),
        dictation_platform::secrets::KeychainSecretStore::new(),
    );
    for command in launch.key_commands {
        match dictation_pipeline::test_hooks::run_key_command(command, &config, &mut stdin) {
            Ok(message) => eprintln!("{message}"),
            Err(error) => {
                eprintln!("{error}");
                return Some(1);
            }
        }
    }
    Some(0)
}

/// Builds and runs the Tauri application.
///
/// # Panics
///
/// Panics if Tauri fails to start, which leaves the app with nothing to show.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The API key commands (debug builds) run in this process, before the
    // single-instance check: a second launch does its own key work and exits,
    // and never forwards it to the running app (which ignores such flags).
    let argv: Vec<String> = std::env::args().collect();
    if let Some(code) = run_key_commands(&argv, std::io::stdin()) {
        std::process::exit(code);
    }
    let context = tauri::generate_context!();
    // Next, so a second launch exits before setting anything else up.
    let config = single_instance::Config::new(&context.config().identifier);
    let instance = match single_instance::acquire(&config, &Message::current()) {
        Ok(Launch::Primary(instance)) => instance,
        Ok(Launch::Forwarded) => return,
        Err(error) => exit_early(&format!(
            "could not hand this launch to the running app: {error}"
        )),
    };

    let (hotkey, injector) = shell_hotkey();

    // No Dock icon and no focus stolen at launch (#59).
    let builder = dictation_platform::menu_bar::builder(tauri::Builder::default());
    // The updater and the native dialogs (M5.4).
    let builder = updater::plugins(builder);
    #[allow(clippy::expect_used)] // No UI exists yet to report a startup failure to.
    let mut app = builder
        // The overlay's non-activating panel (M5.1).
        .plugin(dictation_platform::overlay::plugin())
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
            setup(app, hotkey, injector, instance)?;
            Ok(())
        })
        .build(context)
        .expect("error while building the Tauri application");
    // A menu-bar app from the first moment: no Dock icon, not even briefly
    // (#59). Must come before `run`; see `menu_bar`.
    dictation_platform::menu_bar::app(&mut app);

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

fn setup(
    app: &mut App,
    hotkey: ShellHotkey,
    injector: Option<Injector>,
    mut instance: Instance,
) -> anyhow::Result<()> {
    let log_dir = app.path().app_log_dir().context("no log directory")?;
    logging::init(&log_dir).context("could not start logging")?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"), log_dir = %log_dir.display(), "starting");
    app.manage(LogDir(log_dir));

    build_tray(app).context("could not create the tray icon")?;

    let argv: Vec<String> = std::env::args().collect();
    let launch = flags::parse(&argv, flags::TEST_FLAGS);
    // Settings and API keys for the commands (M3.3), used by the settings UI
    // (M3.4) and, since M4.4, to build the engine.
    let config_dir = app.path().app_config_dir().context("no config directory")?;
    let config = Arc::new(dictation_pipeline::config::Config::new(
        &config_dir,
        dictation_platform::secrets::KeychainSecretStore::new(),
    ));
    tracing::info!(path = %config.path().display(), "settings file");

    let choice = launch.transcriber.unwrap_or_default();
    // `--mock-text` and `--mock-text-sequence` shape the mock engine only.
    let mut mock = launch
        .mock_text
        .clone()
        .map_or_else(MockTranscriber::default, MockTranscriber::new);
    if let Some(texts) = &launch.mock_text_sequence {
        mock = mock.with_texts(texts);
    }
    let (transcribers, live) = match choice {
        flags::TranscriberChoice::Mock => (
            Transcribers::new(Engine::Mock(mock)),
            engine::LiveEngine::fixed(),
        ),
        flags::TranscriberChoice::Settings => {
            // Startup never touches the keychain, which can block on a
            // locked keychain or an access prompt. The key is read on a
            // thread of its own once the app is up (below).
            let transcribers = Transcribers::new(engine::starting());
            (
                transcribers.clone(),
                engine::LiveEngine::follow(transcribers),
            )
        }
    };
    tracing::info!(
        transcriber = choice.name(),
        engine = transcribers.name(),
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
        transcriber: transcribers,
        inserter: SystemInserter::new(),
        permissions,
        clock: SystemClock::new(),
    };
    // The character before the cursor gives the smart leading space (#93);
    // it is read for one dictation at a time and never logged.
    let cursor = CursorReader::new(SystemTextBeforeCursor::new());
    // Pipeline::spawn needs to run inside the tokio runtime Tauri owns.
    let (pipeline, ui_events) = tauri::async_runtime::block_on(async {
        Pipeline::spawn_with_cursor(ports, cursor, Config::default())
    })
    .context("could not start the pipeline")?;
    app.manage(RunningPipeline(Mutex::new(Some(pipeline))));
    app.manage(commands::AppConfig(Arc::clone(&config)));
    app.manage(commands::LoginItem(Arc::new(SystemLaunchAtLogin::new())));
    app.manage(live.clone());
    if let Err(error) = live.rebuild_in_background(config) {
        tracing::error!(%error, "could not start reading the API key");
    }
    // Dictation still works without the overlay.
    let overlay = overlay::start(app.handle())
        .inspect_err(|error| tracing::error!(%error, "could not create the overlay"))
        .ok();
    tauri::async_runtime::spawn(show_ui_events(app.handle().clone(), ui_events, overlay));
    tracing::info!(hotkey = %HotkeySpec::default(), "ready");

    // This launch's own flags first, so they come before any forwarded one.
    // A failed injection is already logged; nothing waits for this launch.
    let _ = handle_flags(injector.as_ref(), launch, THIS_LAUNCH);

    // Last: a later launch is answered only once the pipeline runs and can
    // take its events. Until now it waited in the socket's backlog (its
    // launch gives up after the I/O timeout), so it is never acknowledged
    // without being delivered.
    listen_for_later_launches(&mut instance, injector)
        .context("could not listen for later launches")?;
    app.manage(SingleInstance(Mutex::new(Some(instance))));
    Ok(())
}

/// Hands each later launch's flags to the running pipeline, and
/// acknowledges the launch only if all of them were delivered; otherwise
/// that launch fails with an error instead of being reported as handled.
/// Call only once the pipeline has started.
fn listen_for_later_launches(
    instance: &mut Instance,
    injector: Option<Injector>,
) -> std::io::Result<()> {
    instance.listen_checked(move |message| forwarded_launch(injector.as_ref(), &message))
}

/// Acts on the flags of a launch forwarded by [`listen_for_later_launches`].
fn forwarded_launch(injector: Option<&Injector>, message: &Message) -> Result<(), Rejected> {
    let launch = flags::parse(&message.args, flags::TEST_FLAGS);
    if launch.configure_pipeline() {
        tracing::warn!(
            from = SECOND_LAUNCH,
            "ignoring the debug audio and mock text flags; they only work on the first launch"
        );
    }
    handle_flags(injector, launch, SECOND_LAUNCH)
}

/// Acts on a launch's flags: injects the test flags (debug builds only) and
/// warns about the rest.
///
/// # Errors
///
/// [`Rejected`] if a test event couldn't be delivered (no pipeline is
/// listening, or this build has no injector): it and the events after it
/// are dropped, and the caller must not report the launch as handled.
fn handle_flags(
    injector: Option<&Injector>,
    flags: flags::LaunchFlags,
    from: &str,
) -> Result<(), Rejected> {
    if from == SECOND_LAUNCH && flags.transcriber.is_some() {
        tracing::warn!(
            from,
            "--transcriber only applies when the app starts; ignored"
        );
    }
    for engine in flags.unknown_engines_to_warn_about(from == SECOND_LAUNCH) {
        tracing::warn!(
            %engine,
            known = %flags::TranscriberChoice::known_names(),
            default = flags::TranscriberChoice::default().name(),
            from,
            "unknown --transcriber engine; using the default"
        );
    }
    if !flags.key_commands.is_empty() {
        tracing::warn!(
            from,
            "the API key commands only run as a process of their own; ignored"
        );
    }
    for flag in &flags.ignored {
        tracing::warn!(%flag, from, "ignoring an unknown flag");
    }
    for event in flags.events {
        let delivered = injector.is_some_and(|injector| injector.inject(event));
        if !delivered {
            tracing::warn!(
                ?event,
                from,
                delivered,
                "test hook: not delivered; nothing is listening"
            );
            return Err(Rejected);
        }
        // This line's text is what the e2e scenarios wait for.
        tracing::info!(?event, from, delivered, "test hook");
    }
    Ok(())
}

fn build_tray(app: &App) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, SETTINGS_ID, "Settings…", true, None::<&str>)?;
    let reveal = MenuItem::with_id(app, REVEAL_LOG_ID, "Reveal log", true, None::<&str>)?;
    let updates = MenuItem::with_id(
        app,
        updater::CHECK_ID,
        updater::CHECK_LABEL,
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit dictation-app", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &updates, &reveal, &quit])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TOOLTIP)
        .menu(&menu)
        .on_menu_event(|app, event| {
            if event.id() == QUIT_ID {
                tracing::info!("quit from the tray");
                app.exit(0);
            } else if event.id() == SETTINGS_ID {
                show_settings(app);
            } else if event.id() == REVEAL_LOG_ID {
                reveal_log(app);
            } else if event.id() == updater::CHECK_ID {
                updater::check_from_tray(app);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Opens the log folder in the file manager (Finder on macOS).
fn reveal_log(app: &AppHandle) {
    let Some(dir) = app.try_state::<LogDir>() else {
        tracing::warn!("the log folder isn't known yet");
        return;
    };
    if let Err(error) = dictation_platform::reveal::reveal_folder(&dir.0) {
        tracing::warn!(%error, "could not open the log folder");
    }
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

/// Logs each state change (never the transcript), passes each event to the
/// overlay, and shows a dictation error as the tray's title and tooltip
/// until the next recording starts.
async fn show_ui_events(
    app: AppHandle,
    mut events: UiReceiver,
    overlay: Option<UnboundedSender<UiEvent>>,
) {
    while let Some(event) = events.recv().await {
        if let Some(overlay) = &overlay {
            // Fails only once the overlay has stopped, at shutdown.
            let _ = overlay.send(event.clone());
        }
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

/// The second-launch path, against the real single-instance transport and
/// the real injector (debug builds, which have the test flags).
#[cfg(all(test, debug_assertions))]
// Clippy's allow-in-tests covers only `#[cfg(test)]` modules, not this `all(..)` one.
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    use dictation_platform::fakes::FakeHotkey;
    use dictation_ports::{HotkeyEvent, HotkeySource};

    use super::*;

    fn message(args: &[&str]) -> Message {
        Message {
            cwd: "/".into(),
            args: std::iter::once("app")
                .chain(args.iter().copied())
                .map(String::from)
                .collect(),
        }
    }

    /// A short directory (socket paths are length-limited).
    fn config() -> single_instance::Config {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "sh-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let mut config = single_instance::Config::in_dir(dir, "t");
        config.forward_timeout = Duration::from_secs(5);
        config.io_timeout = Duration::from_secs(3);
        config
    }

    fn primary(config: &single_instance::Config) -> Instance {
        match single_instance::acquire(config, &message(&[])).unwrap() {
            Launch::Primary(instance) => instance,
            Launch::Forwarded => panic!("not the primary"),
        }
    }

    /// A pipeline's hotkey source, started or not, and its injector.
    fn hotkey() -> (impl HotkeySource, Injector) {
        SyntheticHotkey::new(FakeHotkey::new())
    }

    #[test]
    fn an_event_nobody_listens_for_is_rejected_not_dropped() {
        let (mut hotkey, injector) = hotkey();
        let press = message(&["--press"]);
        // Before the pipeline starts.
        assert_eq!(forwarded_launch(Some(&injector), &press), Err(Rejected));
        // Without an injector at all.
        assert_eq!(forwarded_launch(None, &press), Err(Rejected));

        let (tx, rx) = mpsc::channel();
        hotkey
            .start(Box::new(move |event| tx.send(event).unwrap()))
            .unwrap();
        assert_eq!(forwarded_launch(Some(&injector), &press), Ok(()));
        assert_eq!(
            forwarded_launch(Some(&injector), &message(&["--release", "--press"])),
            Ok(())
        );
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            [
                HotkeyEvent::Pressed,
                HotkeyEvent::Released,
                HotkeyEvent::Pressed
            ]
        );

        // Once the pipeline has stopped, nothing listens again.
        hotkey.stop();
        assert_eq!(forwarded_launch(Some(&injector), &press), Err(Rejected));
    }

    #[test]
    fn a_launch_without_events_needs_no_injector() {
        assert_eq!(forwarded_launch(None, &message(&[])), Ok(()));
        assert_eq!(forwarded_launch(None, &message(&["--verbose"])), Ok(()));
    }

    /// R1: a second launch that arrives while the app is still starting
    /// (listener not up, pipeline not running) is delivered once it is, and
    /// only then acknowledged.
    #[test]
    fn a_forward_during_startup_is_delivered_then_acknowledged() {
        let config = config();
        let mut instance = primary(&config);
        let (mut hotkey, injector) = hotkey();

        let second = {
            let config = config.clone();
            std::thread::spawn(move || single_instance::acquire(&config, &message(&["--press"])))
        };
        // Startup is still going on: the launch has no answer yet.
        std::thread::sleep(Duration::from_millis(300));
        assert!(!second.is_finished(), "acknowledged before delivery");

        // The pipeline starts, then the shell listens.
        let (tx, rx) = mpsc::channel();
        hotkey
            .start(Box::new(move |event| tx.send(event).unwrap()))
            .unwrap();
        listen_for_later_launches(&mut instance, Some(injector)).unwrap();

        assert!(matches!(second.join().unwrap().unwrap(), Launch::Forwarded));
        assert_eq!(rx.try_recv(), Ok(HotkeyEvent::Pressed));
    }

    /// If it still can't be delivered, the launch fails, never succeeds.
    #[test]
    fn a_forward_that_cannot_be_delivered_fails_the_launch() {
        let config = config();
        let mut instance = primary(&config);
        let (_never_started, injector) = hotkey();
        listen_for_later_launches(&mut instance, Some(injector)).unwrap();

        let error = single_instance::acquire(&config, &message(&["--press"])).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("couldn't act on"), "{error}");
    }
}
