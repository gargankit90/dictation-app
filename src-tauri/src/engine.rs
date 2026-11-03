//! Wires the speech-to-text engine to the settings and the keychain (M4.4).
//!
//! At startup, and again whenever the settings or an API key are saved, the
//! engine is rebuilt from the saved provider, model and language and that
//! provider's key (`dictation_transcribe::engine`). The key is read here, in
//! Rust, from the keychain; it goes into the engine and nowhere else: not
//! into a log, an error message or an IPC reply.
//!
//! The keychain can block: a locked login keychain, or one whose access
//! prompt nobody answers, makes a read hang. So startup never reads it:
//! the app starts with [`starting`] (dictating says the key is still being
//! read) and [`LiveEngine::rebuild_in_background`] reads it on a thread of
//! its own. Every read gives up after [`KEY_READ_TIMEOUT`]; "locked, needs
//! interaction, denied, timed out" all become a clear message at dictation
//! time ([`TranscribeError::KeychainUnavailable`]), never a hang.
//!
//! A read that times out can't be cancelled, so [`KeyReads`] allows one read
//! in flight at a time for the whole app: later rebuilds wait on it (each for
//! its own timeout) instead of starting reads of their own, and a read that
//! was started before the last save is not trusted for the rebuild after it.
//! A locked keychain therefore costs one parked thread, not one per save.
//!
//! `--transcriber mock` pins the mock engine: [`LiveEngine::fixed`] never
//! rebuilds, so saving settings in a test run can't swap in a real provider.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use dictation_core::settings::{Provider, Settings};
use dictation_pipeline::config::{Config, ConfigError};
use dictation_ports::{Secret, SecretError, SecretStore, TranscribeError};
use dictation_transcribe::engine::{Engine, Transcribers};

use crate::key_writes::KeyWrites;

/// How long a keychain read may take before the key counts as unreadable.
/// Long enough for a person to answer an access prompt.
pub const KEY_READ_TIMEOUT: Duration = Duration::from_secs(15);

/// The engine the app starts with, before the key has been read: dictating
/// says the key is still being read.
#[must_use]
pub fn starting() -> Engine {
    Engine::Unavailable(TranscribeError::KeyPending)
}

/// Rebuilds the engine when the settings or a key change. Managed Tauri
/// state, shared with the commands.
#[derive(Clone)]
pub struct LiveEngine {
    /// `None` when the engine is pinned (`--transcriber mock`).
    transcribers: Option<Transcribers>,
    /// Where the provider is reached instead of its own endpoint; only
    /// tests and test builds (`--debug-provider-url`) set it, so in a
    /// release build it is always `None`.
    base_url: Option<String>,
    /// How long one keychain read may take.
    key_timeout: Duration,
    /// The one keychain read allowed in flight, shared by every clone.
    reads: KeyReads,
    /// The one keychain write allowed in flight, shared by every clone (T3).
    writes: KeyWrites,
}

impl LiveEngine {
    /// Follows the settings: every [`changed`] call rebuilds `transcribers`.
    #[must_use]
    pub fn follow(transcribers: Transcribers) -> Self {
        Self {
            transcribers: Some(transcribers),
            base_url: None,
            key_timeout: KEY_READ_TIMEOUT,
            reads: KeyReads::default(),
            writes: KeyWrites::default(),
        }
    }

    /// The engine never changes (`--transcriber mock`).
    #[must_use]
    pub fn fixed() -> Self {
        Self {
            transcribers: None,
            base_url: None,
            key_timeout: KEY_READ_TIMEOUT,
            reads: KeyReads::default(),
            writes: KeyWrites::default(),
        }
    }

    /// Sends requests to `base_url` (a wiremock server, or the e2e tests'
    /// local provider through `--debug-provider-url`) instead of the
    /// provider's own endpoint. Not compiled into release builds.
    #[cfg(any(test, debug_assertions))]
    #[must_use]
    pub(crate) fn at(mut self, base_url: &str) -> Self {
        self.base_url = Some(base_url.to_owned());
        self
    }

    /// The API root `provider` is reached at: its own endpoint, unless a test
    /// build was pointed elsewhere ([`at`](Self::at)). Settings' Test key
    /// uses it, so a key is tested where dictation sends it.
    #[must_use]
    pub fn provider_base_url(&self, provider: Provider) -> String {
        self.base_url.clone().unwrap_or_else(|| {
            dictation_transcribe::openai_compat::Preset::for_provider(provider)
                .base_url
                .to_owned()
        })
    }

