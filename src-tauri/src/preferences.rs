//! The commands U6's Settings needs besides the ones it shares with
//! onboarding: every control saves on change through a command that writes
//! only its own field ([`set_language`], [`use_cloud_engine`],
//! [`use_local_model`]), like K1's `set_hotkey`, A1's
//! `set_share_usage_events` and S1's `set_launch_at_login`; so there is no
//! form whose older values could overwrite another control's newer one.
//!
//! Also: which engine is in use and whether it loaded ([`get_engine_status`]),
//! Remove key ([`delete_api_key`]), Test microphone
//! ([`start_microphone_test`]), the links Settings opens ([`open_link`], a
//! closed list, so the page can't open any URL), the version for About, and
//! "Check for updates" with when it last ran.

use std::sync::Arc;
use std::time::UNIX_EPOCH;

use dictation_core::local_models;
use dictation_core::settings::{Provider, Settings, Transcription};
use dictation_pipeline::config::Config;
use dictation_pipeline::hotkey_setting::HotkeyControl;
use dictation_pipeline::mic_test::{self, MicTest};
use dictation_ports::{AudioError, SecretStore, TranscribeError};
use dictation_transcribe::engine::Engine;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::{self, ActiveHotkey, AppConfig, Saved};
use crate::diagnostics::AppOpener;
use crate::engine::LiveEngine;
use crate::usage::{self, Usage};

/// What a one-field write did: the usage change (names only) and any
/// problem besides the write.
type Changed = (Option<usage::SettingsChange>, Saved);

/// Runs `change` on the blocking pool, records which setting changed (never
/// its value) and turns a problem into the message Settings shows.
async fn write<S: SecretStore + 'static>(
    config: Arc<Config<S>>,
    usage: &Usage,
    change: impl FnOnce(&Arc<Config<S>>) -> Result<Changed, String> + Send + 'static,
) -> Result<(), String> {
    let (change, saved) = tauri::async_runtime::spawn_blocking(move || change(&config))
        .await
        .map_err(|_| "the settings task crashed".to_owned())?
        .map_err(|error| {
            tracing::warn!(%error, "settings change failed");
            error.to_string()
        })?;
    if let Some(change) = change {
        usage::settings_saved(usage, change);
    }
    saved.problem().map_or(Ok(()), Err)
}

/// `language` as stored: trimmed, and `None` (detect it) when empty.
fn normalized(language: Option<String>) -> Option<String> {
    language
        .map(|code| code.trim().to_owned())
        .filter(|code| !code.is_empty())
}

/// Sets the language and nothing else (`None` or empty: detect it). The
/// engine follows at once.
#[tauri::command]
#[specta::specta]
pub async fn set_language(
    state: State<'_, AppConfig>,
    engine: State<'_, LiveEngine>,
    hotkey: State<'_, ActiveHotkey>,
    usage: State<'_, Usage>,
    language: Option<String>,
) -> Result<(), String> {
    let (live, control) = (engine.inner().clone(), hotkey.0.clone());
    write(Arc::clone(&state.0), &usage, move |config| {
        language_change(config, &live, &control, language)
    })
    .await
}

fn language_change<S: SecretStore + 'static>(
    config: &Arc<Config<S>>,
    live: &LiveEngine,
    control: &HotkeyControl,
    language: Option<String>,
) -> Result<Changed, String> {
    let language = normalized(language);
    commands::write_change(config, live, control, |settings| {
        settings.language = language;
    })
    .map_err(|error| error.to_string())
}

/// Dictates with `provider` and the user's own key from now on. Switching
/// provider also switches to its default model.
#[tauri::command]
#[specta::specta]
pub async fn use_cloud_engine(
    state: State<'_, AppConfig>,
    engine: State<'_, LiveEngine>,
    hotkey: State<'_, ActiveHotkey>,
    usage: State<'_, Usage>,
    provider: Provider,
) -> Result<(), String> {
    let (live, control) = (engine.inner().clone(), hotkey.0.clone());
    write(Arc::clone(&state.0), &usage, move |config| {
        commands::write_change(config, &live, &control, |settings| {
            choose_cloud(settings, provider);
        })
        .map_err(|error| error.to_string())
    })
    .await
}

fn choose_cloud(settings: &mut Settings, provider: Provider) {
    if settings.provider != provider {
        provider.default_model().clone_into(&mut settings.model);
    }
    settings.provider = provider;
    settings.transcription = Transcription::Cloud;
}

