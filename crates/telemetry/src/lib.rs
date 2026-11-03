//! The PostHog adapter for the [`Telemetry`] port (A1, `docs/privacy.md`).
//!
//! - **Sends from Rust**, never the webview, to PostHog Cloud US
//!   ([`POSTHOG_HOST`]).
//! - **A no-op without a key.** [`Config::api_key`] comes from the build
//!   (`DICTATION_POSTHOG_KEY`); dev builds, CI and forks have none, so
//!   nothing is queued, sent or written (not even the install ID).
//! - **Never blocks.** [`Telemetry::record`] takes a lock, pushes to a queue
//!   of at most [`Config::queue_cap`] events (a full queue drops the new
//!   event) and returns. A task on the tokio runtime sends batches.
//! - **Drops on failure.** One attempt per batch, with a timeout. If it
//!   fails, the batch and everything queued are dropped: no retry loop.
//! - **Anonymous.** `distinct_id` is the random [`install_id`]; every event
//!   carries `$process_person_profile: false` and `$geoip_disable: true`,
//!   and never an `$ip`. (The PostHog project must also be set to discard
//!   client IP data; see `docs/privacy.md`.)
//! - **Off means off.** [`PostHogTelemetry::set_enabled`]`(false)` clears the
//!   queue and deletes the install ID at once; turning it on again makes a
//!   new ID.

pub mod install_id;
mod version_marker;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use dictation_ports::{Event, Telemetry};
use reqwest::Client;
use rustls_platform_verifier::BuilderVerifierExt;
use serde_json::{Value, json};
use tokio::sync::Notify;

pub use version_marker::updated_from;

/// PostHog Cloud US's ingestion host.
pub const POSTHOG_HOST: &str = "https://us.i.posthog.com";

/// Adapter settings.
#[derive(Debug, Clone)]
pub struct Config {
    /// The PostHog project key. `None` (or blank) makes the adapter a no-op.
    pub api_key: Option<String>,
    /// The ingestion host; [`POSTHOG_HOST`] outside tests.
    pub host: String,
    /// Where the install ID lives.
    pub dir: PathBuf,
    /// How often queued events are sent.
    pub flush_interval: Duration,
    /// The most events in one request; reaching it sends early.
    pub batch_size: usize,
    /// The most events waiting. More are dropped.
    pub queue_cap: usize,
    /// How long one request may take.
    pub timeout: Duration,
}

impl Config {
    /// The defaults for `api_key`, storing the install ID in `dir`.
    #[must_use]
    pub fn new(api_key: Option<&str>, dir: impl Into<PathBuf>) -> Self {
        Self {
            api_key: api_key
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .map(str::to_owned),
            host: POSTHOG_HOST.to_owned(),
            dir: dir.into(),
            flush_interval: Duration::from_secs(60),
            batch_size: 20,
            queue_cap: 200,
            timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Default)]
struct State {
    /// The install ID while sharing is on; `None` is off.
    id: Option<String>,
    queue: VecDeque<Value>,
}

/// The adapter. Share it as an `Arc`.
pub struct PostHogTelemetry {
    config: Config,
    client: Option<Client>,
    state: Mutex<State>,
    wake: Notify,
}

impl PostHogTelemetry {
    /// An adapter with no key: records and sends nothing, writes nothing.
    #[must_use]
    pub fn disabled() -> Arc<Self> {
        Arc::new(Self {
            config: Config::new(None, ""),
            client: None,
            state: Mutex::new(State::default()),
            wake: Notify::new(),
        })
    }

    /// Starts the adapter; `enabled` is the saved choice. With a key, it
    /// spawns the sending task on `runtime`. Without one, nothing runs and
    /// nothing is written.
    #[must_use]
    pub fn start(config: Config, enabled: bool, runtime: &tokio::runtime::Handle) -> Arc<Self> {
        let live = config.api_key.is_some();
        let client = if live { build_client(&config) } else { None };
        let this = Arc::new(Self {
            config,
            client,
            state: Mutex::new(State::default()),
            wake: Notify::new(),
        });
        if live {
            this.set_enabled(enabled);
            runtime.spawn(send_loop(Arc::downgrade(&this)));
        }
        this
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Nothing panics while holding it; recover if something did.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether events are being queued and sent.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.lock().id.is_some()
    }

    /// Turns sharing on or off. Off clears the queue and deletes the install
    /// ID before returning; on (from off) makes a new ID. A no-op without a
    /// key.
    pub fn set_enabled(&self, enabled: bool) {
        if self.config.api_key.is_none() {
            return;
        }
        let mut state = self.lock();
        if enabled {
            if state.id.is_none() {
                state.id = install_id::load_or_create(&self.config.dir);
            }
        } else {
            state.id = None;
            state.queue.clear();
            install_id::forget(&self.config.dir);
        }
    }

