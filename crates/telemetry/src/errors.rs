//! Error and crash reports to Sentry (A2, `docs/privacy.md`).
//!
//! The adapter for the [`ErrorReporter`] port. It speaks Sentry's envelope
//! protocol directly, with the same TLS setup as the PostHog adapter, and
//! builds every event **from an allow-list**: there is no code path that
//! copies a message, a breadcrumb, a user, a host name or an environment
//! variable into one, so nothing like that can leak through a new field the
//! Sentry SDK adds later. (Why not the `sentry` crate: it collects and then
//! removes; see the spike in `docs/history/milestones/A2.md`.)
//!
//! - **A no-op without a DSN.** [`Config::dsn`] comes from the build
//!   (`DICTATION_SENTRY_DSN`); dev builds, CI and forks have none, so nothing
//!   is built, sent or written.
//! - **What an event holds:** the app version as `release`, `environment`,
//!   tags for the E1 error code, the A1 error type, the CPU family and the
//!   macOS major version, and for a panic its source location and a stack of
//!   function names with crate-relative file names. Never a message.
//! - **Panics** are written (already scrubbed) to a file in the app's data
//!   folder and sent at the next launch, so a panic that aborts the process
//!   still gets reported, and nothing depends on the dying process.
//! - **Off means off.** [`SentryReporter::set_enabled`]`(false)` stops
//!   requests under way, refuses new reports and deletes a waiting panic
//!   report; on again re-enables.
//! - Native crashes (minidumps) are not captured: a minidump holds raw
//!   memory, which no scrubber can promise to be free of text.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dictation_core::diagnostics::ErrorCode;
use dictation_core::error::DictationError;
use dictation_ports::{Arch, ErrorReporter, ErrorType, Version};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::sync::watch;

/// The file a panic report waits in, in the data folder.
const PENDING_FILE: &str = "pending-panic.envelope";
/// Reports in flight at once; more are dropped.
const MAX_IN_FLIGHT: usize = 4;
/// Reports sent in one run of the app; more are dropped (a failure loop
/// must not become a flood).
const MAX_PER_RUN: usize = 30;
/// Frames kept from a stack.
const MAX_FRAMES: usize = 40;
/// The longest function name kept.
const MAX_FUNCTION_LEN: usize = 200;

/// A parsed Sentry DSN: `https://<public key>@<host>/<project id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dsn {
    key: String,
    scheme: String,
    host: String,
    project: String,
}

impl Dsn {
    /// `text` as a DSN, or `None`. Only `https` is accepted, except for a
    /// loopback host (tests). A secret key, a path or a query is refused.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (scheme, rest) = text.trim().split_once("://")?;
        let (key, rest) = rest.split_once('@')?;
        let (host, project) = rest.split_once('/')?;
        let loopback = host.starts_with("127.0.0.1") || host.starts_with("localhost");
        let scheme_ok = scheme == "https" || (scheme == "http" && loopback);
        let key_ok = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric());
        let host_ok = !host.is_empty()
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
        let project_ok = !project.is_empty() && project.chars().all(|c| c.is_ascii_digit());
        (scheme_ok && key_ok && host_ok && project_ok).then(|| Self {
            key: key.to_owned(),
            scheme: scheme.to_owned(),
            host: host.to_owned(),
            project: project.to_owned(),
        })
    }

    fn envelope_url(&self) -> String {
        format!(
            "{}://{}/api/{}/envelope/",
            self.scheme, self.host, self.project
        )
    }

    fn auth(&self) -> String {
        format!(
            "Sentry sentry_version=7, sentry_client=dictation-app/{}, sentry_key={}",
            env!("CARGO_PKG_VERSION"),
            self.key
        )
    }
}

/// Adapter settings.
#[derive(Debug, Clone)]
pub struct Config {
    /// The project DSN. `None` (or one that doesn't parse) makes the adapter
    /// a no-op.
    pub dsn: Option<Dsn>,
    /// The app version, sent as `release`.
    pub release: String,
    /// `development` or `production`.
    pub environment: &'static str,
    /// The CPU family, a tag.
    pub arch: Arch,
    /// The macOS major version, a tag (0 if unknown).
    pub macos_major: u16,
    /// Where a waiting panic report lives.
    pub dir: PathBuf,
    /// How long one request may take.
    pub timeout: Duration,
}

impl Config {
    /// The defaults for `dsn` (blank or malformed means none).
    #[must_use]
    pub fn new(
        dsn: Option<&str>,
        release: &Version,
        environment: &'static str,
        macos_major: u16,
        dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            dsn: dsn.and_then(Dsn::parse),
            release: release.as_str().to_owned(),
            environment,
            arch: Arch::current(),
            macos_major,
            dir: dir.into(),
            timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Default)]
struct State {
    enabled: bool,
    /// Bumped every time sharing is turned off.
    generation: u64,
    in_flight: usize,
    sent: usize,
}