/// Dictates with on-device model `id` from now on, once it has loaded: the
/// model is loaded first ([`LiveEngine::preload_local`]) and the settings
/// choose it only if that worked, so until then the engine in use stays in
/// use (from another on-device model, it is reloaded if the new one fails).
/// Takes seconds; Settings shows "Loading…" meanwhile.
#[tauri::command]
#[specta::specta]
pub async fn use_local_model(
    state: State<'_, AppConfig>,
    engine: State<'_, LiveEngine>,
    hotkey: State<'_, ActiveHotkey>,
    usage: State<'_, Usage>,
    id: String,
) -> Result<(), String> {
    let (live, control) = (engine.inner().clone(), hotkey.0.clone());
    write(Arc::clone(&state.0), &usage, move |config| {
        switch_to_local(config, &live, &control, &id)
    })
    .await
}

fn switch_to_local<S: SecretStore + 'static>(
    config: &Arc<Config<S>>,
    live: &LiveEngine,
    control: &HotkeyControl,
    id: &str,
) -> Result<Changed, String> {
    let model =
        local_models::find(id).ok_or_else(|| format!("There is no on-device model {id:?}."))?;
    let language = config
        .settings()
        .map_err(|error| error.to_string())?
        .language;
    if let Err(error) = live.preload_local(model.id, language.as_deref()) {
        // Whatever the settings still choose becomes current again.
        live.rebuild(config);
        return Err(format!(
            "Couldn't load {}: {}",
            model.name,
            error.user_message()
        ));
    }
    commands::write_change(config, live, control, |settings| {
        settings.transcription = Transcription::Local;
        model.id.clone_into(&mut settings.local_model);
    })
    .map_err(|error| {
        // The new model may already be live (from another on-device model
        // it is published as it loads). The settings weren't written, so
        // what they still choose becomes the engine again (review round 1).
        live.rebuild(config);
        error.to_string()
    })
}

/// Where the engine in use is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    /// The on-device model is loading.
    Loading,
    /// It can dictate (for a cloud engine: it was built; the key is
    /// checked when used).
    Ready,
    /// It can't dictate; `message` says why.
    Failed,
}

/// The engine in use, for the Speech section's chips.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct EngineStatus {
    /// Where it is.
    pub state: EngineState,
    /// Why it can't dictate, in plain words, when it can't.
    pub message: Option<String>,
}

/// The status of `engine`.
#[must_use]
pub fn status_of(engine: &Engine) -> EngineStatus {
    match engine {
        Engine::Unavailable(TranscribeError::ModelLoading) => EngineStatus {
            state: EngineState::Loading,
            message: None,
        },
        Engine::Unavailable(error) => EngineStatus {
            state: EngineState::Failed,
            message: Some(error.user_message()),
        },
        Engine::Mock(_) | Engine::Remote(_) | Engine::Local(_) => EngineStatus {
            state: EngineState::Ready,
            message: None,
        },
    }
}

/// The engine in use: loading, ready, or failed and why. Never blocks.
#[tauri::command]
#[specta::specta]
pub fn get_engine_status(engine: State<'_, LiveEngine>) -> Result<EngineStatus, String> {
    Ok(engine.current_engine().map_or(
        EngineStatus {
            state: EngineState::Ready,
            message: None,
        },
        |current| status_of(&current),
    ))
}

/// Deletes `provider`'s API key from the keychain (Remove key). Bounded
/// like saving one.
#[tauri::command]
#[specta::specta]
pub async fn delete_api_key(
    state: State<'_, AppConfig>,
    engine: State<'_, LiveEngine>,
    provider: Provider,
) -> Result<(), String> {
    let (config, live) = (Arc::clone(&state.0), engine.inner().clone());
    tauri::async_runtime::spawn_blocking(move || live.delete_key(&config, provider))
        .await
        .map_err(|_| "the settings task crashed".to_owned())?
        .map_err(|error| {
            tracing::warn!(%error, "removing the API key failed");
            error.to_string()
        })
}

/// The microphone test, managed state.
pub struct AppMicTest(pub MicTest);

/// What the user is told when the test can't start.
fn microphone_problem(error: &AudioError) -> String {
    match error {
        AudioError::PermissionDenied => "Vocet isn't allowed to use the microphone. Allow it in \
             System Settings → Privacy & Security → Microphone."
            .to_owned(),
        AudioError::NoDevice => {
            "No microphone found. Connect one, or choose one in System Settings → Sound.".to_owned()
        }
        AudioError::AlreadyRecording | AudioError::NotRecording | AudioError::Device(_) => {
            "The microphone didn't start. Try again.".to_owned()
        }
    }
}

