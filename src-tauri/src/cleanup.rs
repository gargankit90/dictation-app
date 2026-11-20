//! Keeps the on-device cleanup helper (Q4) running while it is wanted.
//!
//! It is wanted while on-device transcription is selected and the cleanup
//! model (Gemma 4 E2B) is downloaded. [`LocalCleanup::follow`] is called
//! with the settings whenever the engine is rebuilt: it starts the helper in
//! the background (the model's SHA-256 is checked first, then it loads in a
//! few seconds) or stops it. Dictations meanwhile get no cleaner and paste
//! the raw transcript. A start that finishes after the settings changed
//! again is thrown away, so a quick switch back and forth never leaves two
//! helpers running.
//!
//! Stopping is dropping the last handle (`LlamaServer`'s `Drop` closes its
//! supervisor's pipe). A cleanup in flight holds a handle until it ends,
//! which [`local_cleanup::TIMEOUT`](dictation_core::local_cleanup::TIMEOUT)
//! bounds. Quitting needs nothing here: the supervisor kills the helper when
//! the app's process ends, however it ends.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dictation_core::local_models::{self, LocalModel};
use dictation_core::settings::{Settings, Transcription};
use dictation_transcribe::llama::{ChatRunner, LlamaError, LlamaServer, LocalCleaner};
use dictation_transcribe::models::{DownloadError, ModelStore};

/// A boxed future, for the injected starter and spawner.
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Starts the helper with the model at a path: [`LlamaServer::start`] in
/// the app, a fake in tests.
pub type Starter<R> = Arc<dyn Fn(PathBuf) -> BoxFuture<Result<R, LlamaError>> + Send + Sync>;

/// Runs a task in the background: `tauri::async_runtime::spawn` in the app.
pub type Spawner = Arc<dyn Fn(BoxFuture<()>) + Send + Sync>;

/// Checks the downloaded model before it is loaded:
/// [`ModelStore::verified_path`] in the app.
pub type Check =
    Arc<dyn Fn(&ModelStore, &LocalModel) -> Result<Option<PathBuf>, DownloadError> + Send + Sync>;

/// The cleanup helper's keeper. Clones share it.
pub struct LocalCleanup<R = LlamaServer> {
    store: ModelStore,
    model: &'static LocalModel,
    starter: Starter<R>,
    spawn: Spawner,
    check: Check,
    state: Arc<Mutex<State<R>>>,
}

impl<R> Clone for LocalCleanup<R> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            model: self.model,
            starter: Arc::clone(&self.starter),
            spawn: Arc::clone(&self.spawn),
            check: Arc::clone(&self.check),
            state: Arc::clone(&self.state),
        }
    }
}

struct State<R> {
    /// Bumped on every change of what is wanted; a start of an older one is
    /// thrown away.
    generation: u64,
    /// The model file wanted, if any.
    wanted: Option<PathBuf>,
    /// The helper, once it has started.
    running: Option<Arc<R>>,
}

/// Where the helper is, for Settings and the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupState {
    /// Not wanted (cloud transcription, or the model isn't downloaded).
    Off,
    /// Wanted and starting.
    Starting,
    /// Running, with the model loaded.
    Ready,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl LocalCleanup<LlamaServer> {
    /// The app's keeper: `helper` (the bundled `llama-server`, or `None` in a
    /// build without it) runs the default cleanup model from `store`, and
    /// tasks run on Tauri's async runtime.
    #[must_use]
    pub fn new(store: ModelStore, helper: Option<PathBuf>) -> Self {
        let starter: Starter<LlamaServer> = Arc::new(move |model: PathBuf| {
            let helper = helper.clone();
            Box::pin(async move {
                let helper = helper.ok_or(LlamaError::HelperMissing)?;
                LlamaServer::start(&helper, &model).await
            })
        });
        let spawn: Spawner = Arc::new(|task| {
            tauri::async_runtime::spawn(task);
        });
        Self::with(store, starter, spawn)
    }
}

