//! Troubleshoot and Report a problem (E1, #209, `docs/privacy.md`).
//!
//! Troubleshoot runs live checks and offers a fix for each failing one. It
//! never records audio, and it contacts the provider only when the user
//! presses "Test connection". Report a problem builds the text of
//! [`dictation_core::report`] from typed fields; the user reads it, then
//! saves it, copies it or opens a pre-filled GitHub issue. The app uploads
//! nothing.
//!
//! [`Diagnostics`] remembers the codes and times of the last few errors,
//! never their text. The checks and the report are built by the pure
//! functions [`checks`] and [`report_text`], which the tests drive with
//! planted secrets.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use dictation_core::diagnostics::{ErrorCode, ErrorLog, FixAction, LoggedError};
use dictation_core::error::DictationError;
use dictation_core::report::{self, Access, EngineInfo, ReportInput};
use dictation_core::settings::{Settings, Transcription};
use dictation_pipeline::config::Config;
use dictation_platform::secrets::KeychainSecretStore;
use dictation_ports::{PermissionStatus, TranscribeError};
use dictation_transcribe::engine::Engine;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt as _;

use crate::commands::{self, AppConfig, KeyTestKind};
use crate::engine::LiveEngine;
use crate::models::{ModelDownloads, ModelState};
use crate::onboarding::OsPermissions;

/// What opens the browser and the file manager. Managed Tauri state: the
/// real one in the app, a recording fake in tests (F1).
#[derive(Clone)]
pub struct AppOpener(pub Arc<dyn dictation_ports::Opener>);

/// What the app remembers for Troubleshoot and the report. Managed Tauri
/// state; clones share it.
#[derive(Clone, Default)]
pub struct Diagnostics(Arc<Shared>);

#[derive(Default)]
struct Shared {
    log: Mutex<ErrorLog>,
    /// Whether the hotkey doesn't work, as the pipeline last said.
    hotkey_down: AtomicBool,
    /// Whether the tray asked for the Troubleshoot view.
    wants_troubleshoot: AtomicBool,
    /// The report the user is looking at; Save and "Open a GitHub issue"
    /// use this text, so they can't differ from what was shown.
    shown_report: Mutex<Option<String>>,
    /// For the save dialog.
    app: Mutex<Option<AppHandle>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

impl Diagnostics {
    /// Remembers the code of `error`, and that the hotkey is down if it is
    /// the hotkey's. Nothing of the error's text is kept.
    pub fn record_error(&self, error: &DictationError) {
        lock(&self.0.log).record(error, now());
        if matches!(error, DictationError::HotkeyFailed) {
            self.0.hotkey_down.store(true, Ordering::Relaxed);
        }
    }

    /// The hotkey works again.
    pub fn hotkey_restored(&self) {
        self.0.hotkey_down.store(false, Ordering::Relaxed);
    }

    /// Gives the save dialog its app.
    pub fn attach(&self, app: AppHandle) {
        *lock(&self.0.app) = Some(app);
    }

    /// The tray's "Troubleshoot…": the window shows that view next time it
    /// asks.
    pub fn ask_for_troubleshoot(&self) {
        self.0.wants_troubleshoot.store(true, Ordering::Relaxed);
    }

    fn recent(&self) -> Vec<LoggedError> {
        lock(&self.0.log).recent()
    }
}

/// Which check a row is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    /// The microphone permission.
    Microphone,
    /// The Accessibility permission.
    Accessibility,
    /// Whether the hotkey is registered.
    Hotkey,
    /// Whether the engine can transcribe: a key is readable, or the model
    /// is on this Mac.
    Engine,
    /// The last error that happened.
    LastError,
}

/// One row of Troubleshoot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct Check {
    /// Which check.
    pub id: CheckId,
    /// Its name.
    pub label: String,
    /// ✓ or ✗.
    pub ok: bool,
    /// One plain sentence: what is fine, or what is wrong and what to do.
    pub detail: String,
    /// The error code behind a ✗, if any.
    pub code: Option<ErrorCode>,
    /// The button that fixes it, if one does.
    pub fix: Option<FixAction>,
    /// That button's label.
    pub fix_label: Option<String>,
}

