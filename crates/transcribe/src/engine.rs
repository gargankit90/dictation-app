//! The engine the app dictates with, rebuilt from the user's settings.
//!
//! [`Engine::from_settings`] turns the saved [`Settings`] (provider, model,
//! language) and the provider's API key into an [`OpenAiCompat`] engine with
//! the default [`RetryPolicy`](dictation_core::retry::RetryPolicy). When there
//! is no key (or the keychain can't be read) the engine is
//! [`Engine::Unavailable`]: every dictation fails at once with the reason,
//! which reaches the user, instead of the app failing to start or sending a
//! request that can't work.
//!
//! [`Transcribers`] is the handle the pipeline owns: a [`Transcriber`] that
//! forwards to whichever engine is current, so saving settings or a new key
//! swaps the engine without restarting the pipeline. A transcription already
//! running finishes with the engine it started with.
//!
//! Privacy: the key is read from the [`SecretStore`] by the caller and only
//! ever handed to [`OpenAiCompat`] (its `Authorization` header). Nothing here
//! logs or returns it.

use std::sync::{Arc, Mutex, PoisonError, RwLock};

use dictation_core::settings::Settings;
use dictation_ports::{Audio, Secret, SecretError, TranscribeError, Transcriber};

use crate::mock::MockTranscriber;
use crate::openai_compat::{Config, OpenAiCompat, Preset};

/// A speech-to-text engine the app can dictate with.
pub enum Engine {
    /// Fixed text, no network (`--transcriber mock`).
    Mock(MockTranscriber),
    /// Groq or OpenAI.
    Remote(Box<OpenAiCompat>),
    /// No engine can be built; every dictation fails with this error.
    Unavailable(TranscribeError),
}

impl Engine {
    /// The engine for `settings`, with `key` as read from the keychain
    /// (`Ok(None)`: none is saved).
    #[must_use]
    pub fn from_settings(settings: &Settings, key: Result<Option<Secret>, SecretError>) -> Self {
        Self::build(settings, key, None)
    }

    /// As [`from_settings`](Self::from_settings), but sending requests to
    /// `base_url` instead of the provider's own endpoint (tests point it at
    /// a local server).
    #[must_use]
    pub fn from_settings_at(
        settings: &Settings,
        key: Result<Option<Secret>, SecretError>,
        base_url: &str,
    ) -> Self {
        Self::build(settings, key, Some(base_url))
    }

    fn build(
        settings: &Settings,
        key: Result<Option<Secret>, SecretError>,
        base_url: Option<&str>,
    ) -> Self {
        let key = match key {
            Ok(Some(key)) => key,
            Ok(None) => return Self::Unavailable(TranscribeError::MissingKey),
            Err(error) => {
                // SecretError's text never holds a secret.
                return Self::Unavailable(TranscribeError::KeychainUnavailable(error.to_string()));
            }
        };
        let mut config = Config::from_preset(Preset::for_provider(settings.provider));
        config.model.clone_from(&settings.model);
        config.language.clone_from(&settings.language);
        if let Some(base_url) = base_url {
            base_url.clone_into(&mut config.base_url);
        }
        match OpenAiCompat::new(config, key) {
            Ok(engine) => Self::Remote(Box::new(engine)),
            Err(error) => Self::Unavailable(error),
        }
    }

    /// A short name for logs; never the key.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Mock(_) => "mock",
            Self::Remote(_) => "remote",
            Self::Unavailable(_) => "unavailable",
        }
    }
}

impl Transcriber for Engine {
    async fn transcribe(&self, audio: &Audio) -> Result<String, TranscribeError> {
        match self {
            Self::Mock(engine) => engine.transcribe(audio).await,
            Self::Remote(engine) => engine.transcribe(audio).await,
            Self::Unavailable(error) => Err(error.clone()),
        }
    }
}

/// A [`Transcriber`] whose engine can be swapped while the pipeline runs.
/// Clones share the engine: give one to the pipeline and keep one to call
/// [`update`](Self::update) when the settings or the key change.
#[derive(Clone)]
pub struct Transcribers {
    current: Arc<RwLock<Arc<Engine>>>,
    /// Serializes [`update`](Self::update), so two rebuilds that read the
    /// settings at different times can't finish in the wrong order.
    updating: Arc<Mutex<()>>,
}

impl Transcribers {
    /// Starts with `engine`.
    #[must_use]
    pub fn new(engine: Engine) -> Self {
        Self {
            current: Arc::new(RwLock::new(Arc::new(engine))),
            updating: Arc::new(Mutex::new(())),
        }
    }

    /// Builds the next engine with `build` and makes it current. Concurrent
    /// updates run one after the other. `build` may block (it reads the
    /// keychain); a dictation in progress isn't held up by it.
    pub fn update(&self, build: impl FnOnce() -> Engine) {
        let _updating = self.updating.lock().unwrap_or_else(PoisonError::into_inner);
        let engine = Arc::new(build());
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = engine;
    }

    /// The current engine's [`name`](Engine::name).
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.snapshot().name()
    }

    fn snapshot(&self) -> Arc<Engine> {
        Arc::clone(&self.current.read().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Transcriber for Transcribers {
    async fn transcribe(&self, audio: &Audio) -> Result<String, TranscribeError> {
        // Clone the Arc out first: the lock is never held across the await.
        let engine = self.snapshot();
        engine.transcribe(audio).await
    }
}

impl std::fmt::Debug for Transcribers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transcribers")
            .field("engine", &self.name())
            .finish()
    }
}