    /// `provider`'s saved key, for the settings commands (`has_api_key`, a
    /// test of the saved key). Bounded like the engine's own reads: the
    /// keychain gets [`KEY_READ_TIMEOUT`], through the same [`KeyReads`], so a
    /// keychain that never answers costs one parked thread however often the
    /// UI asks again, and each ask returns an error at its deadline.
    /// Blocks: call it from a blocking task.
    ///
    /// # Errors
    ///
    /// The keychain's error, or that it didn't answer in time.
    pub fn read_key<S: SecretStore + 'static>(
        &self,
        config: &Arc<Config<S>>,
        provider: Provider,
    ) -> Result<Option<Secret>, SecretError> {
        let key = self.reads.read(config, provider, self.key_timeout);
        if matches!(key, Ok(Some(_))) {
            // The keychain answers now: an engine that gave up on it at
            // launch can be rebuilt (#204).
            self.rebuild_if_keychain_failed(config);
        }
        key
    }

    /// Rebuilds the engine if it is unavailable because the keychain failed
    /// or timed out, and does nothing otherwise (so no extra keychain read
    /// when the engine is fine, no key is saved, or it is pinned). Blocks.
    pub fn rebuild_if_keychain_failed<S: SecretStore + 'static>(&self, config: &Arc<Config<S>>) {
        let Some(transcribers) = &self.transcribers else {
            return;
        };
        if !transcribers.keychain_failed() {
            return;
        }
        let engine = transcribers.update_if_keychain_failed(|| self.build(config));
        tracing::info!(
            engine = engine.name(),
            "engine rebuilt after a keychain failure"
        );
    }

    /// Makes a dictation that finds the engine unavailable because the
    /// keychain failed rebuild it from `config` first (#204). Call once at
    /// startup, after the config exists. A no-op for a pinned engine.
    pub fn retry_on_dictation<S: SecretStore + 'static>(&self, config: Arc<Config<S>>) {
        let Some(transcribers) = &self.transcribers else {
            return;
        };
        // Not a clone of `self`: it holds the transcribers, which would hold
        // this closure.
        let (base_url, reads, key_timeout) =
            (self.base_url.clone(), self.reads.clone(), self.key_timeout);
        transcribers.retry_with(move || {
            engine_from_config(&config, base_url.as_deref(), &reads, key_timeout)
        });
    }

    fn build<S: SecretStore + 'static>(&self, config: &Arc<Config<S>>) -> Engine {
        engine_from_config(
            config,
            self.base_url.as_deref(),
            &self.reads,
            self.key_timeout,
        )
    }

    /// Saves `key` as `provider`'s API key and rebuilds the engine from it
    /// (T3, #113). Bounded like the reads: the keychain gets
    /// [`KEY_READ_TIMEOUT`], and at most one write is in flight app-wide, so
    /// a keychain that never answers gives an error at the deadline and
    /// costs one parked thread however often the user saves again. A save
    /// behind a write that is still running waits for it, so the newest key
    /// is written last. Blocks: call it from a blocking task.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidKey`] before anything is written, the keychain's
    /// error, or that it didn't answer in time (the key may then still be
    /// saved later; see [`KeyWrites`]).
    pub fn write_key<S: SecretStore + 'static>(
        &self,
        config: &Arc<Config<S>>,
        provider: Provider,
        key: String,
    ) -> Result<(), ConfigError> {
        // Refused here, so a typo never waits behind a stuck write.
        dictation_pipeline::config::validate_key(&key)?;
        let (config, live) = (Arc::clone(config), self.clone());
        self.writes.write(self.key_timeout, move || {
            changed(&config, &live, |config| config.set_api_key(provider, &key))
        })
    }

    /// The keychain writes, so a test can see a save queue.
    #[cfg(test)]
    pub(crate) fn writes(&self) -> &KeyWrites {
        &self.writes
    }

    /// Gives up on a keychain read or write after `timeout` instead of
    /// [`KEY_READ_TIMEOUT`]; only tests need it short.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_key_timeout(mut self, timeout: Duration) -> Self {
        self.key_timeout = timeout;
        self
    }

    /// Reads the saved settings and key and makes the matching engine
    /// current. A no-op for a pinned engine. Waits for the keychain (at most
    /// the key timeout), so call it from a blocking task, never from the
    /// event loop or startup; [`rebuild_in_background`](Self::rebuild_in_background)
    /// does that for startup.
    pub fn rebuild<S: SecretStore + 'static>(&self, config: &Arc<Config<S>>) {
        let Some(transcribers) = &self.transcribers else {
            return;
        };
        transcribers.update(|| self.build(config));
        tracing::info!(engine = transcribers.name(), "engine rebuilt");
    }

    /// [`rebuild`](Self::rebuild) on a thread of its own, so the caller (app
    /// startup) never waits for the keychain.
    ///
    /// # Errors
    ///
    /// If the thread can't be started.
    pub fn rebuild_in_background<S: SecretStore + 'static>(
        &self,
        config: Arc<Config<S>>,
    ) -> std::io::Result<std::thread::JoinHandle<()>> {
        let live = self.clone();
        std::thread::Builder::new()
            .name("engine-startup".to_owned())
            .spawn(move || live.rebuild(&config))
    }
}

/// The engine for the saved settings and the saved key of their provider.
/// Anything that stops one being built becomes [`Engine::Unavailable`], so
/// dictation reports it.
///
/// The keychain is read through `reads` and given `key_timeout`; if it
/// hasn't answered by then the engine says the key couldn't be read. The
/// stuck read isn't cancelled (it can't be), but it stays the only one:
/// see [`KeyReads`].
pub fn engine_from_config<S: SecretStore + 'static>(
    config: &Arc<Config<S>>,
    base_url: Option<&str>,
    reads: &KeyReads,
    key_timeout: Duration,
) -> Engine {
    let settings = match config.settings() {
        Ok(settings) => settings,
        Err(error) => {
            tracing::warn!(%error, "could not read the settings for the engine");
            return Engine::Unavailable(TranscribeError::Engine(format!(
                "couldn't read the settings: {error}"
            )));
        }
    };
    let key = reads.read(config, settings.provider, key_timeout);
    if let Err(error) = &key {
        // The text never holds a key.
        tracing::warn!(%error, "could not read the API key from the keychain");
    }
    log_choice(&settings, matches!(key, Ok(Some(_))));
    match base_url {
        Some(url) => Engine::from_settings_at(&settings, key, url),
        None => Engine::from_settings(&settings, key),
    }
}

type KeyOutcome = Result<Option<Secret>, SecretError>;

/// Keychain reads with at most one in flight, app-wide.
///
/// A keychain read that never returns (a locked keychain waiting for a
/// person) can't be cancelled; giving up on it only stops waiting. Without a
/// bound, every settings save would start another read that parks on the
/// keychain too. So a request first looks for the read already in flight:
///
/// - one that is fresh (same provider, started after the last
///   [`invalidate`](Self::invalidate)) is waited on and its answer shared;
/// - one that is stale (the provider changed, or a save happened since it
///   began, so its answer may be the old key) is waited on to finish, within
///   the same deadline, and then a new read starts.
///
/// Either way the waiter's deadline is its own timeout from the call, so a
/// stuck read costs each rebuild one timeout and no thread.
#[derive(Clone, Default)]
pub struct KeyReads {
    state: Arc<Mutex<ReadState>>,
}