/// Whether the provider's key can be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    /// A key is saved.
    Saved,
    /// None is saved.
    Missing,
    /// The Keychain wouldn't say.
    Unreadable,
}

/// What the engine needs, as far as the app can tell without using it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineState {
    /// A cloud provider.
    Cloud(KeyState),
    /// A model on this Mac.
    Local(LocalState),
}

/// How far the on-device engine is, from the live engine and the download
/// status together. A file on disk alone is not "ready" (E1 round 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalState {
    /// The model is loaded: the engine can transcribe.
    Loaded,
    /// The engine is loading it now.
    Loading,
    /// It is on disk, but the engine has not loaded it (yet).
    NotLoaded,
    /// The engine tried and could not load it.
    LoadFailed,
    /// It is downloading.
    Downloading,
    /// It is not (fully) on this Mac.
    Missing,
}

/// What the live engine says about the on-device model, without asking it
/// to do anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineView {
    /// A local engine with its model loaded.
    Loaded,
    /// The model is still loading.
    Loading,
    /// The engine could not build (the loader or the file check failed).
    Failed,
    /// The model is not downloaded.
    Missing,
    /// Anything else (a cloud engine not yet rebuilt, the starting engine).
    Other,
}

impl EngineView {
    /// The view of the engine now current.
    #[must_use]
    pub fn of(engine: &Engine) -> Self {
        match engine {
            Engine::Local(_) => Self::Loaded,
            Engine::Unavailable(TranscribeError::ModelLoading) => Self::Loading,
            Engine::Unavailable(TranscribeError::Engine(_)) => Self::Failed,
            Engine::Unavailable(TranscribeError::ModelMissing) => Self::Missing,
            _ => Self::Other,
        }
    }
}

/// The on-device engine's state from the live engine's `view` and the
/// download status of the selected model.
#[must_use]
pub fn local_state(view: EngineView, download: ModelState) -> LocalState {
    if download == ModelState::Downloading {
        return LocalState::Downloading;
    }
    match view {
        EngineView::Loaded => LocalState::Loaded,
        EngineView::Loading => LocalState::Loading,
        EngineView::Failed => LocalState::LoadFailed,
        EngineView::Missing => LocalState::Missing,
        EngineView::Other if download == ModelState::Ready => LocalState::NotLoaded,
        EngineView::Other => LocalState::Missing,
    }
}

impl EngineState {
    /// The code of what is missing, or `None` if the engine is ready.
    fn problem(&self) -> Option<ErrorCode> {
        match self {
            Self::Cloud(KeyState::Saved) | Self::Local(LocalState::Loaded) => None,
            Self::Cloud(KeyState::Missing) => Some(ErrorCode::KeyMissing),
            Self::Cloud(KeyState::Unreadable) => Some(ErrorCode::KeyUnreadable),
            Self::Local(LocalState::Loading | LocalState::NotLoaded) => {
                Some(ErrorCode::ModelLoading)
            }
            Self::Local(LocalState::LoadFailed) => Some(ErrorCode::ModelLoadingFailed),
            Self::Local(LocalState::Downloading | LocalState::Missing) => {
                Some(ErrorCode::ModelMissing)
            }
        }
    }
}

/// Everything [`checks`] looks at.
#[derive(Debug, Clone)]
pub struct CheckInput {
    /// The microphone permission.
    pub microphone: PermissionStatus,
    /// The Accessibility permission.
    pub accessibility: PermissionStatus,
    /// Whether the hotkey works.
    pub hotkey_up: bool,
    /// The engine.
    pub engine: EngineState,
    /// The newest error.
    pub last_error: Option<LoggedError>,
}

