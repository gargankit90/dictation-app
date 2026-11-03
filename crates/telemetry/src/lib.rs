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