/// Starts recording from the microphone for a few seconds, only to show
/// its level ([`read_microphone_test`]). Ends by itself after
/// [`mic_test::MAX_TEST`].
#[tauri::command]
#[specta::specta]
pub async fn start_microphone_test(test: State<'_, AppMicTest>) -> Result<(), String> {
    let test = test.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let generation = test.start().map_err(|error| {
            tracing::warn!(%error, "the microphone test didn't start");
            microphone_problem(&error)
        })?;
        test.stop_after(generation, mic_test::MAX_TEST);
        Ok(())
    })
    .await
    .map_err(|_| "the microphone task crashed".to_owned())?
}

/// The microphone test's level and device, as Settings polls them.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct MicReading {
    /// The level since the last reading, in dBFS; `None` once the test
    /// has ended.
    pub level: Option<f32>,
    /// The input device's name, if known.
    pub device: Option<String>,
}

/// The test's level since the last call, and the device. Never blocks.
#[tauri::command]
#[specta::specta]
pub fn read_microphone_test(test: State<'_, AppMicTest>) -> Result<MicReading, String> {
    let reading = test.0.read();
    Ok(MicReading {
        level: reading.level,
        device: reading.device,
    })
}

/// Ends the microphone test.
#[tauri::command]
#[specta::specta]
pub fn stop_microphone_test(test: State<'_, AppMicTest>) -> Result<(), String> {
    test.0.stop();
    Ok(())
}

/// A page Settings links to. A closed list: the window can't open any
/// other URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    /// Groq's page for creating an API key.
    GroqKeys,
    /// `OpenAI`'s page for creating an API key.
    OpenaiKeys,
    /// The privacy notice: what the app sends.
    Privacy,
    /// The source code.
    Repository,
    /// The MIT licence.
    Licence,
    /// The credits for adapted code.
    Notice,
}

const REPOSITORY: &str = "https://github.com/gargankit90/dictation-app";

impl Link {
    /// Where it goes.
    #[must_use]
    pub fn url(self) -> String {
        match self {
            Self::GroqKeys => "https://console.groq.com/keys".to_owned(),
            Self::OpenaiKeys => "https://platform.openai.com/api-keys".to_owned(),
            Self::Privacy => format!("{REPOSITORY}/blob/main/docs/privacy.md"),
            Self::Repository => REPOSITORY.to_owned(),
            Self::Licence => format!("{REPOSITORY}/blob/main/LICENSE"),
            Self::Notice => format!("{REPOSITORY}/blob/main/NOTICE"),
        }
    }
}

/// Opens `link` in the browser.
#[tauri::command]
#[specta::specta]
pub async fn open_link(opener: State<'_, AppOpener>, link: Link) -> Result<(), String> {
    let opener = opener.0.clone();
    tauri::async_runtime::spawn_blocking(move || opener.open_url(&link.url()))
        .await
        .map_err(|_| "the browser task crashed".to_owned())?
        .map_err(|error| {
            tracing::warn!(%error, "couldn't open the browser");
            "Couldn't open the browser.".to_owned()
        })
}

/// What About and General show.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct AppInfo {
    /// This build's version.
    pub version: String,
    /// When this run of the app last heard from the update server, in
    /// milliseconds since 1970; `None` if it hasn't yet.
    pub last_update_check_ms: Option<f64>,
}

/// The version, and when updates were last checked.
#[tauri::command]
#[specta::specta]
pub fn get_app_info() -> Result<AppInfo, String> {
    #[allow(
        clippy::cast_precision_loss,
        reason = "milliseconds since 1970 fit a double exactly for millennia"
    )]
    let last_update_check_ms = crate::updater::last_checked()
        .and_then(|when| when.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_millis() as f64);
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        last_update_check_ms,
    })
}

/// Starts the tray's update check (a dialog says what it found). Managed
/// state, so the command needn't name the runtime.
pub struct UpdateCheck(pub Arc<dyn Fn() + Send + Sync>);

/// Checks for updates as the tray's item does: a dialog says what it found.
#[tauri::command]
#[specta::specta]
pub fn check_for_updates(check: State<'_, UpdateCheck>) -> Result<(), String> {
    (check.0)();
    Ok(())
}

#[cfg(test)]
mod tests;
