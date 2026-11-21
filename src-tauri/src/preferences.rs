//! The commands U6's Settings needs besides the ones it shares with
//! onboarding: every control saves on change through a command that writes
//! only its own field ([`set_transcription`]), like K1's `set_hotkey`, A1's
//! `set_share_usage_events` and S1's `set_launch_at_login`; so there is no
//! form whose older values could overwrite another control's newer one.
//!
//! Also: which engine is in use and whether it loaded ([`get_engine_status`]),
//! Remove key ([`delete_api_key`]), the links Settings opens ([`open_link`], a
//! closed list, so the page can't open any URL), the version for About, and
//! "Check for updates" with when it last ran.

use std::sync::Arc;
use std::time::UNIX_EPOCH;

use dictation_core::local_models;
use dictation_core::settings::Transcription;
use dictation_pipeline::config::Config;
use dictation_pipeline::hotkey_setting::HotkeyControl;
use dictation_ports::{SecretStore, TranscribeError};
use dictation_transcribe::engine::Engine;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::{self, ActiveHotkey, AppConfig, Saved};
use crate::diagnostics::AppOpener;
use crate::engine::LiveEngine;
use crate::onboarding::Onboarding;
use crate::usage::{self, Usage};

/// What a one-field write did: the usage change (names only) and what
/// happened to the hotkey on the way.
type Changed = (Option<usage::SettingsChange>, Saved);

/// Runs `change` on the blocking pool and records which setting changed (never
/// its value). An error means this setting wasn't saved. None of these writes
/// asks for another hotkey, so the hotkey's state isn't their failure: a hotkey
/// waiting for Accessibility must not stop setup from choosing an engine (U9).
/// Settings shows that from `get_hotkey_notice`.
async fn write<S: SecretStore + 'static>(
    config: Arc<Config<S>>,
    usage: &Usage,
    change: impl FnOnce(&Arc<Config<S>>) -> Result<Changed, String> + Send + 'static,
) -> Result<(), String> {
    let (change, _hotkey) = tauri::async_runtime::spawn_blocking(move || change(&config))
        .await
        .map_err(|_| "the settings task crashed".to_owned())?
        .map_err(|error| {
            tracing::warn!(%error, "settings change failed");
            error.to_string()
        })?;
    if let Some(change) = change {
        usage::settings_saved(usage, change);
    }
    Ok(())
}

/// Dictates in the cloud (`OpenRouter` with the user's own key) or on this
/// Mac from now on.
///
/// On this Mac with the model downloaded, it is loaded first
/// ([`LiveEngine::preload_local`]) and the settings choose it only if that
/// worked, so until then the engine in use stays in use. Takes seconds;
/// Settings shows "Loading…" meanwhile. Not downloaded yet (U8's "Download
/// and continue"), the settings choose it at once, so the choice survives a
/// quit and the model becomes the engine as soon as its download finishes;
/// until then the engine says the model isn't ready.
#[tauri::command]
#[specta::specta]
pub async fn set_transcription(
    state: State<'_, AppConfig>,
    engine: State<'_, LiveEngine>,
    hotkey: State<'_, ActiveHotkey>,
    usage: State<'_, Usage>,
    transcription: Transcription,
) -> Result<(), String> {
    let (live, control) = (engine.inner().clone(), hotkey.0.clone());
    write(Arc::clone(&state.0), &usage, move |config| {
        switch(config, &live, &control, transcription)
    })
    .await
}

fn switch<S: SecretStore + 'static>(
    config: &Arc<Config<S>>,
    live: &LiveEngine,
    control: &HotkeyControl,
    transcription: Transcription,
) -> Result<Changed, String> {
    if transcription == Transcription::Local {
        match live.preload_local() {
            Ok(()) | Err(TranscribeError::ModelMissing) => {}
            Err(error) => {
                // Whatever the settings still choose becomes current again.
                live.rebuild(config);
                return Err(format!(
                    "Couldn't load {}: {}",
                    local_models::DEFAULT.name,
                    error.user_message()
                ));
            }
        }
    }
    commands::write_change(config, live, control, |settings| {
        settings.transcription = transcription;
    })
    .map_err(|error| {
        // The model may already be live (from another on-device engine it
        // is published as it loads). The settings weren't written, so what
        // they still choose becomes the engine again (review round 1).
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
pub fn get_engine_status(
    engine: State<'_, LiveEngine>,
    onboarding: State<'_, Arc<Onboarding>>,
    usage: State<'_, Usage>,
) -> Result<EngineStatus, String> {
    Ok(engine.current_engine().map_or(
        EngineStatus {
            state: EngineState::Ready,
            message: None,
        },
        |current| {
            usage::engine_status_read(&usage, &onboarding, &current);
            status_of(&current)
        },
    ))
}

/// Deletes the API key from the keychain (Remove key). Bounded
/// like saving one.
#[tauri::command]
#[specta::specta]
pub async fn delete_api_key(
    state: State<'_, AppConfig>,
    engine: State<'_, LiveEngine>,
) -> Result<(), String> {
    let (config, live) = (Arc::clone(&state.0), engine.inner().clone());
    tauri::async_runtime::spawn_blocking(move || live.delete_key(&config))
        .await
        .map_err(|_| "the settings task crashed".to_owned())?
        .map_err(|error| {
            tracing::warn!(%error, "removing the API key failed");
            error.to_string()
        })
}

/// A page Settings links to. A closed list: the window can't open any
/// other URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    /// `OpenRouter`'s page for adding credits.
    OpenrouterCredits,
    /// The privacy notice: what the app sends.
    Privacy,
    /// The source code.
    Repository,
    /// The FSL-1.1-ALv2 licence.
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
            Self::OpenrouterCredits => "https://openrouter.ai/settings/credits".to_owned(),
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

/// Starts the update check that says what it found in a dialog (Settings →
/// General). Managed state, so the command needn't name the runtime.
pub struct UpdateCheck(pub Arc<dyn Fn() + Send + Sync>);

/// Check now, in Settings → General: a dialog says what it found.
#[tauri::command]
#[specta::specta]
pub fn check_for_updates(check: State<'_, UpdateCheck>) -> Result<(), String> {
    (check.0)();
    Ok(())
}

#[cfg(test)]
mod tests;
