//! Thin Tauri shell: wires windows, commands and events to the `app` crate.
//!
//! M1.5: a menu-bar app with a tray menu (Settings and Quit) that runs the
//! pipeline with the real hotkey and the stand-ins for transcription and
//! paste. M2.2: it records from the real microphone, and
//! a dictation error (such as a missing microphone permission) shows in the
//! tray until the next recording: a short title next to the icon (#151) and
//! the message in the tooltip. A second launch
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
//!
//! M5.2: onboarding. The settings window shows the first-run steps
//! ([`onboarding`]) at launch while dictation can't work yet (a permission
//! missing or no key saved), after Relaunch, and from the tray's "Set up
//! dictation…".
//!
//! T2: the hotkey is the saved one, at startup and as soon as Settings is
//! saved (`dictation_pipeline::hotkey_setting`); a save whose hotkey can't
//! start keeps the old one and says so.
//!
//! M5.7: while the microphone is denied the tray menu offers "Relaunch
//! dictation-app" (M5.2's restart), since a grant made in System Settings
//! reaches the app only after a relaunch.

// Only the build script and the capability tests need the list.
#[cfg(test)]
mod command_names;
pub mod commands;
pub mod engine;
mod flags;
mod key_writes;
mod logging;
pub mod models;
pub mod onboarding;
mod overlay;
mod updater;
pub mod usage;
mod window;

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use dictation_pipeline::debug_audio::SavingAudio;
use dictation_pipeline::hotkey_setting::{self, ConfigurableHotkey, HotkeyControl};
use dictation_pipeline::pipeline::{Config, CursorReader, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{GrantedPermissions, MicrophoneOnly};
use dictation_pipeline::test_hooks::Injector;
#[cfg(debug_assertions)]
use dictation_pipeline::test_hooks::{Breakable, HotkeyOutage, SyntheticHotkey};
use dictation_pipeline::tray::{TrayStatus, TrayView};
use dictation_pipeline::ui_events::{UiReceiver, UiSender};
use dictation_pipeline::usage::UsageTracker;
use dictation_platform::audio::{FileAudioSource, SystemMicrophone};
use dictation_platform::clock::SystemClock;
use dictation_platform::cursor::SystemTextBeforeCursor;
use dictation_platform::hotkey::SystemHotkey;
use dictation_platform::insert::SystemInserter;
use dictation_platform::launch::SystemLaunchAtLogin;
use dictation_platform::permissions::SystemPermissions;
use dictation_platform::secrets::KeychainSecretStore;
use dictation_platform::single_instance::{self, Instance, Launch, Message, Rejected};
use dictation_ports::{
    AudioSource, EngineKind, Permission, PermissionError, PermissionStatus, Permissions,
};
use dictation_transcribe::engine::{Engine, Transcribers};
use dictation_transcribe::mock::MockTranscriber;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, RunEvent, WindowEvent, Wry};

const QUIT_ID: &str = "quit";
const SETTINGS_ID: &str = "settings";
const REVEAL_LOG_ID: &str = "reveal-log";
const ONBOARDING_ID: &str = "onboarding";
const RELAUNCH_ID: &str = "relaunch";
/// The tray's Relaunch item, offered while the microphone is denied (M5.7).
const RELAUNCH_LABEL: &str = "Relaunch dictation-app";
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

/// Debug builds: the real hotkey for the saved spec, switched when Settings
/// is saved and restarted when it dies; the outage flags can break it.
#[cfg(debug_assertions)]
type SavedHotkey = ConfigurableHotkey<Breakable<SystemHotkey>>;
/// Release builds: the real hotkey for the saved spec, switched when
/// Settings is saved and restarted when it dies. A missing Accessibility
/// grant isn't fatal: the hotkey starts once it is granted (#88).
#[cfg(not(debug_assertions))]
type SavedHotkey = ConfigurableHotkey<SystemHotkey>;

/// Debug builds: the real hotkey plus injected test flags, which work
/// whether or not the real one listens.
#[cfg(debug_assertions)]
type ShellHotkey = SyntheticHotkey<SavedHotkey>;
/// Release builds: only the real hotkey.
#[cfg(not(debug_assertions))]
type ShellHotkey = SavedHotkey;

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