fn failing(id: CheckId, label: &str, code: ErrorCode) -> Check {
    let info = code.info();
    Check {
        id,
        label: label.to_owned(),
        ok: false,
        detail: format!("{} {}", info.message, info.next_step),
        code: Some(code),
        fix: Some(info.action),
        fix_label: Some(info.action_label.to_owned()),
    }
}

fn passing(id: CheckId, label: &str, detail: &str) -> Check {
    Check {
        id,
        label: label.to_owned(),
        ok: true,
        detail: detail.to_owned(),
        code: None,
        fix: None,
        fix_label: None,
    }
}

/// The Troubleshoot rows for `input`, in the order shown.
#[must_use]
pub fn checks(input: &CheckInput) -> Vec<Check> {
    let permission = |id, label: &str, status, code| {
        if status == PermissionStatus::Granted {
            passing(id, label, "Allowed.")
        } else {
            failing(id, label, code)
        }
    };
    let engine = match input.engine.problem() {
        None => passing(CheckId::Engine, "Transcription engine", "Ready."),
        Some(code) => failing(CheckId::Engine, "Transcription engine", code),
    };
    let hotkey = if input.hotkey_up && input.accessibility == PermissionStatus::Granted {
        passing(CheckId::Hotkey, "Hotkey", "Registered.")
    } else {
        failing(CheckId::Hotkey, "Hotkey", ErrorCode::HotkeyUnregistered)
    };
    let last = match input.last_error {
        None => passing(
            CheckId::LastError,
            "Last error",
            "None since the app started.",
        ),
        Some(error) => {
            let info = error.code.info();
            Check {
                id: CheckId::LastError,
                label: "Last error".to_owned(),
                ok: false,
                detail: format!(
                    "{} at {}. {}",
                    error.code,
                    report::utc(error.at),
                    info.next_step
                ),
                code: Some(error.code),
                fix: Some(info.action),
                fix_label: Some(info.action_label.to_owned()),
            }
        }
    };
    vec![
        permission(
            CheckId::Microphone,
            "Microphone",
            input.microphone,
            ErrorCode::MicDenied,
        ),
        permission(
            CheckId::Accessibility,
            "Accessibility",
            input.accessibility,
            ErrorCode::AccessibilityMissing,
        ),
        hotkey,
        engine,
        last,
    ]
}

/// What identifies this build and Mac in the report.
#[derive(Debug, Clone)]
pub struct Environment {
    /// The app's version.
    pub app_version: String,
    /// The macOS version.
    pub macos_version: String,
    /// The CPU architecture.
    pub arch: String,
}

impl Environment {
    /// Reads the macOS version (a short child process: call it off the
    /// event loop).
    #[must_use]
    pub fn read(app_version: &str) -> Self {
        let macos_version = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .map_or_else(|| "unknown".to_owned(), |text| text.trim().to_owned());
        Self {
            app_version: app_version.to_owned(),
            macos_version,
            arch: std::env::consts::ARCH.to_owned(),
        }
    }
}

fn access(status: PermissionStatus) -> Access {
    match status {
        PermissionStatus::Granted => Access::Granted,
        PermissionStatus::Denied => Access::Denied,
        PermissionStatus::NotDetermined => Access::NotDetermined,
    }
}

/// The report for the user to read. The inputs are all typed: the settings
/// (of which only the hotkey, provider, transcription kind, on-device model
/// id and the analytics switch are read), permission states, the engine's
/// readiness, and error codes with times.
#[must_use]
pub fn report_text(
    env: &Environment,
    settings: &Settings,
    permissions: (PermissionStatus, PermissionStatus),
    engine_ready: Option<bool>,
    errors: &[LoggedError],
) -> String {
    let engine = match settings.transcription {
        Transcription::Cloud => EngineInfo::Cloud(settings.provider),
        Transcription::Local => EngineInfo::Local(&settings.local_model),
    };
    report::render(&ReportInput {
        app_version: &env.app_version,
        macos_version: &env.macos_version,
        arch: &env.arch,
        microphone: access(permissions.0),
        accessibility: access(permissions.1),
        hotkey: &settings.hotkey,
        engine,
        engine_ready,
        errors,
        analytics_on: settings.share_usage_events,
    })
}

