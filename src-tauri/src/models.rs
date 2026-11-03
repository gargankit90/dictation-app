//! The on-device model commands Settings calls (L1): list the catalogue
//! with each model's status, start or cancel a download, delete a model.
//!
//! A download runs in the background on the async runtime; the command
//! returns once it has started. Its progress (from the store's callbacks)
//! is kept in [`ModelDownloads`], and Settings polls
//! [`list_local_models`] while one runs, so no event channel or extra IPC
//! permission is needed. When a download finishes the engine is rebuilt, so
//! a model the settings already chose becomes the engine at once; deleting
//! the selected model rebuilds it into "download the model in Settings".
//!
//! Nothing here sends or logs audio, text or keys; the download goes only to
//! the catalogue's pinned Hugging Face URL.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dictation_core::local_models::{self, LocalModel};
use dictation_pipeline::config::Config;
use dictation_ports::SecretStore;
use dictation_transcribe::models::{DownloadCancel, DownloadError, ModelStatus, Progress};
use serde::Serialize;
use tauri::State;

use crate::commands::AppConfig;
use crate::engine::LiveEngine;

/// Where a model is, as Settings shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ModelState {
    /// Not on this Mac.
    Missing,
    /// A download stopped part way; downloading again continues it.
    Partial,
    /// Downloading now.
    Downloading,
    /// Downloaded and verified.
    Ready,
}

/// One catalogue model, for the picker.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct LocalModelInfo {
    /// Its id, saved as `local_model`.
    pub id: String,
    /// The name shown.
    pub name: String,
    /// Its languages in words ("English only").
    pub languages: String,
    /// The download's size in bytes (a number: sizes fit a double exactly).
    pub size_bytes: f64,
    /// Bytes on disk so far (the size when ready, 0 when missing).
    pub downloaded_bytes: f64,
    /// Where it is.
    pub state: ModelState,
    /// Why the last download failed, in plain words, if it did.
    pub error: Option<String>,
    /// Whether it is the catalogue's default.
    pub default: bool,
}

/// The downloads running now and the last failure of each model. Managed
/// Tauri state; clones share it.
#[derive(Clone, Default)]
pub struct ModelDownloads {
    inner: Arc<Mutex<Downloads>>,
}

#[derive(Default)]
struct Downloads {
    running: HashMap<&'static str, Running>,
    failed: HashMap<&'static str, String>,
}

struct Running {
    cancel: DownloadCancel,
    progress: Progress,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Why a second model can't start downloading.
pub const ONE_AT_A_TIME: &str =
    "Another model is downloading. Wait for it to finish, or cancel it, then try again.";

fn find(id: &str) -> Result<&'static LocalModel, String> {
    local_models::find(id).ok_or_else(|| format!("there is no on-device model {id:?}"))
}

fn no_local() -> String {
    "on-device models aren't available in this build".to_owned()
}