impl<R: ChatRunner + 'static> LocalCleanup<R> {
    /// A keeper with its own starter and spawner (tests).
    #[must_use]
    pub fn with(store: ModelStore, starter: Starter<R>, spawn: Spawner) -> Self {
        let model = local_models::find_cleanup(local_models::DEFAULT_CLEANUP_MODEL)
            .unwrap_or(&local_models::GEMMA_4_E2B);
        Self {
            store,
            model,
            starter,
            spawn,
            check: Arc::new(|store: &ModelStore, model: &LocalModel| store.verified_path(model)),
            state: Arc::new(Mutex::new(State {
                generation: 0,
                wanted: None,
                running: None,
            })),
        }
    }

    /// As [`with`](Self::with), with any file of the right size counting
    /// as verified, so tests can use sparse stand-ins.
    #[cfg(test)]
    #[must_use]
    pub fn trusting(store: ModelStore, starter: Starter<R>, spawn: Spawner) -> Self {
        Self {
            check: Arc::new(|store: &ModelStore, model: &LocalModel| Ok(store.ready_path(model))),
            ..Self::with(store, starter, spawn)
        }
    }

    /// The cleanup model this keeper runs.
    #[must_use]
    pub fn model(&self) -> &'static LocalModel {
        self.model
    }

    /// Starts or stops the helper for `settings`: running while on-device
    /// transcription is selected and the model is downloaded.
    pub fn follow(&self, settings: &Settings) {
        let wanted = match settings.transcription {
            Transcription::Local => self.store.ready_path(self.model),
            Transcription::Cloud => None,
        };
        let generation = {
            let mut state = lock(&self.state);
            if state.wanted == wanted {
                return;
            }
            state.generation += 1;
            state.wanted.clone_from(&wanted);
            // Dropping the handle stops the helper.
            state.running = None;
            state.generation
        };
        let Some(path) = wanted else {
            tracing::info!("on-device cleanup off");
            return;
        };
        let this = self.clone();
        (self.spawn)(Box::pin(async move { this.start(generation, path).await }));
    }

    /// Stops the helper (the cleanup model is being deleted); the next
    /// [`follow`](Self::follow) decides again.
    pub fn release(&self) {
        let mut state = lock(&self.state);
        state.generation += 1;
        state.wanted = None;
        state.running = None;
    }

    async fn start(&self, generation: u64, path: PathBuf) {
        let current = || lock(&self.state).generation == generation;
        let (check, store, model) = (Arc::clone(&self.check), self.store.clone(), self.model);
        let verified = tokio::task::spawn_blocking(move || check(&store, model)).await;
        let path = match verified {
            Ok(Ok(Some(verified))) if verified == path => verified,
            Ok(Ok(_) | Err(DownloadError::Checksum)) => {
                tracing::warn!(model = model.id, "cleanup model missing or damaged");
                return;
            }
            Ok(Err(error)) => {
                tracing::warn!(model = model.id, %error, "could not check the cleanup model");
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "cleanup model check did not finish");
                return;
            }
        };
        if !current() {
            return;
        }
        let started = std::time::Instant::now();
        match (self.starter)(path).await {
            Ok(server) => {
                let mut state = lock(&self.state);
                if state.generation == generation {
                    state.running = Some(Arc::new(server));
                    tracing::info!(model = model.id, elapsed = ?started.elapsed(), "on-device cleanup ready");
                }
                // Otherwise the settings moved on: `server` drops here and stops.
            }
            Err(error) => {
                tracing::warn!(model = model.id, %error, "on-device cleanup didn't start");
                // Off, so the next rebuild tries again; dictation meanwhile
                // pastes the raw transcript.
                let mut state = lock(&self.state);
                if state.generation == generation {
                    state.wanted = None;
                }
            }
        }
    }

    /// Where the helper is.
    #[must_use]
    pub fn state(&self) -> CleanupState {
        let state = lock(&self.state);
        match (&state.wanted, &state.running) {
            (None, _) => CleanupState::Off,
            (Some(_), None) => CleanupState::Starting,
            (Some(_), Some(_)) => CleanupState::Ready,
        }
    }

    /// A cleaner for the next dictation, while the helper is running.
    #[must_use]
    pub fn cleaner(&self) -> Option<LocalCleaner<Arc<R>>> {
        lock(&self.state)
            .running
            .as_ref()
            .map(|server| LocalCleaner::new(Arc::clone(server)))
    }

    /// The models folder.
    #[must_use]
    pub fn store(&self) -> &ModelStore {
        &self.store
    }
}

/// The bundled helper next to the app's executable, if this build has it.
#[must_use]
pub fn bundled_helper() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    dictation_transcribe::llama::helper_next_to(Path::new(&exe))
}

#[cfg(test)]
mod tests;