/// The adapter. Share it as an `Arc`.
pub struct SentryReporter {
    config: Config,
    client: Option<Client>,
    runtime: Option<tokio::runtime::Handle>,
    state: Arc<Mutex<State>>,
    /// Carries the opt-out generation, to stop requests under way.
    cancel: watch::Sender<u64>,
}

impl SentryReporter {
    /// A reporter with no DSN: reports and writes nothing.
    #[must_use]
    pub fn disabled() -> Arc<Self> {
        Arc::new(Self {
            config: Config {
                dsn: None,
                release: String::new(),
                environment: "development",
                arch: Arch::Other,
                macos_major: 0,
                dir: PathBuf::new(),
                timeout: Duration::from_secs(1),
            },
            client: None,
            runtime: None,
            state: Arc::default(),
            cancel: watch::channel(0).0,
        })
    }

    /// Starts the reporter; `enabled` is the saved choice. With a DSN, a
    /// panic report left by the last run is sent now (if enabled) or deleted
    /// (if not). Without one, nothing runs and nothing is written.
    #[must_use]
    pub fn start(config: Config, enabled: bool, runtime: &tokio::runtime::Handle) -> Arc<Self> {
        let live = config.dsn.is_some();
        let client = if live {
            crate::build_client(config.timeout)
        } else {
            None
        };
        let this = Arc::new(Self {
            config,
            client,
            runtime: live.then(|| runtime.clone()),
            state: Arc::default(),
            cancel: watch::channel(0).0,
        });
        if live {
            this.set_enabled(enabled);
        }
        this
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Nothing panics while holding it; recover if something did.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn pending_path(&self) -> PathBuf {
        self.config.dir.join(PENDING_FILE)
    }

    /// Whether reports are being sent.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.lock().enabled
    }

    /// Turns reporting on or off. Off refuses new reports, stops requests
    /// under way and deletes a waiting panic report before returning; on
    /// (from off) sends a waiting panic report. A no-op without a DSN.
    pub fn set_enabled(&self, enabled: bool) {
        if self.config.dsn.is_none() {
            return;
        }
        let mut state = self.lock();
        if enabled == state.enabled && (enabled || state.generation > 0) {
            return;
        }
        state.enabled = enabled;
        if enabled {
            drop(state);
            if let Ok(body) = std::fs::read(self.pending_path()) {
                let _ = std::fs::remove_file(self.pending_path());
                self.send(body);
            }
        } else {
            state.generation += 1;
            self.cancel.send_replace(state.generation);
            drop(state);
            let _ = std::fs::remove_file(self.pending_path());
        }
    }

    /// Sends `body` (an envelope) in the background, within the limits.
    fn send(&self, body: Vec<u8>) {
        let (Some(dsn), Some(client), Some(runtime)) =
            (&self.config.dsn, &self.client, &self.runtime)
        else {
            return;
        };
        let generation = {
            let mut state = self.lock();
            if !state.enabled || state.in_flight >= MAX_IN_FLIGHT || state.sent >= MAX_PER_RUN {
                return;
            }
            state.in_flight += 1;
            state.sent += 1;
            state.generation
        };
        let request = client
            .post(dsn.envelope_url())
            .header("X-Sentry-Auth", dsn.auth())
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-sentry-envelope",
            )
            .timeout(self.config.timeout)
            .body(body);
        let mut cancelled = self.cancel.subscribe();
        let state = Arc::clone(&self.state);
        runtime.spawn(async move {
            // One attempt; a failure drops the report.
            tokio::select! {
                _ = request.send() => {}
                _ = cancelled.wait_for(|now| *now != generation) => {}
            }
            state
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .in_flight -= 1;
        });
    }

    /// How many reports are on their way.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.lock().in_flight
    }

    /// Records a panic at `location` (`file`, `line`) with the stack text
    /// from [`std::backtrace::Backtrace`], for the next launch. The panic's
    /// message is not an input: it can't be sent. Called from the panic
    /// hook, so it only writes a small file.
    pub fn record_panic(&self, location: Option<(&str, u32)>, backtrace: &str) {
        if self.config.dsn.is_none() || !self.lock().enabled {
            return;
        }
        let value = match location {
            Some((file, line)) => format!("panicked at {}:{line}", crate_relative(file)),
            None => "panicked".to_owned(),
        };
        let event = self.event(
            "fatal",
            "panic",
            Some(value),
            parse_backtrace(backtrace),
            None,
        );
        let body = envelope(&event);
        let path = self.pending_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, body);
    }

    /// Installs a panic hook that calls [`Self::record_panic`] and then the
    /// hook that was there. Does nothing without a DSN.
    pub fn install_panic_hook(self: &Arc<Self>) {
        if self.config.dsn.is_none() {
            return;
        }
        let reporter = Arc::clone(self);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let location = info.location().map(|l| (l.file().to_owned(), l.line()));
            let trace = std::backtrace::Backtrace::force_capture().to_string();
            reporter.record_panic(location.as_ref().map(|(f, l)| (f.as_str(), *l)), &trace);
            previous(info);
        }));
    }

    /// An event built from allow-listed parts.
    fn event(
        &self,
        level: &str,
        kind: &str,
        value: Option<String>,
        frames: Vec<Value>,
        tags: Option<(ErrorCode, ErrorType)>,
    ) -> Value {
        let mut tag_map = serde_json::Map::new();
        tag_map.insert("arch".into(), json!(self.config.arch));
        tag_map.insert("macos_major".into(), json!(self.config.macos_major));
        if let Some((code, error_type)) = tags {
            tag_map.insert("error_code".into(), json!(code));
            tag_map.insert("error_type".into(), json!(error_type));
        }
        let mut exception = json!({ "type": kind });
        if let Some(value) = value {
            exception["value"] = json!(value);
        }
        if !frames.is_empty() {
            exception["stacktrace"] = json!({ "frames": frames });
        }
        json!({
            "event_id": event_id(),
            "timestamp": now(),
            "platform": "other",
            "level": level,
            "release": self.config.release,
            "environment": self.config.environment,
            "sdk": { "name": "dictation-app", "version": env!("CARGO_PKG_VERSION") },
            "tags": tag_map,
            "fingerprint": [kind],
            "exception": { "values": [exception] },
        })
    }

    /// The envelope for a dictation error: its E1 code and A1 type, never
    /// its text.
    fn error_envelope(&self, error: &DictationError) -> Vec<u8> {
        let code = ErrorCode::of(error);
        let code_name = serde_json::to_value(code)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        let event = self.event(
            "error",
            &code_name,
            Some(code_name.clone()),
            Vec::new(),
            Some((code, ErrorType::from(error))),
        );
        envelope(&event)
    }
}