#[derive(Default)]
struct ReadState {
    /// Bumped by every save; a read started before it is stale after it.
    generation: u64,
    /// The read whose thread is still running, if any.
    in_flight: Option<Arc<Read>>,
}

/// One keychain read: what it asked for and, once it finishes, the answer.
struct Read {
    provider: Provider,
    generation: u64,
    outcome: Mutex<Option<KeyOutcome>>,
    finished: Condvar,
}

impl Read {
    /// The answer, or `None` if `deadline` passes first.
    fn wait(&self, deadline: Instant) -> Option<KeyOutcome> {
        let mut outcome = lock(&self.outcome);
        loop {
            if let Some(answer) = &*outcome {
                return Some(answer.clone());
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            outcome = self
                .finished
                .wait_timeout(outcome, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl KeyReads {
    /// Marks every read in flight as stale: call it after a save, before the
    /// rebuild that should see what was saved.
    pub fn invalidate(&self) {
        lock(&self.state).generation += 1;
    }

    /// `provider`'s key from the keychain, waiting at most `timeout` (from
    /// now) for it.
    fn read<S: SecretStore + 'static>(
        &self,
        config: &Arc<Config<S>>,
        provider: Provider,
        timeout: Duration,
    ) -> KeyOutcome {
        let deadline = Instant::now() + timeout;
        let wanted = lock(&self.state).generation;
        loop {
            let read = self.in_flight_or_start(config, provider)?;
            let Some(answer) = read.wait(deadline) else {
                return Err(SecretError::Store(format!(
                    "no answer from the keychain after {} s (locked, or waiting for access to be allowed)",
                    timeout.as_secs()
                )));
            };
            if read.provider == provider && read.generation >= wanted {
                return answer;
            }
            // A stale answer: it's finished now, so the next pass starts a
            // read of its own (the only one in flight).
        }
    }

    /// The read in flight, or a new one if there is none.
    fn in_flight_or_start<S: SecretStore + 'static>(
        &self,
        config: &Arc<Config<S>>,
        provider: Provider,
    ) -> Result<Arc<Read>, SecretError> {
        let mut state = lock(&self.state);
        if let Some(read) = &state.in_flight {
            return Ok(Arc::clone(read));
        }
        let read = Arc::new(Read {
            provider,
            generation: state.generation,
            outcome: Mutex::new(None),
            finished: Condvar::new(),
        });
        let (reader, shared, task) = (
            Arc::clone(config),
            Arc::clone(&self.state),
            Arc::clone(&read),
        );
        std::thread::Builder::new()
            .name("keychain-read".to_owned())
            .spawn(move || {
                let answer = catch_unwind(AssertUnwindSafe(|| reader.api_key(provider)))
                    .map_or_else(
                        |_| {
                            Err(SecretError::Store(
                                "the keychain read stopped unexpectedly".to_owned(),
                            ))
                        },
                        |result| match result {
                            Ok(key) => Ok(key),
                            Err(ConfigError::Secret(error)) => Err(error),
                            // `api_key` only fails in the keychain; ConfigError never holds a key.
                            Err(other) => Err(SecretError::Store(other.to_string())),
                        },
                    );
                // Done and no longer in flight in one step, so nobody sees a
                // finished read as still running.
                let mut state = lock(&shared);
                state.in_flight = None;
                *lock(&task.outcome) = Some(answer);
                task.finished.notify_all();
            })
            .map_err(|error| {
                SecretError::Store(format!("could not start the keychain read: {error}"))
            })?;
        state.in_flight = Some(Arc::clone(&read));
        Ok(read)
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
pub fn changed<S: SecretStore + 'static, T>(
    config: &Arc<Config<S>>,
    live: &LiveEngine,
    change: impl FnOnce(&Config<S>) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    let result = change(config)?;
    // A keychain read already in flight may have the old key.
    live.reads.invalidate();
    live.rebuild(config);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    use dictation_core::coordinator::Phase::{Idle, Inserting, Recording, Transcribing};
    use dictation_core::error::DictationError;
    use dictation_core::settings::Provider;
    use dictation_pipeline::pipeline::{Config as PipelineConfig, Pipeline, Ports, UiEvent};
    use dictation_pipeline::ui_events::UiReceiver;
    use dictation_platform::fakes::{
        FakeClock, FakeHotkey, FakeInserter, FakePermissions, FileAudioSource, MemSecretStore,
    };
    use dictation_ports::{HotkeyEvent, Secret};
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
        config: Arc<Config<MemSecretStore>>,
        secrets: MemSecretStore,
        live: LiveEngine,
        server: MockServer,
        hotkey: FakeHotkey,
        clock: FakeClock,
        inserter: FakeInserter,
        pipeline: Pipeline,
        ui: UiReceiver,
    }

    impl Rig {
        /// `status` is what the provider answers with for `key`; any other
        /// key gets a 401 (as Groq does).
        async fn start(key_status: u16) -> Self {
            let dir = TempDir::new(&format!("rig{key_status}"));
            let secrets = MemSecretStore::new();
            let config = Arc::new(Config::new(dir.0.join("config"), secrets.clone()));

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
                &KeyReads::default(),
                WAIT,
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
        let config = Arc::new(Config::new(dir.0.join("config"), secrets.clone()));
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
        let engine = engine_from_config(&config, Some(&server.uri()), &KeyReads::default(), WAIT);
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
            key_timeout: WAIT,
            reads: rig.live.reads.clone(),
            writes: rig.live.writes.clone(),
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
        assert!(reason.contains("Couldn't read your API key from the Keychain"));
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
        let config = Arc::new(Config::new(dir.0.join("config"), MemSecretStore::new()));
        let live = LiveEngine::fixed();
        // Nothing to rebuild: no engine, no keychain read, no panic.
        changed(&config, &live, |c| c.set_api_key(Provider::Groq, KEY)).unwrap();
        live.rebuild(&config);
    }

    /// A rig whose engine starts as the app does: before the key is read.
    async fn rig_at_startup() -> Rig {
        let mut rig = Rig::start(200).await;
        rig.secrets.set(KEY_NAME, &Secret::new(KEY)).unwrap();
        rig.live.transcribers.as_ref().unwrap().update(starting);
        rig.live = rig.live.clone().with_key_timeout(WAIT);
        rig
    }

    const KEY_NAME: &str = "groq-api-key";

    fn message(events: &[UiEvent]) -> String {
        let Some(UiEvent::Error(DictationError::TranscriptionFailed { reason })) = events.get(2)
        else {
            panic!("{events:?}");
        };
        reason.clone()
    }

    #[tokio::test]
    async fn criterion_startup_does_not_wait_for_a_blocked_keychain() {
        let mut rig = rig_at_startup().await;
        rig.secrets.set_blocked(true);
        let started = std::time::Instant::now();
        let handle = rig.live.rebuild_in_background(rig.config.clone()).unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "starting the read waited for the keychain"
        );
        assert!(!handle.is_finished());

        // The app is usable while the keychain is stuck: dictation says
        // what is going on instead of hanging.
        let events = rig.dictate().await;
        assert!(message(&events).contains("Still reading your API key"));
        assert!(rig.inserter.inserted().is_empty());
        assert_eq!(rig.requests().await, 0);

        // Allowing access (or unlocking) finishes the read: no restart.
        rig.secrets.set_blocked(false);
        handle.join().unwrap();
        assert_eq!(rig.dictate().await, pasted());
        assert_eq!(rig.inserter.inserted(), [TRANSCRIPT]);
        rig.finish().await;
    }

    #[tokio::test]
    async fn a_keychain_that_never_answers_times_out_into_a_clear_message() {
        let mut rig = rig_at_startup().await;
        rig.secrets.set_blocked(true);
        rig.live = rig
            .live
            .clone()
            .with_key_timeout(Duration::from_millis(200));
        let started = std::time::Instant::now();
        let handle = rig.live.rebuild_in_background(rig.config.clone()).unwrap();
        // The background read gives up on its own (this join would hang
        // with no timeout).
        handle.join().unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));

        let events = rig.dictate().await;
        let reason = message(&events);
        assert!(
            reason.starts_with("Couldn't read your API key from the Keychain"),
            "{reason}"
        );
        assert!(!format!("{events:?}").contains(KEY));
        assert!(rig.inserter.inserted().is_empty());
        assert_eq!(rig.requests().await, 0);

        // A save while the keychain is back fixes it without a restart.
        rig.secrets.set_blocked(false);
        rig.save_key(KEY);
        assert_eq!(rig.dictate().await, pasted());
        rig.finish().await;
    }

    #[tokio::test]
    async fn a_locked_or_refusing_keychain_is_a_clear_message_not_a_hang() {
        let mut rig = rig_at_startup().await;
        rig.secrets
            .set_failing(Some("User interaction is not allowed."));
        let handle = rig.live.rebuild_in_background(rig.config.clone()).unwrap();
        handle.join().unwrap();
        let reason = message(&rig.dictate().await);
        assert!(reason.starts_with("Couldn't read your API key from the Keychain"));

        rig.secrets.set_failing(None);
        rig.secrets.set_access_denied(true);
        rig.live.rebuild(&rig.config);
        let reason = message(&rig.dictate().await);
        assert!(reason.starts_with("Couldn't read your API key from the Keychain"));
        assert!(rig.inserter.inserted().is_empty());
        rig.finish().await;
    }

    // ---- a keychain failure at launch heals without a restart (B1, #204) ----

    /// The launch of #204: the key is saved, the keychain doesn't answer in
    /// time, so the engine is unavailable. Returns once the startup read has
    /// given up; the keychain is still blocked.
    async fn rig_after_keychain_timeout() -> Rig {
        let mut rig = rig_at_startup().await;
        rig.secrets.set_blocked(true);
        rig.live = rig
            .live
            .clone()
            .with_key_timeout(Duration::from_millis(200));
        // After the timeout is set: the retry is bounded by the same one.
        rig.live.retry_on_dictation(rig.config.clone());
        rig.live
            .rebuild_in_background(rig.config.clone())
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(
            rig.live.transcribers.as_ref().unwrap().name(),
            "unavailable"
        );
        rig
    }

    #[tokio::test]
    async fn criterion_1_the_next_dictation_retries_after_a_keychain_timeout() {
        let mut rig = rig_after_keychain_timeout().await;
        // Still blocked: the retry is bounded, and the user sees the same error.
        let reason = message(&rig.dictate().await);
        assert!(
            reason.starts_with("Couldn't read your API key from the Keychain"),
            "{reason}"
        );
        assert!(rig.inserter.inserted().is_empty());

        // Access allowed: the next dictation works, no restart, no save.
        rig.secrets.set_blocked(false);
        assert_eq!(rig.dictate().await, pasted());
        assert_eq!(rig.inserter.inserted(), [TRANSCRIPT]);
        rig.finish().await;
    }

    #[tokio::test]
    async fn criterion_1_a_failing_read_is_retried_too() {
        let mut rig = rig_at_startup().await;
        rig.live.retry_on_dictation(rig.config.clone());
        rig.secrets
            .set_failing(Some("User interaction is not allowed."));
        rig.live.rebuild(&rig.config);
        let reason = message(&rig.dictate().await);
        assert!(reason.starts_with("Couldn't read your API key from the Keychain"));

        rig.secrets.set_failing(None);
        assert_eq!(rig.dictate().await, pasted());
        rig.finish().await;
    }

    #[tokio::test]
    async fn criterion_2_a_successful_stored_key_read_rebuilds_the_engine() {
        for via_key_saved in [false, true] {
            let rig = rig_after_keychain_timeout().await;
            rig.secrets.set_blocked(false);
            let transcribers = rig.live.transcribers.clone().unwrap();
            if via_key_saved {
                assert!(crate::onboarding::key_saved(&rig.config, &rig.live));
            } else {
                // What the key test and has_api_key call.
                let key = rig.live.read_key(&rig.config, Provider::Groq);
                assert_eq!(key, Ok(Some(Secret::new(KEY))));
            }
            assert_eq!(transcribers.name(), "remote", "key_saved: {via_key_saved}");
            rig.finish().await;
        }
    }

    #[tokio::test]
    async fn criterion_3_no_extra_reads_when_the_engine_is_fine() {
        let mut rig = Rig::start(200).await;
        rig.live.retry_on_dictation(rig.config.clone());
        rig.save_key(KEY);
        let before = rig.secrets.get_calls();
        assert_eq!(rig.dictate().await, pasted());
        assert_eq!(rig.dictate().await, pasted());
        rig.live.rebuild_if_keychain_failed(&rig.config);
        assert_eq!(
            rig.secrets.get_calls(),
            before,
            "a working engine reads nothing"
        );
        rig.finish().await;
    }

    #[tokio::test]
    async fn criterion_3_no_extra_reads_when_no_key_is_saved() {
        let mut rig = Rig::start(200).await;
        rig.live.retry_on_dictation(rig.config.clone());
        rig.live.rebuild(&rig.config);
        let before = rig.secrets.get_calls();
        for _ in 0..2 {
            let reason = message(&rig.dictate().await);
            assert!(!reason.contains("Keychain"), "{reason}");
        }
        rig.live.rebuild_if_keychain_failed(&rig.config);
        assert_eq!(
            rig.secrets.get_calls(),
            before,
            "no key is not a keychain failure"
        );
        rig.finish().await;
    }

    #[tokio::test]
    async fn criterion_3_a_keychain_that_keeps_failing_costs_one_bounded_read_per_dictation() {
        let mut rig = rig_after_keychain_timeout().await;
        let before = rig.secrets.get_calls();
        for _ in 0..3 {
            let reason = message(&rig.dictate().await);
            assert!(reason.starts_with("Couldn't read your API key from the Keychain"));
        }
        // The blocked read from launch is still the one in flight: the
        // retries wait on it (each for its timeout) and start no thread.
        assert_eq!(
            rig.secrets.get_calls(),
            before,
            "retries shared the stuck read"
        );
        rig.secrets.set_blocked(false);
        rig.finish().await;
    }

    #[tokio::test]
    async fn criterion_3_a_failing_keychain_is_read_at_most_once_per_dictation() {
        let mut rig = rig_at_startup().await;
        rig.live.retry_on_dictation(rig.config.clone());
        rig.secrets.set_failing(Some("locked"));
        rig.live.rebuild(&rig.config);
        let before = rig.secrets.get_calls();
        for n in 1..=3 {
            message(&rig.dictate().await);
            assert_eq!(rig.secrets.get_calls() - before, n);
        }
        rig.finish().await;
    }

    #[tokio::test]
    async fn a_pinned_engine_never_rebuilds() {
        let live = LiveEngine::fixed();
        let dir = TempDir::new("pinned");
        let secrets = MemSecretStore::new();
        let config = Arc::new(Config::new(dir.0.join("config"), secrets.clone()));
        live.retry_on_dictation(config.clone());
        live.rebuild_if_keychain_failed(&config);
        assert_eq!(secrets.get_calls(), 0);
    }

    #[test]
    fn engine_from_config_returns_within_the_timeout_when_the_keychain_blocks() {
        let dir = TempDir::new("blocked");
        let secrets = MemSecretStore::new();
        secrets.set_blocked(true);
        let config = Arc::new(Config::new(dir.0.join("config"), secrets.clone()));
        let started = std::time::Instant::now();
        let engine = engine_from_config(
            &config,
            None,
            &KeyReads::default(),
            Duration::from_millis(100),
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(engine.name(), "unavailable");
        secrets.set_blocked(false); // lets the abandoned read thread end
    }

    // ---- one keychain read in flight (R3) ----

    /// Runs `f` on `n` threads at once and waits for all of them.
    fn at_once(n: usize, f: impl Fn(usize) + Send + Sync + 'static) {
        let f = Arc::new(f);
        let threads: Vec<_> = (0..n)
            .map(|i| {
                let f = Arc::clone(&f);
                std::thread::spawn(move || f(i))
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
    }

    #[tokio::test]
    async fn a_blocked_keychain_gets_one_reader_however_many_rebuilds_and_dictations() {
        let mut rig = rig_at_startup().await;
        let before = rig.secrets.get_calls(); // the rig's own startup read
        rig.secrets.set_blocked(true);
        rig.live = rig.live.clone().with_key_timeout(Duration::from_millis(30));

        // Startup's read, then Settings saves and dictations on top of it.
        rig.live.rebuild(&rig.config);
        for _ in 0..5 {
            changed(&rig.config, &rig.live, |c| {
                c.save_settings(&Settings::default())
            })
            .unwrap();
            let reason = message(&rig.dictate().await);
            assert!(reason.starts_with("Couldn't read your API key from the Keychain"));
        }
        // And a burst of rebuilds at the same time.
        let (live, config) = (rig.live.clone(), rig.config.clone());
        at_once(8, move |_| live.rebuild(&config));
        assert_eq!(
            rig.secrets.get_calls() - before,
            1,
            "every rebuild started its own keychain read"
        );

        // The keychain answers: the next rebuild gets the key, no restart.
        rig.secrets.set_blocked(false);
        rig.live = rig.live.clone().with_key_timeout(WAIT);
        rig.live.rebuild(&rig.config);
        assert_eq!(rig.dictate().await, pasted());
        assert!(
            rig.secrets.get_calls() - before <= 2,
            "the unblocked rebuild reads once"
        );
        rig.finish().await;
    }

    #[tokio::test]
    async fn requests_made_while_a_read_is_pending_share_its_answer() {
        let rig = rig_at_startup().await;
        let before = rig.secrets.get_calls();
        rig.secrets.set_blocked(true);
        let (reads, config) = (KeyReads::default(), rig.config.clone());
        let answers = Arc::new(Mutex::new(Vec::new()));
        let waiters = {
            let answers = Arc::clone(&answers);
            std::thread::spawn(move || {
                at_once(6, move |_| {
                    let answer = reads.read(&config, Provider::Groq, WAIT);
                    answers.lock().unwrap().push(answer);
                });
            })
        };
        while rig.secrets.get_calls() == before {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        assert!(!waiters.is_finished(), "waiters returned while blocked");

        rig.secrets.set_blocked(false);
        waiters.join().unwrap();
        assert_eq!(
            rig.secrets.get_calls() - before,
            1,
            "one read served every waiter"
        );
        {
            let answers = answers.lock().unwrap();
            assert_eq!(answers.len(), 6);
            assert!(answers.iter().all(|a| *a == Ok(Some(Secret::new(KEY)))));
        }
        rig.finish().await;
    }

    /// A store whose `get` reads the value, then waits to hand it over, like
    /// a keychain that was asked before a save and answers after it.
    #[derive(Clone)]
    struct SlowStore {
        inner: MemSecretStore,
        gate: Arc<(Mutex<bool>, Condvar)>,
        gets: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl SlowStore {
        fn open(&self, open: bool) {
            *self.gate.0.lock().unwrap() = open;
            self.gate.1.notify_all();
        }
    }

    impl SecretStore for SlowStore {
        fn get(&self, name: &str) -> Result<Option<Secret>, dictation_ports::SecretError> {
            let value = self.inner.get(name);
            self.gets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut open = self.gate.0.lock().unwrap();
            while !*open {
                open = self.gate.1.wait(open).unwrap();
            }
            value
        }

        fn set(&self, name: &str, secret: &Secret) -> Result<(), dictation_ports::SecretError> {
            self.inner.set(name, secret)
        }

        fn delete(&self, name: &str) -> Result<(), dictation_ports::SecretError> {
            self.inner.delete(name)
        }
    }

    #[tokio::test]
    async fn a_key_saved_while_a_read_is_pending_beats_the_stale_answer() {
        let dir = TempDir::new("stale");
        let store = SlowStore {
            inner: MemSecretStore::new(),
            gate: Arc::new((Mutex::new(false), Condvar::new())),
            gets: Arc::default(),
        };
        let config = Arc::new(Config::new(dir.0.join("config"), store.clone()));
        config.set_api_key(Provider::Groq, WRONG_KEY).unwrap();

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .and(header("authorization", format!("Bearer {KEY}").as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "text": "fresh" })),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(401))
            .with_priority(10)
            .mount(&server)
            .await;
        let transcribers = Transcribers::new(starting());
        let live = LiveEngine::follow(transcribers.clone()).at(&server.uri());

        // Startup's read has the old key in hand, but hasn't answered, and
        // startup gives up on it.
        let startup = live
            .clone()
            .with_key_timeout(Duration::from_millis(30))
            .rebuild_in_background(config.clone())
            .unwrap();
        while store.gets.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }
        startup.join().unwrap();
        assert_eq!(transcribers.name(), "unavailable");
        // The user saves the right key; its rebuild must not take the
        // pending read's old answer.
        let saved = {
            let (config, live) = (config.clone(), live.clone());
            std::thread::spawn(move || {
                changed(&config, &live, |c| c.set_api_key(Provider::Groq, KEY)).unwrap();
            })
        };
        std::thread::sleep(Duration::from_millis(100));
        store.open(true); // the stale answer arrives
        saved.join().unwrap();

        let audio = dictation_ports::Audio {
            sample_rate: 16_000,
            samples: vec![0.1; 1_600],
        };
        assert_eq!(
            dictation_ports::Transcriber::transcribe(&transcribers, &audio)
                .await
                .unwrap(),
            "fresh",
            "the engine kept the stale key"
        );
        assert!(
            store.gets.load(std::sync::atomic::Ordering::SeqCst) <= 2,
            "a stale answer costs one more read, never a parallel one"
        );
    }

    // ---- quitting during a real request (#115) ----
    //
    // T1 aborts an OpenAiCompat future directly; these go through the real
    // pipeline and its `shutdown`, with the engine the settings build,
    // against a raw local listener that reports what the client did. No
    // sleep is a proof: each step waits on a fence with an upper bound.

    use std::io::{Read as _, Write as _};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use dictation_ports::{Audio, Transcriber, Transcript};
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

    /// How the raw provider answers the next connection.
    enum RawReply {
        /// Holds the reply: says when the client closes the connection, then
        /// waits for `release` and sends a 200 with the transcript anyway
        /// (the reply that would have been pasted), then says so in `sent`.
        Held {
            closed: UnboundedSender<()>,
            release: std::sync::mpsc::Receiver<()>,
            sent: UnboundedSender<()>,
        },
        /// A 503 asking for a 30 s wait before the retry, then closes.
        Backoff,
    }

    /// A provider on 127.0.0.1 that counts requests and signals each one
    /// once it has read it whole. Connections with no reply queued get a
    /// 200 with [`TRANSCRIPT`].
    struct RawProvider {
        url: String,
        requests: Arc<AtomicUsize>,
        arrived: UnboundedReceiver<()>,
        replies: Arc<Mutex<std::collections::VecDeque<RawReply>>>,
    }

    impl RawProvider {
        fn start() -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let requests = Arc::new(AtomicUsize::new(0));
            let (arrived_tx, arrived) = unbounded_channel();
            let replies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
            let (count, queue) = (Arc::clone(&requests), Arc::clone(&replies));
            // Ends with the test process; it holds nothing on disk.
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { return };
                    let (count, arrived) = (Arc::clone(&count), arrived_tx.clone());
                    let reply = queue.lock().unwrap().pop_front();
                    std::thread::spawn(move || serve_raw(stream, reply, &count, &arrived));
                }
            });
            Self {
                url,
                requests,
                arrived,
                replies,
            }
        }

