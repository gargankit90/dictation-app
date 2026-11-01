//! Wires the speech-to-text engine to the settings and the keychain (M4.4).
//!
//! At startup, and again whenever the settings or an API key are saved, the
//! engine is rebuilt from the saved provider, model and language and that
//! provider's key (`dictation_transcribe::engine`). The key is read here, in
//! Rust, from the keychain; it goes into the engine and nowhere else: not
//! into a log, an error message or an IPC reply.
//!
//! `--transcriber mock` pins the mock engine: [`LiveEngine::fixed`] never
//! rebuilds, so saving settings in a test run can't swap in a real provider.

use dictation_core::settings::Settings;
use dictation_pipeline::config::{Config, ConfigError};
use dictation_ports::{SecretError, SecretStore, TranscribeError};
use dictation_transcribe::engine::{Engine, Transcribers};

/// Rebuilds the engine when the settings or a key change. Managed Tauri
/// state, shared with the commands.
#[derive(Clone)]
pub struct LiveEngine {
    /// `None` when the engine is pinned (`--transcriber mock`).
    transcribers: Option<Transcribers>,
    /// Where the provider is reached instead of its own endpoint; only
    /// tests set it.
    base_url: Option<String>,
}

impl LiveEngine {
    /// Follows the settings: every [`changed`] call rebuilds `transcribers`.
    #[must_use]
    pub fn follow(transcribers: Transcribers) -> Self {
        Self {
            transcribers: Some(transcribers),
            base_url: None,
        }
    }

    /// The engine never changes (`--transcriber mock`).
    #[must_use]
    pub fn fixed() -> Self {
        Self {
            transcribers: None,
            base_url: None,
        }
    }

    /// Sends requests to `base_url` (a wiremock server) instead of the
    /// provider's own endpoint.
    #[cfg(test)]
    #[must_use]
    fn at(mut self, base_url: &str) -> Self {
        self.base_url = Some(base_url.to_owned());
        self
    }

    /// Reads the saved settings and key and makes the matching engine
    /// current. A no-op for a pinned engine. Reads the keychain, so call it
    /// from a blocking task.
    pub fn rebuild<S: SecretStore>(&self, config: &Config<S>) {
        let Some(transcribers) = &self.transcribers else {
            return;
        };
        transcribers.update(|| engine_from_config(config, self.base_url.as_deref()));
        tracing::info!(engine = transcribers.name(), "engine rebuilt");
    }
}

/// The engine for the saved settings and the saved key of their provider.
/// Anything that stops one being built becomes [`Engine::Unavailable`], so
/// dictation reports it.
pub fn engine_from_config<S: SecretStore>(config: &Config<S>, base_url: Option<&str>) -> Engine {
    let settings = match config.settings() {
        Ok(settings) => settings,
        Err(error) => {
            tracing::warn!(%error, "could not read the settings for the engine");
            return Engine::Unavailable(TranscribeError::Engine(format!(
                "couldn't read the settings: {error}"
            )));
        }
    };
    let key = config
        .api_key(settings.provider)
        .map_err(|error| match error {
            ConfigError::Secret(error) => error,
            // `api_key` only fails in the keychain; ConfigError never holds a key.
            other => SecretError::Store(other.to_string()),
        });
    log_choice(&settings, matches!(key, Ok(Some(_))));
    match base_url {
        Some(url) => Engine::from_settings_at(&settings, key, url),
        None => Engine::from_settings(&settings, key),
    }
}

/// What the engine was built from, for the log: whether a key exists, never
/// the key.
fn log_choice(settings: &Settings, has_key: bool) {
    tracing::info!(
        provider = %settings.provider,
        model = %settings.model,
        language = ?settings.language,
        has_key,
        "engine settings"
    );
}