impl ErrorReporter for SentryReporter {
    fn report(&self, error: &DictationError) {
        if self.config.dsn.is_none() || !self.lock().enabled {
            return;
        }
        self.send(self.error_envelope(error));
    }
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// 32 random hex digits.
fn event_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        bytes = [7; 16];
    }
    bytes.iter().fold(String::new(), |mut out, b| {
        out.push_str(&format!("{b:02x}"));
        out
    })
}

/// A Sentry envelope holding `event`.
fn envelope(event: &Value) -> Vec<u8> {
    let payload = event.to_string();
    let id = event["event_id"].as_str().unwrap_or_default();
    format!(
        "{}\n{}\n{payload}\n",
        json!({ "event_id": id }),
        json!({ "type": "event", "length": payload.len() })
    )
    .into_bytes()
}

/// `path` as a crate-relative name: `crates/x/src/y.rs` for our code,
/// `dep-1.2.3/src/z.rs` for a dependency, `library/std/src/…` for the
/// standard library, and the bare file name for anything else. A home
/// directory, a user name or a project folder never survives.
#[must_use]
pub fn crate_relative(path: &str) -> String {
    let path = path.replace('\\', "/");
    for marker in ["/crates/", "/src-tauri/"] {
        if let Some(at) = path.find(marker) {
            return path[at + 1..].to_owned();
        }
    }
    if path.starts_with("crates/") || path.starts_with("src-tauri/") {
        return path;
    }
    if let Some(rest) = path.split("/registry/src/").nth(1) {
        // `<index dir>/<crate-version>/…`
        if let Some((_, dep)) = rest.split_once('/') {
            return dep.to_owned();
        }
    }
    if let Some(rest) = path.strip_prefix("/rustc/") {
        // `<commit>/library/…`
        if let Some((_, lib)) = rest.split_once('/') {
            return lib.to_owned();
        }
    }
    path.rsplit('/').next().unwrap_or_default().to_owned()
}

/// A function name from a stack: code names only (ASCII, no spaces beyond
/// those generics use), cut short.
fn clean_function(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(MAX_FUNCTION_LEN)
        .collect()
}

/// Sentry frames (oldest first) from the text of a
/// [`std::backtrace::Backtrace`]: function names and crate-relative files.
fn parse_backtrace(text: &str) -> Vec<Value> {
    let mut frames: Vec<(String, Option<(String, Option<u32>)>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(at) = line.strip_prefix("at ") {
            // `path:line:col`
            let mut parts = at.rsplitn(3, ':');
            let (_col, row, file) = (parts.next(), parts.next(), parts.next());
            if let (Some(row), Some(file), Some(last)) = (row, file, frames.last_mut()) {
                last.1 = Some((crate_relative(file), row.parse().ok()));
            }
        } else if let Some((index, function)) = line.split_once(": ") {
            if !index.is_empty() && index.chars().all(|c| c.is_ascii_digit()) {
                frames.push((clean_function(function), None));
            }
        }
    }
    frames
        .into_iter()
        .take(MAX_FRAMES)
        .map(|(function, place)| {
            let mut frame = json!({
                "function": function,
                "in_app": function.starts_with("dictation_"),
            });
            if let Some((file, row)) = place {
                frame["filename"] = json!(file);
                if let Some(row) = row {
                    frame["lineno"] = json!(row);
                }
            }
            frame
        })
        .rev()
        .collect()
}

#[cfg(test)]
mod tests;