        fn queue(&self, reply: RawReply) {
            self.replies.lock().unwrap().push_back(reply);
        }

        fn requests(&self) -> usize {
            self.requests.load(Ordering::SeqCst)
        }
    }

    fn serve_raw(
        mut stream: std::net::TcpStream,
        reply: Option<RawReply>,
        count: &AtomicUsize,
        arrived: &UnboundedSender<()>,
    ) {
        if read_raw_request(&mut stream).is_none() {
            return;
        }
        count.fetch_add(1, Ordering::SeqCst);
        let _ = arrived.send(());
        let ok = {
            let body = serde_json::json!({ "text": TRANSCRIPT }).to_string();
            format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
        };
        match reply {
            None => {
                let _ = stream.write_all(ok.as_bytes());
            }
            Some(RawReply::Backoff) => {
                let _ = stream.write_all(
                    b"HTTP/1.1 503 Service Unavailable\r\nretry-after: 30\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                );
            }
            Some(RawReply::Held {
                closed,
                release,
                sent,
            }) => {
                // Read until the client hangs up (EOF or a reset).
                let mut buf = [0u8; 256];
                while matches!(stream.read(&mut buf), Ok(n) if n > 0) {}
                let _ = closed.send(());
                let _ = release.recv_timeout(WAIT);
                let _ = stream.write_all(ok.as_bytes());
                let _ = sent.send(());
            }
        }
    }

    /// Reads one request: the head, then `content-length` bytes of body.
    fn read_raw_request(stream: &mut std::net::TcpStream) -> Option<()> {
        let mut data = Vec::new();
        let mut chunk = [0u8; 4096];
        let head_end = loop {
            let n = stream.read(&mut chunk).ok().filter(|n| *n > 0)?;
            data.extend_from_slice(&chunk[..n]);
            if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                break at + 4;
            }
        };
        let head = String::from_utf8_lossy(&data[..head_end]).to_ascii_lowercase();
        let length: usize = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0);
        while data.len() < head_end + length {
            let n = stream.read(&mut chunk).ok().filter(|n| *n > 0)?;
            data.extend_from_slice(&chunk[..n]);
        }
        Some(())
    }

    /// Waits for one signal on `rx`, or fails the test.
    async fn fenced(rx: &mut UnboundedReceiver<()>, what: &str) {
        match tokio::time::timeout(WAIT, rx.recv()).await {
            Ok(Some(())) => {}
            Ok(None) => panic!("{what}: the signal's sender is gone"),
            Err(_) => panic!("timed out waiting for {what}"),
        }
    }

    /// The real engine behind the pipeline, and a signal when the pipeline
    /// lets go of it: the last holder is the transcription job, so once this
    /// is dropped the job (its request, its backoff) is gone for good.
    struct DropProbe {
        inner: Transcribers,
        dropped: UnboundedSender<()>,
    }

    impl Drop for DropProbe {
        fn drop(&mut self) {
            let _ = self.dropped.send(());
        }
    }

    impl Transcriber for DropProbe {
        async fn transcribe(&self, audio: &Audio) -> Result<String, TranscribeError> {
            self.inner.transcribe(audio).await
        }

        async fn transcribe_detailed(&self, audio: &Audio) -> Result<Transcript, TranscribeError> {
            self.inner.transcribe_detailed(audio).await
        }
    }

    /// Settings with a saved key and the engine they build, pointed at
    /// `provider`.
    struct RawSetup {
        dir: TempDir,
        wav: std::path::PathBuf,
        transcribers: Transcribers,
    }

    impl RawSetup {
        fn new(tag: &str, provider: &RawProvider) -> Self {
            let dir = TempDir::new(tag);
            let config = Arc::new(Config::new(dir.0.join("config"), MemSecretStore::new()));
            let transcribers = Transcribers::new(starting());
            let live = LiveEngine::follow(transcribers.clone())
                .at(&provider.url)
                .with_key_timeout(WAIT);
            changed(&config, &live, |c| c.set_api_key(Provider::Groq, KEY)).unwrap();
            assert_eq!(transcribers.name(), "remote");
            let wav = dir.0.join("speech.wav");
            let wav_bytes = dictation_core::wav::encode_f32(&vec![0.25; 8_000], 16_000).unwrap();
            std::fs::write(&wav, wav_bytes).unwrap();
            Self {
                dir,
                wav,
                transcribers,
            }
        }
    }

    /// One running pipeline with a fake hotkey and inserter.
    struct Run {
        pipeline: Pipeline,
        ui: UiReceiver,
        hotkey: FakeHotkey,
        clock: FakeClock,
        inserter: FakeInserter,
    }

    impl Run {
        fn spawn<T: Transcriber + 'static>(wav: &std::path::Path, transcriber: T) -> Self {
            let hotkey = FakeHotkey::new();
            let clock = FakeClock::at(Duration::from_secs(100));
            let inserter = FakeInserter::new();
            let ports = Ports {
                hotkey: hotkey.clone(),
                audio: FileAudioSource::new(wav),
                transcriber,
                inserter: inserter.clone(),
                permissions: FakePermissions::all_granted(),
                clock: clock.clone(),
            };
            let (pipeline, ui) = Pipeline::spawn(ports, PipelineConfig::default()).unwrap();
            Self {
                pipeline,
                ui,
                hotkey,
                clock,
                inserter,
            }
        }

        fn press_and_release(&self) {
            assert!(self.hotkey.emit(HotkeyEvent::Pressed));
            self.clock.advance(Duration::from_millis(500));
            assert!(self.hotkey.emit(HotkeyEvent::Released));
        }

        async fn next(&mut self) -> Option<UiEvent> {
            tokio::time::timeout(WAIT, self.ui.recv())
                .await
                .expect("no UI event in time")
        }

        /// UI events up to Transcribing: the job has started.
        async fn until_transcribing(&mut self) {
            for phase in [Recording, Transcribing] {
                assert_eq!(self.next().await, Some(UiEvent::StateChanged(phase)));
            }
        }

        /// Stops the pipeline, which must not wait for the provider, and
        /// returns every UI event it sent after that.
        async fn shut_down(self) -> (Vec<UiEvent>, FakeInserter) {
            let Self {
                pipeline,
                mut ui,
                inserter,
                ..
            } = self;
            tokio::time::timeout(WAIT, pipeline.shutdown())
                .await
                .expect("shutdown waited for the provider");
            let mut rest = Vec::new();
            while let Some(event) = tokio::time::timeout(WAIT, ui.recv())
                .await
                .expect("the UI stream didn't end after shutdown")
            {
                rest.push(event);
            }
            (rest, inserter)
        }
    }

    /// After a shutdown, a new pipeline on the same engine dictates normally.
    async fn a_fresh_pipeline_pastes(setup: &RawSetup, provider: &mut RawProvider) {
        let mut run = Run::spawn(&setup.wav, setup.transcribers.clone());
        run.press_and_release();
        let mut events = Vec::new();
        while let Some(event) = run.next().await {
            let idle = event == UiEvent::StateChanged(Idle);
            events.push(event);
            if idle {
                break;
            }
        }
        assert_eq!(events, pasted());
        assert_eq!(run.inserter.inserted(), [TRANSCRIPT]);
        fenced(&mut provider.arrived, "the fresh pipeline's request").await;
        run.pipeline.shutdown().await;
    }

    fn no_insert(after: &[UiEvent], inserter: &FakeInserter) {
        assert!(
            !after.contains(&UiEvent::StateChanged(Inserting)),
            "inserting after shutdown: {after:?}"
        );
        assert!(inserter.inserted().is_empty(), "pasted after shutdown");
    }

    #[tokio::test]
    async fn pipeline_shutdown_closes_real_http_without_retry_or_insert() {
        let mut provider = RawProvider::start();
        let (closed_tx, mut closed) = unbounded_channel();
        let (sent_tx, mut sent) = unbounded_channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        provider.queue(RawReply::Held {
            closed: closed_tx,
            release: release_rx,
            sent: sent_tx,
        });
        let setup = RawSetup::new("quit-in-flight", &provider);
        let (dropped_tx, mut dropped) = unbounded_channel();
        let mut run = Run::spawn(
            &setup.wav,
            DropProbe {
                inner: setup.transcribers.clone(),
                dropped: dropped_tx,
            },
        );

        run.press_and_release();
        run.until_transcribing().await;
        fenced(&mut provider.arrived, "the request").await;
        // Quit while the provider holds the reply: shutdown returns without it.
        let (after, inserter) = run.shut_down().await;
        // The request was cancelled: the client closed the connection, and
        // the job holding it is gone.
        fenced(&mut closed, "the client to close the connection").await;
        fenced(&mut dropped, "the transcription job to be dropped").await;
        // The reply the user would have had pasted arrives late.
        release.send(()).unwrap();
        fenced(&mut sent, "the late reply").await;

        no_insert(&after, &inserter);
        assert_eq!(provider.requests(), 1, "retried after shutdown");
        a_fresh_pipeline_pastes(&setup, &mut provider).await;
        assert_eq!(provider.requests(), 2);
        drop(setup.dir);
    }

    /// Writes nothing; signals each time the engine logs that it is waiting
    /// to retry (its `retrying` debug line).
    #[derive(Clone)]
    struct RetrySignal(UnboundedSender<()>);

    impl std::io::Write for RetrySignal {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if String::from_utf8_lossy(buf).contains(" retrying ") {
                let _ = self.0.send(());
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn pipeline_shutdown_during_retry_backoff_sends_no_retry_or_insert() {
        // The test's runtime has one thread, so the engine logs here.
        let (retrying_tx, mut retrying) = unbounded_channel();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .with_writer(move || RetrySignal(retrying_tx.clone()))
            .finish();
        let _logging = tracing::subscriber::set_default(subscriber);

        let mut provider = RawProvider::start();
        provider.queue(RawReply::Backoff);
        let setup = RawSetup::new("quit-in-backoff", &provider);
        let (dropped_tx, mut dropped) = unbounded_channel();
        let mut run = Run::spawn(
            &setup.wav,
            DropProbe {
                inner: setup.transcribers.clone(),
                dropped: dropped_tx,
            },
        );

        run.press_and_release();
        run.until_transcribing().await;
        fenced(&mut provider.arrived, "the first request").await;
        // The 503 came back and the engine is waiting out its 30 s.
        fenced(&mut retrying, "the engine to back off").await;
        let (after, inserter) = run.shut_down().await;
        // The job, and the retry it was waiting to send, are gone: no second
        // request can follow.
        fenced(&mut dropped, "the transcription job to be dropped").await;

        no_insert(&after, &inserter);
        assert_eq!(provider.requests(), 1, "retried after shutdown");
        a_fresh_pipeline_pastes(&setup, &mut provider).await;
        assert_eq!(provider.requests(), 2);
        drop(setup.dir);
    }
}