/// Reads whether the engine is ready, off the event loop.
async fn engine_state(
    config: &Arc<Config<KeychainSecretStore>>,
    live: &LiveEngine,
    downloads: &ModelDownloads,
    settings: &Settings,
) -> EngineState {
    match settings.transcription {
        Transcription::Local => {
            let download = downloads
                .list(live)
                .into_iter()
                .find(|model| model.id == settings.local_model)
                .map_or(ModelState::Missing, |model| model.state);
            let view = live
                .current_engine()
                .map_or(EngineView::Other, |engine| EngineView::of(&engine));
            EngineState::Local(local_state(view, download))
        }
        Transcription::Cloud => {
            let (config, live, provider) = (Arc::clone(config), live.clone(), settings.provider);
            let read =
                tauri::async_runtime::spawn_blocking(move || live.read_key(&config, provider))
                    .await;
            EngineState::Cloud(match read {
                Ok(Ok(Some(_))) => KeyState::Saved,
                Ok(Ok(None)) => KeyState::Missing,
                _ => KeyState::Unreadable,
            })
        }
    }
}

/// The live checks. Reads permissions and the key; records nothing and
/// contacts nobody.
#[tauri::command]
#[specta::specta]
pub async fn run_checks(
    config: State<'_, AppConfig>,
    live: State<'_, LiveEngine>,
    downloads: State<'_, ModelDownloads>,
    permissions: State<'_, OsPermissions>,
    diagnostics: State<'_, Diagnostics>,
) -> Result<Vec<Check>, String> {
    let settings = config.0.settings().unwrap_or_default();
    let engine = engine_state(&config.0, live.inner(), downloads.inner(), &settings).await;
    let os = Arc::clone(&permissions.0);
    let (microphone, accessibility) = tauri::async_runtime::spawn_blocking(move || {
        (
            os.check(dictation_ports::Permission::Microphone),
            os.check(dictation_ports::Permission::Accessibility),
        )
    })
    .await
    .map_err(|_| "the check task crashed".to_owned())?;
    let input = CheckInput {
        microphone,
        accessibility,
        hotkey_up: !diagnostics.0.hotkey_down.load(Ordering::Relaxed),
        engine,
        last_error: lock(&diagnostics.0.log).last(),
    };
    Ok(checks(&input))
}

/// What "Test connection" found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct ConnectionResult {
    /// Whether the provider answered and accepted the key.
    pub ok: bool,
    /// The error code if not.
    pub code: Option<ErrorCode>,
    /// One plain sentence.
    pub message: String,
}

/// The code for a failed key test.
#[must_use]
pub fn connection_code(kind: KeyTestKind) -> ErrorCode {
    match kind {
        KeyTestKind::NoKey => ErrorCode::KeyMissing,
        KeyTestKind::InvalidKey => ErrorCode::KeyRejected,
        KeyTestKind::RateLimited => ErrorCode::ProviderQuota,
        KeyTestKind::Network => ErrorCode::NetworkUnreachable,
        KeyTestKind::Server => ErrorCode::EngineFailed,
        KeyTestKind::Keychain => ErrorCode::KeyUnreadable,
    }
}

/// Asks the selected provider whether it answers and accepts the saved key,
/// with a short silent clip (the key test). Only when the user presses the
/// button; never for the on-device engine.
#[tauri::command]
#[specta::specta]
pub async fn test_connection(
    config: State<'_, AppConfig>,
    live: State<'_, LiveEngine>,
    diagnostics: State<'_, Diagnostics>,
) -> Result<ConnectionResult, String> {
    let settings = config.0.settings().unwrap_or_default();
    if settings.transcription == Transcription::Local {
        return Ok(ConnectionResult {
            ok: true,
            code: None,
            message: "The on-device engine needs no connection.".to_owned(),
        });
    }
    let base_url = live.provider_base_url(settings.provider);
    let result =
        commands::test_saved_key(&config.0, live.inner(), settings.provider, &base_url).await;
    Ok(match result {
        Ok(()) => ConnectionResult {
            ok: true,
            code: None,
            message: "The provider answered and accepted your key.".to_owned(),
        },
        Err(error) => {
            let code = connection_code(error.kind);
            lock(&diagnostics.0.log).record_code(code, now());
            ConnectionResult {
                ok: false,
                code: Some(code),
                message: format!("{} {}", code.info().message, code.info().next_step),
            }
        }
    })
}