impl ModelDownloads {
    /// Every catalogue model with its status.
    #[must_use]
    pub fn list(&self, live: &LiveEngine) -> Vec<LocalModelInfo> {
        let downloads = lock(&self.inner);
        local_models::CATALOGUE
            .iter()
            .map(|model| {
                let (state, downloaded) = match downloads.running.get(model.id) {
                    Some(running) => (ModelState::Downloading, running.progress.downloaded),
                    None => match live.local().map(|local| local.store().status(model)) {
                        Some(ModelStatus::Ready) => (ModelState::Ready, model.size_bytes),
                        Some(ModelStatus::Partial { downloaded }) => {
                            (ModelState::Partial, downloaded)
                        }
                        Some(ModelStatus::Missing) | None => (ModelState::Missing, 0),
                    },
                };
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "model sizes are far below 2^53 bytes"
                )]
                LocalModelInfo {
                    id: model.id.to_owned(),
                    name: model.name.to_owned(),
                    languages: model.languages_label.to_owned(),
                    size_bytes: model.size_bytes as f64,
                    downloaded_bytes: downloaded as f64,
                    state,
                    error: downloads.failed.get(model.id).cloned(),
                    default: model.id == local_models::DEFAULT_MODEL,
                }
            })
            .collect()
    }

    /// Starts downloading `id` in the background, unless it is already
    /// downloading. When it finishes, `done` runs (the app rebuilds the
    /// engine there).
    ///
    /// One model downloads at a time: each download checks the free space
    /// for itself alone, so two at once could together fill the disk.
    ///
    /// # Errors
    ///
    /// An unknown id, a build without on-device models, or another model
    /// downloading.
    pub fn start(
        &self,
        live: &LiveEngine,
        id: &str,
        url: Option<String>,
        done: impl FnOnce(Result<(), DownloadError>) + Send + 'static,
    ) -> Result<(), String> {
        let model = find(id)?;
        let store = live.local().ok_or_else(no_local)?.store().clone();
        let cancel = DownloadCancel::new();
        {
            let mut downloads = lock(&self.inner);
            if downloads.running.contains_key(model.id) {
                return Ok(());
            }
            if !downloads.running.is_empty() {
                return Err(ONE_AT_A_TIME.to_owned());
            }
            downloads.failed.remove(model.id);
            downloads.running.insert(
                model.id,
                Running {
                    cancel: cancel.clone(),
                    progress: Progress {
                        downloaded: 0,
                        total: model.size_bytes,
                    },
                },
            );
        }
        let shared = Arc::clone(&self.inner);
        tauri::async_runtime::spawn(async move {
            let progress = |progress: Progress| {
                if let Some(running) = lock(&shared).running.get_mut(model.id) {
                    running.progress = progress;
                }
            };
            let url = url.unwrap_or_else(|| model.url.to_owned());
            let result = store
                .download_from(model, &url, &cancel, progress)
                .await
                .map(|_| ());
            {
                let mut downloads = lock(&shared);
                downloads.running.remove(model.id);
                match &result {
                    Err(DownloadError::Cancelled) | Ok(()) => {}
                    Err(error) => {
                        tracing::warn!(model = model.id, %error, "model download failed");
                        downloads.failed.insert(model.id, error.user_message());
                    }
                }
            }
            done(result);
        });
        Ok(())
    }

    /// Cancels `id`'s download, if one runs. What was fetched stays, so the
    /// next download continues it.
    pub fn cancel(&self, id: &str) {
        if let Some(running) = lock(&self.inner).running.get(id) {
            running.cancel.cancel();
        }
    }

    /// Whether `id` is downloading.
    #[must_use]
    pub fn is_running(&self, id: &str) -> bool {
        lock(&self.inner).running.contains_key(id)
    }

    /// Deletes `id` (and any part), after cancelling its download, and lets
    /// go of it if it is the loaded model.
    ///
    /// # Errors
    ///
    /// An unknown id, a build without on-device models, or a file that
    /// can't be removed.
    pub fn delete(&self, live: &LiveEngine, id: &str) -> Result<(), String> {
        let model = find(id)?;
        let local = live.local().ok_or_else(no_local)?;
        self.cancel(id);
        if local.loaded() == Some(model.id) {
            local.release();
        }
        lock(&self.inner).failed.remove(model.id);
        local
            .store()
            .delete(model)
            .map_err(|error| error.user_message())
    }
}

/// Rebuilds the engine on the blocking pool (loading a model takes seconds).
fn rebuild_later<S: SecretStore + 'static>(config: Arc<Config<S>>, live: LiveEngine) {
    tauri::async_runtime::spawn_blocking(move || live.rebuild(&config));
}

/// Every on-device model with its status and download progress.
#[tauri::command]
#[specta::specta]
pub async fn list_local_models(
    engine: State<'_, LiveEngine>,
    downloads: State<'_, ModelDownloads>,
) -> Result<Vec<LocalModelInfo>, String> {
    Ok(downloads.list(engine.inner()))
}

/// Starts downloading model `id`; poll [`list_local_models`] for progress.
#[tauri::command]
#[specta::specta]
pub async fn download_local_model(
    state: State<'_, AppConfig>,
    engine: State<'_, LiveEngine>,
    downloads: State<'_, ModelDownloads>,
    id: String,
) -> Result<(), String> {
    let (config, live) = (Arc::clone(&state.0), engine.inner().clone());
    downloads.start(engine.inner(), &id, None, move |result| {
        if result.is_ok() {
            rebuild_later(config, live);
        }
    })
}

/// Cancels model `id`'s download; what was fetched is kept.
#[tauri::command]
#[specta::specta]
pub async fn cancel_local_model_download(
    downloads: State<'_, ModelDownloads>,
    id: String,
) -> Result<(), String> {
    downloads.cancel(&id);
    Ok(())
}

/// Deletes model `id` from this Mac and rebuilds the engine.
#[tauri::command]
#[specta::specta]
pub async fn delete_local_model(
    state: State<'_, AppConfig>,
    engine: State<'_, LiveEngine>,
    downloads: State<'_, ModelDownloads>,
    id: String,
) -> Result<(), String> {
    downloads.delete(engine.inner(), &id)?;
    rebuild_later(Arc::clone(&state.0), engine.inner().clone());
    Ok(())
}

#[cfg(test)]
mod tests;