/// Runs `change` (saving settings or a key) and, if it worked, rebuilds the
/// engine, so the next dictation uses what was saved.
///
/// # Errors
///
/// Whatever `change` returns; nothing is rebuilt then.
pub fn changed<S: SecretStore, T>(
    config: &Config<S>,
    live: &LiveEngine,
    change: impl FnOnce(&Config<S>) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    let result = change(config)?;
    live.rebuild(config);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use dictation_core::coordinator::Phase::{Idle, Inserting, Recording, Transcribing};
    use dictation_core::error::DictationError;
    use dictation_core::settings::Provider;
    use dictation_pipeline::pipeline::{Config as PipelineConfig, Pipeline, Ports, UiEvent};
    use dictation_platform::fakes::{
        FakeClock, FakeHotkey, FakeInserter, FakePermissions, FileAudioSource, MemSecretStore,
    };
    use dictation_ports::HotkeyEvent;
    use tokio::sync::mpsc::UnboundedReceiver;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// Obviously fake; every test checks it never reaches the UI.
    const KEY: &str = "gsk_wiring-key-NOT-REAL-3b8f";
    const WRONG_KEY: &str = "gsk_invalid_test_key";
    const TRANSCRIPT: &str = "hello from groq";
    const WAIT: Duration = Duration::from_secs(10);

    /// A fresh directory in the temp dir, removed on drop.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "dictation-engine-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The whole wiring around a real pipeline: settings and keychain in
    /// memory, a wiremock server standing in for Groq, a fake hotkey and
    /// inserter.
    struct Rig {
        dir: TempDir,
        config: Config<MemSecretStore>,
        secrets: MemSecretStore,
        live: LiveEngine,
        server: MockServer,
        hotkey: FakeHotkey,
        clock: FakeClock,
        inserter: FakeInserter,
        pipeline: Pipeline,
        ui: UnboundedReceiver<UiEvent>,
    }

    impl Rig {
        /// `status` is what the provider answers with for `key`; any other
        /// key gets a 401 (as Groq does).
        async fn start(key_status: u16) -> Self {
            let dir = TempDir::new(&format!("rig{key_status}"));
            let secrets = MemSecretStore::new();
            let config = Config::new(dir.0.join("config"), secrets.clone());

            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/audio/transcriptions"))
                .and(header("authorization", format!("Bearer {KEY}").as_str()))
                .respond_with(match key_status {
                    200 => ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "text": TRANSCRIPT })),
                    status => ResponseTemplate::new(status).insert_header("retry-after", "0"),
                })
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path("/audio/transcriptions"))
                .respond_with(ResponseTemplate::new(401))
                .with_priority(10)
                .mount(&server)
                .await;

            let live = LiveEngine::follow(Transcribers::new(engine_from_config(
                &config,
                Some(&server.uri()),
            )))
            .at(&server.uri());
            let transcribers = live.transcribers.clone().unwrap();
            let wav = dir.0.join("speech.wav");
            let wav_bytes = dictation_core::wav::encode_f32(&vec![0.25; 8_000], 16_000).unwrap();
            std::fs::write(&wav, wav_bytes).unwrap();

            let hotkey = FakeHotkey::new();
            let clock = FakeClock::at(Duration::from_secs(100));
            let inserter = FakeInserter::new();
            let ports = Ports {
                hotkey: hotkey.clone(),
                audio: FileAudioSource::new(&wav),
                transcriber: transcribers,
                inserter: inserter.clone(),
                permissions: FakePermissions::all_granted(),
                clock: clock.clone(),
            };
            let (pipeline, ui) = Pipeline::spawn(ports, PipelineConfig::default()).unwrap();
            Self {
                dir,
                config,
                secrets,
                live,
                server,
                hotkey,
                clock,
                inserter,
                pipeline,
                ui,
            }
        }

        /// One press and release, then every UI event up to Idle.
        async fn dictate(&mut self) -> Vec<UiEvent> {
            assert!(self.hotkey.emit(HotkeyEvent::Pressed));
            self.clock.advance(Duration::from_millis(500));
            assert!(self.hotkey.emit(HotkeyEvent::Released));
            let mut events = Vec::new();
            loop {
                let event = tokio::time::timeout(WAIT, self.ui.recv())
                    .await
                    .expect("no UI event in time")
                    .expect("pipeline stopped");
                let idle = event == UiEvent::StateChanged(Idle);
                events.push(event);
                if idle {
                    return events;
                }
            }
        }

        fn save_key(&self, key: &str) {
            changed(&self.config, &self.live, |c| {
                c.set_api_key(Provider::Groq, key)
            })
            .unwrap();
        }

        async fn requests(&self) -> usize {
            self.server.received_requests().await.unwrap().len()
        }

        async fn finish(self) {
            self.pipeline.shutdown().await;
            drop(self.dir);
        }
    }

    fn failed(reason: &str) -> Vec<UiEvent> {
        vec![
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::Error(DictationError::TranscriptionFailed {
                reason: reason.to_owned(),
            }),
            UiEvent::StateChanged(Idle),
        ]
    }

    fn pasted() -> Vec<UiEvent> {
        [Recording, Transcribing, Inserting, Idle]
            .map(UiEvent::StateChanged)
            .to_vec()
    }

    #[tokio::test]
    async fn criterion_without_a_key_dictation_says_add_one_and_pastes_nothing() {
        let mut rig = Rig::start(200).await;
        assert_eq!(rig.dictate().await, failed("Add your API key in Settings."));
        assert!(rig.inserter.inserted().is_empty());
        assert_eq!(rig.requests().await, 0, "no request without a key");
        rig.finish().await;
    }

    #[tokio::test]
    async fn saving_the_key_fixes_it_without_a_restart() {
        let mut rig = Rig::start(200).await;
        rig.dictate().await;
        rig.save_key(&format!("  {KEY}\n"));
        assert_eq!(rig.dictate().await, pasted());
        assert_eq!(rig.inserter.inserted(), [TRANSCRIPT]);
        assert_eq!(rig.requests().await, 1);
        rig.finish().await;
    }

    #[tokio::test]
    async fn a_key_saved_before_startup_is_used_from_the_first_dictation() {
        let dir = TempDir::new("startup");
        let secrets = MemSecretStore::new();
        let config = Config::new(dir.0.join("config"), secrets.clone());
        config.set_api_key(Provider::Groq, KEY).unwrap();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .and(header("authorization", format!("Bearer {KEY}").as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "text": "ok" })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let engine = engine_from_config(&config, Some(&server.uri()));
        assert_eq!(engine.name(), "remote");
        let audio = dictation_ports::Audio {
            sample_rate: 16_000,
            samples: vec![0.1; 1_600],
        };
        assert_eq!(
            dictation_ports::Transcriber::transcribe(&engine, &audio)
                .await
                .unwrap(),
            "ok"
        );
    }

    #[tokio::test]
    async fn a_wrong_key_shows_the_error_pastes_nothing_and_the_next_dictation_works() {
        let mut rig = Rig::start(200).await;
        rig.save_key(WRONG_KEY);
        let events = rig.dictate().await;
        assert_eq!(
            events,
            failed("The provider rejected your API key. Check it in Settings.")
        );
        assert!(!format!("{events:?}").contains(WRONG_KEY));
        assert!(rig.inserter.inserted().is_empty());
        assert_eq!(rig.requests().await, 1, "a rejected key isn't retried");

        // The app stayed usable: fix the key and the next one pastes.
        rig.save_key(KEY);
        assert_eq!(rig.dictate().await, pasted());
        assert_eq!(rig.inserter.inserted(), [TRANSCRIPT]);
        rig.finish().await;
    }

    #[tokio::test]
    async fn rate_limits_and_server_errors_reach_the_user_after_the_retries() {
        for (status, reason, attempts) in [
            (
                429,
                "The provider is limiting requests right now. Try again in a minute.",
                3,
            ),
            (
                503,
                "Something went wrong: the provider failed (HTTP 503).",
                3,
            ),
            (
                400,
                "Something went wrong: the provider rejected the request (HTTP 400).",
                1,
            ),
        ] {
            let mut rig = Rig::start(status).await;
            rig.save_key(KEY);
            let events = rig.dictate().await;
            assert_eq!(events, failed(reason), "HTTP {status}");
            assert!(!format!("{events:?}").contains(KEY));
            assert!(rig.inserter.inserted().is_empty());
            assert_eq!(rig.requests().await, attempts, "HTTP {status}");
            rig.finish().await;
        }
    }

    #[tokio::test]
    async fn an_unreachable_provider_is_reported_and_dictation_recovers() {
        let mut rig = Rig::start(200).await;
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        // Same wiring, but nothing listens at the base URL.
        let dead = LiveEngine {
            transcribers: rig.live.transcribers.clone(),
            base_url: Some(format!("http://127.0.0.1:{port}")),
        };
        changed(&rig.config, &dead, |c| c.set_api_key(Provider::Groq, KEY)).unwrap();
        assert_eq!(
            rig.dictate().await,
            failed("Couldn't reach the provider. Check your internet connection.")
        );
        assert!(rig.inserter.inserted().is_empty());

        rig.live.rebuild(&rig.config); // back to the working server
        assert_eq!(rig.dictate().await, pasted());
        rig.finish().await;
    }

    #[tokio::test]
    async fn saving_settings_rebuilds_the_engine_with_the_new_model_and_language() {
        let mut rig = Rig::start(200).await;
        rig.save_key(KEY);
        rig.dictate().await;
        let settings = Settings {
            model: "whisper-large-v3".into(),
            language: Some("fr".into()),
            ..Settings::default()
        };
        changed(&rig.config, &rig.live, |c| c.save_settings(&settings)).unwrap();
        assert_eq!(rig.dictate().await, pasted());

        let requests = rig.server.received_requests().await.unwrap();
        let body = |n: usize| String::from_utf8_lossy(&requests[n].body).into_owned();
        assert!(body(0).contains("whisper-large-v3-turbo"), "the default");
        assert!(!body(0).contains("name=\"language\""));
        assert!(body(1).contains("whisper-large-v3\r\n"), "the saved model");
        assert!(body(1).contains("name=\"language\""));
        assert!(body(1).contains("\r\n\r\nfr\r\n"));
        rig.finish().await;
    }

    #[tokio::test]
    async fn switching_provider_looks_for_that_providers_key() {
        let mut rig = Rig::start(200).await;
        rig.save_key(KEY); // a Groq key only
        let openai = Settings {
            provider: Provider::OpenAi,
            model: Provider::OpenAi.default_model().to_owned(),
            ..Settings::default()
        };
        changed(&rig.config, &rig.live, |c| c.save_settings(&openai)).unwrap();
        assert_eq!(rig.dictate().await, failed("Add your API key in Settings."));
        assert_eq!(rig.requests().await, 0);
        rig.finish().await;
    }

    #[tokio::test]
    async fn a_failed_change_rebuilds_nothing() {
        let mut rig = Rig::start(200).await;
        rig.save_key(KEY);
        let invalid = Settings {
            model: String::new(),
            ..Settings::default()
        };
        assert!(changed(&rig.config, &rig.live, |c| c.save_settings(&invalid)).is_err());
        assert!(
            changed(&rig.config, &rig.live, |c| c
                .set_api_key(Provider::Groq, "  "))
            .is_err()
        );
        assert_eq!(rig.dictate().await, pasted(), "still the saved key");
        rig.finish().await;
    }

    #[tokio::test]
    async fn an_unreadable_keychain_or_settings_file_is_reported_not_fatal() {
        let mut rig = Rig::start(200).await;
        rig.save_key(KEY);
        rig.secrets.set_access_denied(true);
        rig.live.rebuild(&rig.config);
        let events = rig.dictate().await;
        let UiEvent::Error(DictationError::TranscriptionFailed { reason }) = &events[2] else {
            panic!("{events:?}");
        };
        assert!(reason.contains("keychain access was denied"), "{reason}");
        assert!(rig.inserter.inserted().is_empty());

        rig.secrets.set_access_denied(false);
        std::fs::create_dir_all(rig.config.path().parent().unwrap()).unwrap();
        std::fs::write(rig.config.path(), "{ not json").unwrap();
        rig.live.rebuild(&rig.config);
        let events = rig.dictate().await;
        let UiEvent::Error(DictationError::TranscriptionFailed { reason }) = &events[2] else {
            panic!("{events:?}");
        };
        assert!(reason.contains("couldn't read the settings"), "{reason}");
        rig.finish().await;
    }

    #[tokio::test]
    async fn a_pinned_engine_ignores_saved_settings_and_keys() {
        let dir = TempDir::new("pinned");
        let config = Config::new(dir.0.join("config"), MemSecretStore::new());
        let live = LiveEngine::fixed();
        // Nothing to rebuild: no engine, no keychain read, no panic.
        changed(&config, &live, |c| c.set_api_key(Provider::Groq, KEY)).unwrap();
        live.rebuild(&config);
    }
}
