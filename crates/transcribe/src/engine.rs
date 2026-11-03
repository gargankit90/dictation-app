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
//! The on-device engine (L1) is [`Engine::Local`]: the shell loads the model
//! (`crate::local`) and keeps it, so a rebuild for an unrelated setting
//! reuses the loaded model instead of reading the file again.
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
use dictation_core::text::Transcript;
use dictation_ports::{Audio, Secret, SecretError, TranscribeError, Transcriber};

use dictation_core::local_models::LocalModel;

use crate::local::LocalTranscriber;
use crate::mock::MockTranscriber;
use crate::openai_compat::{Config, OpenAiCompat, Preset};

/// A speech-to-text engine the app can dictate with.
pub enum Engine {
    /// Fixed text, no network (`--transcriber mock`).
    Mock(MockTranscriber),
    /// Groq or OpenAI.
    Remote(Box<OpenAiCompat>),
    /// A downloaded model on this Mac (L1).
    Local(LocalTranscriber),
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
            Self::Local(_) => "local",
            Self::Unavailable(_) => "unavailable",
        }
    }
}

impl Transcriber for Engine {
    async fn transcribe(&self, audio: &Audio) -> Result<String, TranscribeError> {
        match self {
            Self::Mock(engine) => engine.transcribe(audio).await,
            Self::Remote(engine) => engine.transcribe(audio).await,
            Self::Local(engine) => engine.transcribe(audio).await,
            Self::Unavailable(error) => Err(error.clone()),
        }
    }

    // Forwarded, so the engine's no_speech_prob reaches the pipeline's
    // hallucination filter.
    async fn transcribe_detailed(&self, audio: &Audio) -> Result<Transcript, TranscribeError> {
        match self {
            Self::Mock(engine) => engine.transcribe_detailed(audio).await,
            Self::Remote(engine) => engine.transcribe_detailed(audio).await,
            Self::Local(engine) => engine.transcribe_detailed(audio).await,
            Self::Unavailable(error) => Err(error.clone()),
        }
    }
}

/// The language hint for `model`: the saved language if the model speaks
/// it, else `None` (its default). Parakeet is English only, so a saved
/// `de` must not reach it.
#[must_use]
pub fn local_language(model: &LocalModel, language: Option<&str>) -> Option<String> {
    language
        .filter(|code| model.speaks(code))
        .map(str::to_owned)
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

    /// Like [`update`](Self::update), but `build` may make an engine current
    /// before it returns, through the function it is given: the local
    /// engine puts "the model is loading" in place of the old model before
    /// loading the next, so the old one is freed first (two models at once
    /// can need gigabytes). Dictations already running keep the engine they
    /// started with.
    pub fn update_staged(&self, build: impl FnOnce(&dyn Fn(Engine)) -> Engine) {
        let _updating = self.updating.lock().unwrap_or_else(PoisonError::into_inner);
        let stage = |engine: Engine| {
            let previous = std::mem::replace(
                &mut *self.current.write().unwrap_or_else(PoisonError::into_inner),
                Arc::new(engine),
            );
            // Dropped here, outside the lock.
            drop(previous);
        };
        let engine = Arc::new(build(&stage));
        let previous = std::mem::replace(
            &mut *self.current.write().unwrap_or_else(PoisonError::into_inner),
            engine,
        );
        drop(previous);
    }

    /// The current engine, for a rebuild that wants to reuse its loaded
    /// local model.
    #[must_use]
    pub fn current(&self) -> Arc<Engine> {
        self.snapshot()
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

    async fn transcribe_detailed(&self, audio: &Audio) -> Result<Transcript, TranscribeError> {
        let engine = self.snapshot();
        engine.transcribe_detailed(audio).await
    }
}

impl std::fmt::Debug for Transcribers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transcribers")
            .field("engine", &self.name())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use dictation_core::local_models;

    use super::*;
    use crate::local::SpeechModel;

    struct Says(&'static str);

    impl SpeechModel for Says {
        fn run(&mut self, _: &[f32], _: Option<&str>) -> Result<String, String> {
            Ok(self.0.to_owned())
        }
    }

    #[test]
    fn a_staged_update_is_current_before_the_build_returns() {
        let transcribers = Transcribers::new(Engine::Local(LocalTranscriber::new(
            Box::new(Says("old")),
            None,
        )));
        let watch = transcribers.clone();
        transcribers.update_staged(|stage| {
            stage(Engine::Unavailable(TranscribeError::ModelLoading));
            assert!(matches!(
                &*watch.current(),
                Engine::Unavailable(TranscribeError::ModelLoading)
            ));
            Engine::Mock(MockTranscriber::default())
        });
        assert_eq!(transcribers.name(), "mock");
    }

    #[test]
    fn only_a_language_the_model_speaks_is_passed_on() {
        let parakeet = local_models::find("parakeet-unified-en-0.6b-q4_k_m").unwrap();
        let qwen = local_models::find("qwen3-asr-0.6b-q4_k_m").unwrap();
        assert_eq!(local_language(parakeet, Some("en")).as_deref(), Some("en"));
        assert_eq!(local_language(parakeet, Some("hi")), None);
        assert_eq!(local_language(qwen, Some("hi")).as_deref(), Some("hi"));
        assert_eq!(local_language(qwen, None), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn criterion_5_a_local_engine_dictates_through_transcribers() {
        let local = LocalTranscriber::new(Box::new(Says("on device")), None);
        let transcribers = Transcribers::new(Engine::Unavailable(TranscribeError::ModelMissing));
        let audio = Audio {
            sample_rate: 16_000,
            samples: vec![0.0; 1_600],
        };
        assert_eq!(
            transcribers.transcribe(&audio).await,
            Err(TranscribeError::ModelMissing)
        );
        transcribers.update(|| Engine::Local(local.clone()));
        assert_eq!(transcribers.name(), "local");
        assert_eq!(transcribers.transcribe(&audio).await.unwrap(), "on device");
        let detailed = transcribers.transcribe_detailed(&audio).await.unwrap();
        assert_eq!(detailed, Transcript::from_text("on device".to_owned()));
        let Engine::Local(current) = &*transcribers.current() else {
            panic!("not local");
        };
        assert!(current.shares_model(&local));
    }
}