    /// Sends one batch of what is queued, once. Returns whether more is
    /// waiting to go. A failure drops the batch and the rest of the queue.
    async fn flush_once(&self) -> bool {
        let (Some(key), Some(client)) = (&self.config.api_key, &self.client) else {
            return false;
        };
        let batch: Vec<Value> = {
            let mut state = self.lock();
            let take = state.queue.len().min(self.config.batch_size.max(1));
            state.queue.drain(..take).collect()
        };
        if batch.is_empty() {
            return false;
        }
        let body = json!({ "api_key": key, "batch": batch }).to_string();
        let sent = client
            .post(format!("{}/batch/", self.config.host.trim_end_matches('/')))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .timeout(self.config.timeout)
            .body(body)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        let mut state = self.lock();
        if !sent {
            tracing::debug!("usage events not sent; dropped");
            state.queue.clear();
        }
        sent && !state.queue.is_empty()
    }

    /// Sends everything queued, for a quiet shutdown. Gives up on the first
    /// failure.
    pub async fn flush(&self) {
        while self.flush_once().await {}
    }

    /// How many events wait to be sent.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.lock().queue.len()
    }
}

impl Telemetry for PostHogTelemetry {
    fn record(&self, event: Event) {
        let mut state = self.lock();
        let Some(id) = &state.id else {
            return;
        };
        let Ok(Value::Object(parts)) = serde_json::to_value(&event) else {
            return;
        };
        let mut properties = match parts.get("properties") {
            Some(Value::Object(properties)) => properties.clone(),
            _ => serde_json::Map::new(),
        };
        properties.insert("$process_person_profile".into(), json!(false));
        properties.insert("$geoip_disable".into(), json!(true));
        properties.insert("$lib".into(), json!("dictation-app"));
        let name = parts.get("event").cloned().unwrap_or(Value::Null);
        let item = json!({ "event": name, "distinct_id": id, "properties": properties });
        if state.queue.len() >= self.config.queue_cap {
            return;
        }
        state.queue.push_back(item);
        let full = state.queue.len() >= self.config.batch_size;
        drop(state);
        if full {
            self.wake.notify_one();
        }
    }
}

/// Sends batches until the adapter is dropped.
async fn send_loop(this: Weak<PostHogTelemetry>) {
    loop {
        let Some(telemetry) = this.upgrade() else {
            return;
        };
        // Holds the adapter while waiting, so the task ends within one
        // interval of the last other handle going.
        tokio::select! {
            () = telemetry.wake.notified() => {}
            () = tokio::time::sleep(telemetry.config.flush_interval) => {}
        }
        telemetry.flush().await;
    }
}

/// A client with rustls + ring and the OS trust store, as the transcribers
/// use. `None` if the TLS setup fails: nothing is sent then.
fn build_client(config: &Config) -> Option<Client> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .ok()?
        .with_platform_verifier()
        .ok()?
        .with_no_client_auth();
    Client::builder()
        .tls_backend_preconfigured(tls)
        .timeout(config.timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()
}

#[cfg(test)]
mod tests {
    use dictation_ports::{ErrorType, SettingName};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const KEY: &str = "phc_test_not_a_real_key";

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dictation-ph-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn config(server: &MockServer, dir: &PathBuf) -> Config {
        let mut config = Config::new(Some(KEY), dir);
        config.host = server.uri();
        config.flush_interval = Duration::from_secs(3600);
        config
    }

    fn failed() -> Event {
        Event::DictationFailed {
            error_type: ErrorType::Transcription,
        }
    }

    async fn bodies(server: &MockServer) -> Vec<Value> {
        let requests = server.received_requests().await.unwrap_or_default();
        requests
            .iter()
            .map(|r| serde_json::from_slice(&r.body).unwrap())
            .collect()
    }

    #[test]
    fn the_default_host_is_posthog_us() {
        assert_eq!(POSTHOG_HOST, "https://us.i.posthog.com");
        assert_eq!(Config::new(Some("k"), "x").host, POSTHOG_HOST);
    }

    #[tokio::test]
    async fn without_a_key_nothing_is_queued_sent_or_written() {
        let dir = temp("nokey");
        for key in [None, Some(""), Some("  ")] {
            let telemetry = PostHogTelemetry::start(
                Config::new(key, &dir),
                true,
                &tokio::runtime::Handle::current(),
            );
            telemetry.record(failed());
            assert!(!telemetry.is_enabled());
            assert_eq!(telemetry.queued(), 0);
            telemetry.flush().await;
        }
        assert!(!dir.exists(), "no install ID without a key");
    }