/// Builds the report and remembers it as the one on screen.
#[tauri::command]
#[specta::specta]
pub async fn build_report(
    config: State<'_, AppConfig>,
    live: State<'_, LiveEngine>,
    downloads: State<'_, ModelDownloads>,
    permissions: State<'_, OsPermissions>,
    diagnostics: State<'_, Diagnostics>,
) -> Result<String, String> {
    let settings = config.0.settings().unwrap_or_default();
    let engine = engine_state(&config.0, live.inner(), downloads.inner(), &settings).await;
    let os = Arc::clone(&permissions.0);
    let version = env!("CARGO_PKG_VERSION").to_owned();
    let (env, microphone, accessibility) = tauri::async_runtime::spawn_blocking(move || {
        (
            Environment::read(&version),
            os.check(dictation_ports::Permission::Microphone),
            os.check(dictation_ports::Permission::Accessibility),
        )
    })
    .await
    .map_err(|_| "the report task crashed".to_owned())?;
    let text = report_text(
        &env,
        &settings,
        (microphone, accessibility),
        Some(engine.problem().is_none()),
        &diagnostics.recent(),
    );
    *lock(&diagnostics.0.shown_report) = Some(text.clone());
    Ok(text)
}

/// Asks where to save the report on screen and writes it there. `None` if
/// the user cancelled. The text is the one [`build_report`] returned.
#[tauri::command]
#[specta::specta]
pub async fn save_report(diagnostics: State<'_, Diagnostics>) -> Result<Option<String>, String> {
    let text = lock(&diagnostics.0.shown_report)
        .clone()
        .ok_or_else(|| "There is no report to save yet.".to_owned())?;
    let app = lock(&diagnostics.0.app)
        .clone()
        .ok_or_else(|| "The save dialog isn't ready yet.".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_file_name("dictation-app-report.txt")
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = path
            .into_path()
            .map_err(|_| "That location can't be written to.".to_owned())?;
        std::fs::write(&path, text).map_err(|error| {
            tracing::warn!(%error, "couldn't save the report");
            "Couldn't save the report there. Try another folder.".to_owned()
        })?;
        // The user chose the place, so naming the file is fine; the path is
        // not logged.
        Ok(path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|_| "the save task crashed".to_owned())?
}

/// Opens the browser at a new GitHub issue whose body is the report on
/// screen. The user reads it there and submits it; the app sends nothing.
#[tauri::command]
#[specta::specta]
pub async fn open_issue(
    diagnostics: State<'_, Diagnostics>,
    opener: State<'_, AppOpener>,
) -> Result<(), String> {
    let text = lock(&diagnostics.0.shown_report)
        .clone()
        .ok_or_else(|| "There is no report to open yet.".to_owned())?;
    let url = report::issue_url(&text);
    let opener = opener.0.clone();
    tauri::async_runtime::spawn_blocking(move || opener.open_url(&url))
        .await
        .map_err(|_| "the browser task crashed".to_owned())?
        .map_err(|error| {
            tracing::warn!(%error, "couldn't open the browser");
            "Couldn't open the browser. Use Copy and paste the report into a new issue.".to_owned()
        })
}

/// Whether the tray asked for Troubleshoot since the window last asked;
/// asking clears it.
#[tauri::command]
#[specta::specta]
pub async fn take_troubleshoot_request(
    diagnostics: State<'_, Diagnostics>,
) -> Result<bool, String> {
    Ok(diagnostics
        .0
        .wants_troubleshoot
        .swap(false, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use dictation_core::diagnostics::ErrorCode;
    use dictation_core::error::Permission;
    use dictation_core::redact;

    use super::*;

    fn input() -> CheckInput {
        CheckInput {
            microphone: PermissionStatus::Granted,
            accessibility: PermissionStatus::Granted,
            hotkey_up: true,
            engine: EngineState::Cloud(KeyState::Saved),
            last_error: None,
        }
    }

    fn by_id(checks: &[Check], id: CheckId) -> &Check {
        checks.iter().find(|check| check.id == id).unwrap()
    }

    #[test]
    fn a_healthy_setup_passes_every_check() {
        let rows = checks(&input());
        assert_eq!(rows.len(), 5);
        assert!(
            rows.iter().all(|row| row.ok && row.fix.is_none()),
            "{rows:?}"
        );
    }

    #[test]
    fn a_denied_microphone_fails_with_its_fix() {
        let mut data = input();
        data.microphone = PermissionStatus::Denied;
        let rows = checks(&data);
        let mic = by_id(&rows, CheckId::Microphone);
        assert!(!mic.ok);
        assert_eq!(mic.code, Some(ErrorCode::MicDenied));
        assert_eq!(mic.fix, Some(FixAction::OpenMicrophoneSettings));
        assert_eq!(mic.fix_label.as_deref(), Some("Open Microphone Settings"));
        assert!(mic.detail.contains("Microphone is off"), "{}", mic.detail);
        assert!(by_id(&rows, CheckId::Accessibility).ok);
    }

    #[test]
    fn the_hotkey_needs_accessibility_and_a_working_registration() {
        let mut data = input();
        data.hotkey_up = false;
        let rows = checks(&data);
        let hotkey = by_id(&rows, CheckId::Hotkey);
        assert_eq!(hotkey.code, Some(ErrorCode::HotkeyUnregistered));
        assert_eq!(hotkey.fix, Some(FixAction::OpenAccessibilitySettings));

        let mut data = input();
        data.accessibility = PermissionStatus::NotDetermined;
        let rows = checks(&data);
        assert!(!by_id(&rows, CheckId::Accessibility).ok);
        assert!(!by_id(&rows, CheckId::Hotkey).ok);
    }

    #[test]
    fn the_engine_check_names_what_is_missing() {
        let cases = [
            (EngineState::Cloud(KeyState::Missing), ErrorCode::KeyMissing),
            (
                EngineState::Cloud(KeyState::Unreadable),
                ErrorCode::KeyUnreadable,
            ),
            (
                EngineState::Local(LocalState::Missing),
                ErrorCode::ModelMissing,
            ),
            (
                EngineState::Local(LocalState::Downloading),
                ErrorCode::ModelMissing,
            ),
            (
                EngineState::Local(LocalState::Loading),
                ErrorCode::ModelLoading,
            ),
            (
                EngineState::Local(LocalState::NotLoaded),
                ErrorCode::ModelLoading,
            ),
            (
                EngineState::Local(LocalState::LoadFailed),
                ErrorCode::ModelLoadingFailed,
            ),
        ];
        for (engine, code) in cases {
            let mut data = input();
            data.engine = engine.clone();
            let rows = checks(&data);
            let row = by_id(&rows, CheckId::Engine);
            assert!(!row.ok, "{engine:?}");
            assert_eq!(row.code, Some(code), "{engine:?}");
        }
        let mut data = input();
        data.engine = EngineState::Local(LocalState::Loaded);
        assert!(by_id(&checks(&data), CheckId::Engine).ok);
    }

    /// E1 round 1: a model file that is on disk but will not load is not
    /// "Ready", in the check or in the report.
    #[test]
    fn a_present_model_that_fails_to_load_is_not_ready() {
        let failed = Engine::Unavailable(TranscribeError::Engine(
            "couldn't load the on-device model: out of memory".into(),
        ));
        let state = local_state(EngineView::of(&failed), ModelState::Ready);
        assert_eq!(state, LocalState::LoadFailed);
        let mut data = input();
        data.engine = EngineState::Local(state);
        let rows = checks(&data);
        let row = by_id(&rows, CheckId::Engine);
        assert!(!row.ok);
        assert_eq!(row.code, Some(ErrorCode::ModelLoadingFailed));
        assert_eq!(row.fix, Some(FixAction::OpenSettings));
        assert!(row.detail.contains("another model"), "{}", row.detail);
        assert!(data.engine.problem().is_some());
    }

    #[test]
    fn a_model_that_is_loading_is_not_ready_yet() {
        let loading = Engine::Unavailable(TranscribeError::ModelLoading);
        assert_eq!(
            local_state(EngineView::of(&loading), ModelState::Ready),
            LocalState::Loading
        );
        // Present on disk, engine not built for it yet.
        assert_eq!(
            local_state(EngineView::Other, ModelState::Ready),
            LocalState::NotLoaded
        );
        assert_eq!(
            local_state(EngineView::Other, ModelState::Partial),
            LocalState::Missing
        );
        assert_eq!(
            local_state(EngineView::Loaded, ModelState::Downloading),
            LocalState::Downloading
        );
        assert_eq!(
            local_state(EngineView::Loaded, ModelState::Ready),
            LocalState::Loaded
        );
        let missing = Engine::Unavailable(TranscribeError::ModelMissing);
        assert_eq!(
            local_state(EngineView::of(&missing), ModelState::Ready),
            LocalState::Missing
        );
    }

    /// The report says `engine_ready: no` for a model that cannot load.
    #[test]
    fn the_report_says_no_for_a_model_that_cannot_load() {
        let settings = Settings {
            transcription: Transcription::Local,
            ..Settings::default()
        };
        let env = Environment {
            app_version: "0.1.0".into(),
            macos_version: "15.1".into(),
            arch: "aarch64".into(),
        };
        let ready = EngineState::Local(LocalState::LoadFailed)
            .problem()
            .is_none();
        let text = report_text(
            &env,
            &settings,
            (PermissionStatus::Granted, PermissionStatus::Granted),
            Some(ready),
            &[],
        );
        assert!(text.contains("engine_ready: no"), "{text}");
    }

    #[test]
    fn the_last_error_shows_its_code_and_time() {
        let mut data = input();
        data.last_error = Some(LoggedError {
            code: ErrorCode::KeyRejected,
            at: 1_793_700_672,
        });
        let rows = checks(&data);
        let last = by_id(&rows, CheckId::LastError);
        assert!(!last.ok);
        assert!(
            last.detail
                .starts_with("key_rejected at 2026-11-03T10:11:12Z")
        );
        assert_eq!(last.fix, Some(FixAction::OpenSettings));
    }

    #[test]
    fn a_failed_key_test_has_a_code_for_every_kind() {
        use KeyTestKind::{InvalidKey, Keychain, Network, NoKey, RateLimited, Server};
        let codes: Vec<ErrorCode> = [NoKey, InvalidKey, RateLimited, Network, Server, Keychain]
            .into_iter()
            .map(connection_code)
            .collect();
        assert_eq!(
            codes,
            [
                ErrorCode::KeyMissing,
                ErrorCode::KeyRejected,
                ErrorCode::ProviderQuota,
                ErrorCode::NetworkUnreachable,
                ErrorCode::EngineFailed,
                ErrorCode::KeyUnreadable,
            ]
        );
    }

    #[test]
    fn diagnostics_keep_codes_and_the_hotkey_state_only() {
        let diagnostics = Diagnostics::default();
        diagnostics.record_error(&DictationError::HotkeyFailed);
        assert!(diagnostics.0.hotkey_down.load(Ordering::Relaxed));
        diagnostics.hotkey_restored();
        assert!(!diagnostics.0.hotkey_down.load(Ordering::Relaxed));
        diagnostics.record_error(&DictationError::PermissionMissing(Permission::Microphone));
        let recent = diagnostics.recent();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[1].code, ErrorCode::MicDenied);
        assert!(!diagnostics.0.wants_troubleshoot.load(Ordering::Relaxed));
        diagnostics.ask_for_troubleshoot();
        assert!(
            diagnostics
                .0
                .wants_troubleshoot
                .swap(false, Ordering::Relaxed)
        );
    }

    // E1 criterion 4. Everything that could reach the report is planted
    // with a secret or private text, and none of it may appear.
    #[test]
    fn criterion_4_planted_secrets_and_text_never_reach_the_report() {
        let key = "gsk_PLANTED_KEY_0123456789abcdef";
        let registered = "registered-secret-value-9f8e7d";
        let transcript = "my password is hunter2 and the launch code is 4815162342";
        let clipboard = "clipboard: wire the money to account 99887766";
        let app_name = "Payroll Spreadsheet - Slack";
        let home = "/Users/alice";
        let log_line = "INFO shown to the user: Transcription failed: raw log line 31337";
        redact::register_secret(registered);

        // Settings: every text field the user or a hand edit controls.
        let settings = Settings {
            hotkey: format!("{home}/.ssh/{key}"),
            model: format!("{transcript} {key}"),
            language: Some(transcript.to_owned()),
            local_model: format!("{home}/Library/{registered}"),
            ..Settings::default()
        };
        let local = Settings {
            transcription: Transcription::Local,
            ..settings.clone()
        };

        // Errors: their reasons hold the transcript, keys, app names, paths
        // and a provider's body; only the code is kept.
        let diagnostics = Diagnostics::default();
        for error in [
            DictationError::TranscriptionFailed {
                reason: format!("{transcript} {key} {registered} {home}/x.wav {log_line}"),
            },
            DictationError::InsertFailed {
                reason: format!("{app_name} refused {clipboard}"),
            },
            DictationError::HotkeyFailed,
            DictationError::EmptyTranscript,
        ] {
            diagnostics.record_error(&error);
        }

        // The environment strings are read from the OS; a hostile value
        // (a user name in a version string) is dropped too.
        let env = Environment {
            app_version: format!("0.1.0 {home}"),
            macos_version: "15.1".to_owned(),
            arch: format!("arm64 {key}"),
        };

        for settings in [&settings, &local] {
            let text = report_text(
                &env,
                settings,
                (PermissionStatus::Denied, PermissionStatus::Granted),
                Some(false),
                &diagnostics.recent(),
            );
            for secret in [
                key,
                registered,
                transcript,
                "hunter2",
                "4815162342",
                clipboard,
                "99887766",
                app_name,
                "Payroll",
                "Slack",
                home,
                "alice",
                ".ssh",
                "x.wav",
                log_line,
                "raw log line",
                "31337",
                "wire the money",
            ] {
                assert!(!text.contains(secret), "{secret:?} leaked:\n{text}");
            }
            // And the shape: only the allowlisted lines are there.
            let allowed = [
                "dictation-app diagnostics report",
                "Built from a fixed list of fields.",
                "app_version: ",
                "macos_version: ",
                "arch: ",
                "microphone: ",
                "accessibility: ",
                "hotkey: ",
                "engine: ",
                "engine_ready: ",
                "analytics: ",
                "recent_errors:",
                "  - ",
            ];
            for line in text.lines().filter(|line| !line.is_empty()) {
                assert!(
                    allowed.iter().any(|prefix| line.starts_with(prefix)),
                    "unexpected line {line:?} in:\n{text}"
                );
            }
            // The errors are listed by code and time only.
            for line in text.lines().filter(|line| line.starts_with("  - ")) {
                let code = line.rsplit(' ').next().unwrap();
                assert!(
                    ErrorCode::ALL.iter().any(|known| known.as_str() == code),
                    "{line}"
                );
            }
            assert!(text.contains("engine_failed") && text.contains("paste_failed"));
        }
    }
}