/// The hotkey for this build, listening for `spec` once the pipeline starts
/// it; the control that switches it when Settings is saved; and the
/// injector for the test flags (debug builds only).
fn shell_hotkey(
    spec: dictation_core::hotkey_spec::HotkeySpec,
) -> (ShellHotkey, HotkeyControl, Option<Injector>) {
    #[cfg(debug_assertions)]
    {
        let outage = HotkeyOutage::default();
        let breaker = outage.clone();
        let (saved, control) =
            ConfigurableHotkey::new(spec, move |spec| breaker.wrap(SystemHotkey::new(spec)));
        let (hotkey, injector) = SyntheticHotkey::new(saved);
        (hotkey, control, Some(injector.with_outage(outage)))
    }
    #[cfg(not(debug_assertions))]
    {
        let (saved, control) = ConfigurableHotkey::new(spec, SystemHotkey::new);
        (saved, control, None)
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
    // First, before anything that can panic or fail (#144): Rust's default
    // hook would print a panic's message to stderr as it is.
    logging::install_panic_hook();
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

    // The updater and the native dialogs (M5.4), then no Dock icon and no
    // focus stolen at launch (#59, T2): `menu_bar::builder` doesn't activate
    // the app while another one is active.
    let builder =
        dictation_platform::menu_bar::builder(updater::plugins(tauri::Builder::default()));
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
            setup(app, instance)?;
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

fn setup(app: &mut App, mut instance: Instance) -> anyhow::Result<()> {
    let log_dir = app.path().app_log_dir().context("no log directory")?;
    logging::init(&log_dir).context("could not start logging")?;
    // The bundle's version (tauri.conf.json), the one the updater compares.
    let version = app.package_info().version.to_string();
    tracing::info!(version, log_dir = %log_dir.display(), "starting");
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
    // The saved hotkey (a file read; the keychain isn't touched).
    let (hotkey, hotkey_control, injector) =
        shell_hotkey(hotkey_setting::startup_spec(config.settings()));

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
    let live = with_provider_url(live, &launch, choice);
    // On-device models (L1) live in Application Support/<app>/models.
    let models_dir = app
        .path()
        .app_data_dir()
        .context("no data directory")?
        .join("models");
    let live = match dictation_transcribe::models::ModelStore::new(
        models_dir,
        Arc::new(dictation_platform::disk::available_space),
    ) {
        Ok(store) => live.with_local(engine::LocalModels::new(store)),
        Err(error) => {
            tracing::error!(%error, "on-device models are unavailable");
            live
        }
    };
    app.manage(models::ModelDownloads::default());
    tracing::info!(
        transcriber = choice.name(),
        engine = transcribers.name(),
        "engine"
    );

    let data_dir = app.path().app_data_dir().context("no data directory")?;
    let scripted = onboarding::scripted_launch(
        launch.audio_file.is_some(),
        choice == flags::TranscriberChoice::Mock,
        false,
    );
    // Anonymous usage events (A1): on by default, a no-op without a build
    // key, and silent in scripted test launches.
    let usage = usage::start(&data_dir, &config.settings().unwrap_or_default(), scripted);
    usage::launched(usage.clone(), &data_dir, &version);
    app.manage(usage.clone());
    let recordings = data_dir.join("debug-recordings");
    let (audio, permissions) = shell_audio(&launch, &recordings);
    // Already refused at launch: the tray offers Relaunch before any press.
    let microphone_denied = permissions.check(Permission::Microphone) == PermissionStatus::Denied;
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
    app.manage(commands::ActiveHotkey(hotkey_control.clone()));
    app.manage(commands::ConfigDir(config_dir.clone()));
    app.manage(commands::AppShell(Arc::new(TauriShell(
        app.handle().clone(),
    ))));
    app.manage(Arc::new(onboarding::Onboarding::new()));
    app.manage(onboarding::OsPermissions(
        Arc::new(SystemPermissions::new()),
    ));
    app.manage(live.clone());
    // A keychain that timed out at launch is tried again by the next
    // dictation (#204).
    live.retry_on_dictation(Arc::clone(&config));
    if let Err(error) = live.rebuild_in_background(Arc::clone(&config)) {
        tracing::error!(%error, "could not start reading the API key");
    }
    // Dictation still works without the overlay.
    let overlay = overlay::start(app.handle())
        .inspect_err(|error| tracing::error!(%error, "could not create the overlay"))
        .ok();
    tauri::async_runtime::spawn(show_ui_events(
        app.handle().clone(),
        ui_events,
        overlay,
        microphone_denied,
        usage,
    ));
    tracing::info!(hotkey = %hotkey_control.spec(), "ready");
    onboarding_at_launch(
        app.handle(),
        config,
        live,
        LaunchOnboarding {
            resumed: onboarding::take_resume_marker(&config_dir),
            scripted: onboarding::scripted_launch(
                launch.audio_file.is_some(),
                choice == flags::TranscriberChoice::Mock,
                launch.show_onboarding,
            ),
        },
    );
    // M5.4: a quiet update check; a dialog only if there is an update.
    if updater::check_at_launch_wanted(scripted, launch.check_updates) {
        updater::check_at_launch(app.handle());
    }

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

/// The running app, for the onboarding commands.
struct TauriShell(AppHandle);

impl commands::Shell for TauriShell {
    fn hide_window(&self) {
        window::hide(&self.0);
    }

    fn restart(&self) {
        // Quits as Quit does (the pipeline stops and the single-instance
        // hold is released in `shut_down`), then Tauri starts the app again
        // with the same arguments, through LaunchServices on macOS.
        self.0.request_restart();
    }
}

/// What a launch says about showing onboarding.
#[derive(Debug, Clone, Copy)]
struct LaunchOnboarding {
    /// The last launch was Relaunch.
    resumed: bool,
    /// A scripted test launch ([`onboarding::scripted_launch`]).
    scripted: bool,
}

/// Shows onboarding if this launch needs it ([`onboarding::show_at_launch`]).
/// On a thread of its own, since it reads the keychain (bounded).
fn onboarding_at_launch(
    app: &AppHandle,
    config: Arc<dictation_pipeline::config::Config<KeychainSecretStore>>,
    live: engine::LiveEngine,
    launch: LaunchOnboarding,
) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("onboarding".into())
        .spawn(move || {
            let readiness = onboarding::readiness(&SystemPermissions::new(), true, || {
                onboarding::key_saved(&config, &live)
            });
            if !onboarding::show_at_launch(readiness, launch.resumed, launch.scripted) {
                tracing::info!(?readiness, ?launch, "onboarding not shown at launch");
                return;
            }
            let step = app.state::<Arc<onboarding::Onboarding>>().open(readiness);
            // This line's text is what the e2e scenario waits for.
            tracing::info!(?step, ?readiness, ?launch, "onboarding shown at launch");
            window::show(&app, onboarding::View::Onboarding);
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "could not start the onboarding check");
    }
}

/// Opens onboarding from the tray, at the step the user was on. The key is
/// read (bounded) on a thread of its own, in case this is the first time.
fn show_onboarding(app: &AppHandle) {
    let (Some(config), Some(live)) = (
        app.try_state::<commands::AppConfig>(),
        app.try_state::<engine::LiveEngine>(),
    ) else {
        tracing::warn!("onboarding isn't ready yet");
        return;
    };
    let (config, live, app) = (Arc::clone(&config.0), live.inner().clone(), app.clone());
    let spawned = std::thread::Builder::new()
        .name("onboarding".into())
        .spawn(move || {
            let readiness = onboarding::readiness(&SystemPermissions::new(), true, || {
                onboarding::key_saved(&config, &live)
            });
            let step = app.state::<Arc<onboarding::Onboarding>>().open(readiness);
            tracing::info!(?step, "onboarding opened from the tray");
            window::show(&app, onboarding::View::Onboarding);
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "could not open onboarding");
    }
}

/// Shows the settings form.
fn show_settings(app: &AppHandle) {
    if let Some(onboarding) = app.try_state::<Arc<onboarding::Onboarding>>() {
        onboarding.close();
    }
    window::show(app, onboarding::View::Settings);
}

/// Test builds: points the real engine at `--debug-provider-url` (a local
/// stand-in provider, T4a). Release builds have no such flag (it is never
/// parsed) and this does nothing: the engine always talks to the provider.
#[cfg(debug_assertions)]
fn with_provider_url(
    live: engine::LiveEngine,
    launch: &flags::LaunchFlags,
    choice: flags::TranscriberChoice,
) -> engine::LiveEngine {
    let Some(url) = &launch.provider_url else {
        return live;
    };
    if choice == flags::TranscriberChoice::Mock {
        tracing::warn!(%url, "test hook: --debug-provider-url needs the real engine; ignored with the mock");
        return live;
    }
    // This line's text is what the e2e scenarios wait for.
    tracing::info!(%url, "test hook: the provider is reached at a local URL");
    live.at(url)
}

#[cfg(not(debug_assertions))]
fn with_provider_url(
    live: engine::LiveEngine,
    _: &flags::LaunchFlags,
    _: flags::TranscriberChoice,
) -> engine::LiveEngine {
    live
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
    for url in &flags.refused_provider_urls {
        tracing::warn!(%url, from, "ignoring --debug-provider-url: only a loopback http URL is allowed");
    }
    for flag in &flags.ignored {
        tracing::warn!(%flag, from, "ignoring an unknown flag");
    }
    for command in flags.outage_commands {
        let delivered = injector.is_some_and(|injector| injector.outage(command));
        if !delivered {
            tracing::warn!(?command, from, "test hook: this build has no hotkey outage");
            return Err(Rejected);
        }
        // This line's text is what the e2e scenario waits for.
        tracing::info!(?command, from, "test hook outage");
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

/// The tray's menu and its Relaunch item, which [`show_in_tray`] adds and
/// removes (Tauri menu items can't be hidden).
struct TrayMenu {
    menu: Menu<Wry>,
    relaunch: MenuItem<Wry>,
}

/// What a tray menu item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayAction {
    Settings,
    Onboarding,
    CheckForUpdates,
    RevealLog,
    Relaunch,
    Quit,
}

/// The action for a tray menu item's id.
fn tray_action(id: &str) -> Option<TrayAction> {
    Some(match id {
        SETTINGS_ID => TrayAction::Settings,
        ONBOARDING_ID => TrayAction::Onboarding,
        updater::CHECK_ID => TrayAction::CheckForUpdates,
        REVEAL_LOG_ID => TrayAction::RevealLog,
        RELAUNCH_ID => TrayAction::Relaunch,
        QUIT_ID => TrayAction::Quit,
        _ => return None,
    })
}

fn build_tray(app: &App) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, SETTINGS_ID, "Settings…", true, None::<&str>)?;
    let setup = MenuItem::with_id(app, ONBOARDING_ID, "Set up dictation…", true, None::<&str>)?;
    let reveal = MenuItem::with_id(app, REVEAL_LOG_ID, "Reveal log", true, None::<&str>)?;
    let updates = MenuItem::with_id(
        app,
        updater::CHECK_ID,
        updater::CHECK_LABEL,
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit dictation-app", true, None::<&str>)?;
    // Not in the menu until the microphone is denied.
    let relaunch = MenuItem::with_id(app, RELAUNCH_ID, RELAUNCH_LABEL, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&settings, &setup, &updates, &reveal, &quit])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TOOLTIP)
        .menu(&menu)
        .on_menu_event(|app, event| match tray_action(event.id().as_ref()) {
            Some(TrayAction::Quit) => {
                tracing::info!("quit from the tray");
                app.exit(0);
            }
            Some(TrayAction::Settings) => show_settings(app),
            Some(TrayAction::Onboarding) => show_onboarding(app),
            Some(TrayAction::RevealLog) => reveal_log(app),
            Some(TrayAction::CheckForUpdates) => updater::check_from_tray(app),
            Some(TrayAction::Relaunch) => {
                if let Some(shell) = app.try_state::<commands::AppShell>() {
                    commands::relaunch_from_tray(&shell);
                }
            }
            None => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(TrayMenu { menu, relaunch });
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

/// Logs each state change (never the transcript), passes each event to the
/// overlay, and shows in the tray's title and tooltip a dictation error
/// until the next recording starts, and "Hotkey unavailable" while the
/// hotkey doesn't work, and in its menu Relaunch while the microphone is
/// denied (`microphone_denied` at launch; [`TrayStatus`]).
async fn show_ui_events(
    app: AppHandle,
    mut events: UiReceiver,
    overlay: Option<UiSender>,
    microphone_denied: bool,
    usage: usage::Usage,
) {
    let mut tray = TrayStatus::new(microphone_denied);
    // Usage events from the same stream; only kinds and buckets leave it.
    let mut tracker = UsageTracker::new(usage.0.clone(), EngineKind::Cloud);
    let started = std::time::Instant::now();
    if microphone_denied {
        show_in_tray(&app, tray.view());
    }
    while let Some(event) = events.recv().await {
        if let Some(overlay) = &overlay {
            // Bounded and never blocks (#112): a stalled overlay can't hold
            // up the tray or grow the queue.
            overlay.send(event.clone());
        }
        tracker.on_event(&event, started.elapsed());
        match &event {
            UiEvent::StateChanged(phase) => tracing::info!(?phase, "state changed"),
            UiEvent::Error(error) => tracing::info!(%error, "shown to the user"),
            // This line's text is what the e2e scenario waits for.
            UiEvent::HotkeyRestored => tracing::info!("hotkey restored; shown to the user"),
        }
        if let Some(view) = tray.on_event(&event) {
            show_in_tray(&app, &view);
        }
    }
}

/// The tray's tooltip for `view`: the app's name, then the detail if any.
fn tray_tooltip(view: &TrayView) -> String {
    view.detail.as_ref().map_or_else(
        || TOOLTIP.to_owned(),
        |detail| format!("{TOOLTIP}: {detail}"),
    )
}

/// Puts `view` in the tray's title (next to the icon), tooltip and menu.
fn show_in_tray(app: &AppHandle, view: &TrayView) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    if let Err(error) = tray
        .set_title(view.title.as_deref())
        .and_then(|()| tray.set_tooltip(Some(tray_tooltip(view))))
    {
        tracing::warn!(%error, "could not update the tray");
    }
    if let Some(menu) = app.try_state::<TrayMenu>() {
        offer_relaunch(&menu, view.relaunch);
    }
}

/// Adds Relaunch at the top of the tray menu, or removes it, unless it is
/// already so.
fn offer_relaunch(tray: &TrayMenu, offered: bool) {
    let shown = tray.menu.get(RELAUNCH_ID).is_some();
    let result = match (shown, offered) {
        (false, true) => tray.menu.insert(&tray.relaunch, 0),
        (true, false) => tray.menu.remove(&tray.relaunch),
        _ => return,
    };
    match result {
        // This line's text is what the e2e scenario waits for.
        Ok(()) => tracing::info!(offered, "tray relaunch item"),
        Err(error) => tracing::warn!(%error, "could not update the tray menu"),
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
    if let Some(usage) = app.try_state::<usage::Usage>() {
        usage::flush_on_quit(&usage, std::time::Duration::from_secs(2));
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

#[cfg(test)]
mod tray_tests {
    use dictation_core::error::{DictationError, Permission};
    use dictation_pipeline::tray::{DICTATION_FAILED, HOTKEY_UNAVAILABLE, MAX_TITLE_CHARS};

    use super::*;

    /// One of each error, with reasons as long as the summary lets through.
    fn every_error() -> Vec<DictationError> {
        let long = "The provider rejected your API key. Check it in Settings. ".repeat(4);
        let all = vec![
            DictationError::TranscriptionFailed {
                reason: long.clone(),
            },
            DictationError::EmptyTranscript,
            DictationError::InsertFailed { reason: long },
            DictationError::SecureInput,
            DictationError::PermissionMissing(Permission::Microphone),
            DictationError::PermissionMissing(Permission::Accessibility),
            DictationError::HotkeyFailed,
        ];
        // A new kind of error fails to compile here until it is listed above.
        for error in &all {
            match error {
                DictationError::TranscriptionFailed { .. }
                | DictationError::EmptyTranscript
                | DictationError::InsertFailed { .. }
                | DictationError::SecureInput
                | DictationError::PermissionMissing(_)
                | DictationError::HotkeyFailed => {}
            }
        }
        all
    }

    /// #151: whatever the error, the title is the same short text, so the
    /// status item fits in the menu bar; the whole message is the tooltip.
    #[test]
    fn the_tray_title_stays_short_for_every_error() {
        for error in every_error() {
            let mut tray = TrayStatus::default();
            let view = tray.on_event(&UiEvent::Error(error.clone()));
            let view = view.unwrap_or_else(|| tray.view().clone());
            let title = view.title.clone().unwrap_or_default();
            assert!(title.chars().count() <= MAX_TITLE_CHARS, "{error}: {title}");
            let expected = match error {
                DictationError::HotkeyFailed => HOTKEY_UNAVAILABLE,
                _ => DICTATION_FAILED,
            };
            assert_eq!(title, expected, "{error}");
            assert_eq!(tray_tooltip(&view), format!("dictation-app: {error}"));
        }
    }

    /// M5.7 criterion 2: the Relaunch item's id is the Relaunch action, which
    /// calls the restart M5.2's `relaunch_app` uses
    /// (`commands::tests::the_tray_relaunch_restarts_the_app`).
    #[test]
    fn m5_7_criterion_2_the_relaunch_item_is_the_relaunch_action() {
        assert_eq!(tray_action(RELAUNCH_ID), Some(TrayAction::Relaunch));
        assert_eq!(tray_action(QUIT_ID), Some(TrayAction::Quit));
        assert_eq!(tray_action(SETTINGS_ID), Some(TrayAction::Settings));
        assert_eq!(tray_action(ONBOARDING_ID), Some(TrayAction::Onboarding));
        assert_eq!(tray_action(REVEAL_LOG_ID), Some(TrayAction::RevealLog));
        assert_eq!(
            tray_action(updater::CHECK_ID),
            Some(TrayAction::CheckForUpdates)
        );
        assert_eq!(tray_action("something else"), None);
    }

    #[test]
    fn no_error_clears_the_title() {
        let view = TrayView::default();
        assert_eq!(view.title, None);
        assert_eq!(tray_tooltip(&view), "dictation-app");
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

    /// A [`single_instance::Config`] in a short directory (socket paths are
    /// length-limited) inside a scratch directory that is removed with
    /// everything in it when the test ends, pass or fail (#127). Declare it
    /// before the instances and threads that use it, so it's dropped last.
    struct ScratchConfig {
        config: single_instance::Config,
        root: std::path::PathBuf,
    }

    impl std::ops::Deref for ScratchConfig {
        type Target = single_instance::Config;

        fn deref(&self) -> &single_instance::Config {
            &self.config
        }
    }

    impl Drop for ScratchConfig {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Unique among processes (pid and time) and within this one (counter).
    fn config() -> ScratchConfig {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let root = std::env::temp_dir().join(format!(
            "sh-{}-{}-{nanos}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let mut config = single_instance::Config::in_dir(root.join("d"), "t");
        config.forward_timeout = Duration::from_secs(5);
        config.io_timeout = Duration::from_secs(3);
        ScratchConfig { config, root }
    }

    /// #127: the fixture's lock and socket directory goes with it.
    #[test]
    fn the_config_fixture_leaves_nothing_behind() {
        let config = config();
        let root = config.root.clone();
        drop(primary(&config));
        assert!(config.lock_path().exists());
        drop(config);
        assert!(
            matches!(std::fs::symlink_metadata(&root), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
            "left {}",
            root.display()
        );
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