    #[tokio::test]
    async fn events_go_out_as_one_anonymous_batch() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/batch/"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let dir = temp("batch");
        let telemetry = PostHogTelemetry::start(
            config(&server, &dir),
            true,
            &tokio::runtime::Handle::current(),
        );
        telemetry.record(failed());
        telemetry.record(Event::SettingChanged {
            setting: SettingName::Hotkey,
        });
        assert_eq!(telemetry.queued(), 2);
        telemetry.flush().await;
        assert_eq!(telemetry.queued(), 0);

        let sent = bodies(&server).await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["api_key"], KEY);
        let batch = sent[0]["batch"].as_array().unwrap();
        assert_eq!(batch.len(), 2);
        let id = install_id::load_or_create(&dir).unwrap();
        for item in batch {
            assert_eq!(item["distinct_id"], id.as_str());
            let properties = item["properties"].as_object().unwrap();
            assert_eq!(properties["$process_person_profile"], false);
            assert_eq!(properties["$geoip_disable"], true);
            assert!(!properties.contains_key("$ip"));
            assert!(!properties.contains_key("$set"));
        }
        assert_eq!(batch[0]["event"], "dictation_failed");
        assert_eq!(batch[0]["properties"]["error_type"], "transcription");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn turning_it_off_clears_the_queue_and_the_id_and_sends_nothing() {
        let server = MockServer::start().await;
        let dir = temp("off");
        let telemetry = PostHogTelemetry::start(
            config(&server, &dir),
            true,
            &tokio::runtime::Handle::current(),
        );
        let first = install_id::load_or_create(&dir).unwrap();
        telemetry.record(failed());
        telemetry.set_enabled(false);
        assert_eq!(telemetry.queued(), 0);
        assert!(!telemetry.is_enabled());
        assert!(!dir.join(install_id::FILE).exists());
        telemetry.record(failed());
        telemetry.flush().await;
        assert!(bodies(&server).await.is_empty());

        telemetry.set_enabled(true);
        let second = install_id::load_or_create(&dir).unwrap();
        assert_ne!(first, second, "a new ID after off and on");
        telemetry.record(failed());
        assert_eq!(telemetry.queued(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_saved_off_choice_sends_and_stores_nothing() {
        let server = MockServer::start().await;
        let dir = temp("savedoff");
        let telemetry = PostHogTelemetry::start(
            config(&server, &dir),
            false,
            &tokio::runtime::Handle::current(),
        );
        telemetry.record(failed());
        assert_eq!(telemetry.queued(), 0);
        assert!(!dir.join(install_id::FILE).exists());
    }

    #[tokio::test]
    async fn a_failed_send_drops_everything_and_is_not_retried() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1..=2)
            .mount(&server)
            .await;
        let dir = temp("fail");
        let mut config = config(&server, &dir);
        config.batch_size = 1;
        let telemetry = PostHogTelemetry::start(config, true, &tokio::runtime::Handle::current());
        for _ in 0..5 {
            telemetry.record(failed());
        }
        telemetry.flush().await;
        assert_eq!(telemetry.queued(), 0, "the rest was dropped, not retried");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let requests = bodies(&server).await.len();
        assert!(
            (1..=2).contains(&requests),
            "{requests} requests for 5 events, no retries"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_unreachable_host_drops_silently() {
        let dir = temp("down");
        let mut config = Config::new(Some(KEY), &dir);
        config.host = "http://127.0.0.1:1".into();
        config.timeout = Duration::from_millis(500);
        let telemetry = PostHogTelemetry::start(config, true, &tokio::runtime::Handle::current());
        telemetry.record(failed());
        telemetry.flush().await;
        assert_eq!(telemetry.queued(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_queue_is_bounded() {
        let server = MockServer::start().await;
        let dir = temp("bound");
        let mut config = config(&server, &dir);
        config.queue_cap = 3;
        let telemetry = PostHogTelemetry::start(config, true, &tokio::runtime::Handle::current());
        for _ in 0..50 {
            telemetry.record(failed());
        }
        assert_eq!(telemetry.queued(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_background_task_sends_a_full_batch_without_being_asked() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let dir = temp("task");
        let mut config = config(&server, &dir);
        config.batch_size = 2;
        let telemetry = PostHogTelemetry::start(config, true, &tokio::runtime::Handle::current());
        telemetry.record(failed());
        telemetry.record(failed());
        for _ in 0..100 {
            if !bodies(&server).await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(bodies(&server).await.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
