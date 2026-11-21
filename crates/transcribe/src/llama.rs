//! On-device cleanup (Q4): a small chat model (Gemma 4 E2B) run by
//! llama.cpp's `llama-server`, bundled with the app as a helper binary.
//!
//! Why a helper process and not a library: transcribe.cpp links its own
//! copy of ggml statically, and a second static ggml (llama.cpp's) in the
//! same binary would clash. A separate process also means a crash in the
//! model can't take the app down. (PR #326, "How to ship it".)
//!
//! [`LlamaServer::start`] launches it:
//!
//! - bound to `127.0.0.1` only, on a free port picked at launch;
//! - with a random API key made for this launch and passed in the
//!   environment (`LLAMA_API_KEY`, not the command line, which every local
//!   user can read), so other local programs can't use it;
//! - without its web UI, its slots endpoint or its log;
//! - with PR #326's settings: all layers on the GPU, one slot, a 4,096-token
//!   context, reasoning off.
//!
//! No orphans: it runs under a two-line `/bin/sh` supervisor that holds a
//! pipe from the app on its standard input. When the app closes the pipe
//! ([`LlamaServer::stop`], or dropping the server) *or the app dies*, for
//! any reason including a crash, the pipe closes, the supervisor's `read`
//! returns and it kills the server.
//!
//! [`ChatRunner`] is the seam for tests; [`LocalCleaner`] is the port's
//! [`Cleaner`]. A request that runs past the pipeline's deadline is
//! dropped, which closes its connection, and llama-server stops a
//! non-streamed generation whose client has gone (`wait_for_all` with the
//! request's `should_stop`), so the next dictation isn't queued behind it.
//!
//! Privacy: the transcript goes only to the loopback helper; neither it nor
//! the reply nor the key is logged.

use std::future::Future;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use dictation_core::redact;
use dictation_core::{cleanup, local_cleanup};
use dictation_ports::{Cleaner, CleanupError};
use reqwest::Client;
use serde_json::{Value, json};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

/// The helper's file name, next to the app's own executable (Tauri puts
/// `bundle.externalBin` there, in `Contents/MacOS`, and next to the dev
/// build's binary).
pub const HELPER_NAME: &str = "llama-server";

/// How long the helper may take to load the model and answer `/health`.
/// A first load after a reboot reads 3 GB from disk.
pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(90);

/// How often `/health` is asked while the helper starts.
const HEALTH_POLL: Duration = Duration::from_millis(100);

/// How long [`LlamaServer::shutdown_and_reap`] waits for the supervisor before
/// killing the server and the supervisor itself. The supervisor escalates
/// on its own after 2 s (SIGTERM, then SIGKILL), so this is a backstop.
const STOP_GRACE: Duration = Duration::from_secs(4);

/// The supervisor: starts the server (its path is `$0`, its arguments
/// `$@`) in the background and prints its process id. A watcher holds the
/// app's pipe (standard input, kept as descriptor 3 because a background
/// job's own standard input is `/dev/null`) and stops the server when the
/// pipe closes: SIGTERM, then SIGKILL 2 s later if it is still there. The shell waits for the server, so it exits when the server
/// does, for whatever reason, and the app sees it. The server's standard
/// input is `/dev/null` and descriptor 3 is opened after it starts, so only
/// the supervisor holds the pipe.
const SUPERVISOR: &str = r#""$0" "$@" </dev/null >/dev/null 2>&1 & pid=$!
echo "$pid"
exec 3<&0
{ read -r _ <&3; kill "$pid" 2>/dev/null; sleep 2; kill -9 "$pid" 2>/dev/null; } & watch=$!
wait "$pid"; kill "$watch" 2>/dev/null"#;

/// Why the helper or a cleanup failed. None holds the transcript, the
/// reply or the key.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LlamaError {
    /// The helper binary isn't there (a build without it).
    #[error("the on-device cleanup helper is missing")]
    HelperMissing,
    /// The helper couldn't be started, or exited while starting.
    #[error("couldn't start the on-device cleanup: {}", redact::summary(.0))]
    Start(String),
    /// The helper didn't become ready within [`STARTUP_TIMEOUT`].
    #[error("the on-device cleanup took too long to start")]
    StartTimeout,
    /// The request failed (the helper exited, or the connection broke).
    #[error("the on-device cleanup failed: {}", redact::summary(.0))]
    Request(String),
    /// The helper answered with an HTTP error.
    #[error("the on-device cleanup answered HTTP {0}")]
    Status(u16),
    /// The start was given up because it was no longer wanted.
    #[error("the on-device cleanup start was cancelled")]
    Cancelled,
    /// The answer wasn't a finished chat completion with text (any
    /// `finish_reason` but `stop` counts as unfinished).
    #[error("the on-device cleanup sent an unexpected answer")]
    Reply,
}

/// One chat completion: the seam between [`LocalCleaner`] and the helper.
pub trait ChatRunner: Send + Sync {
    /// The model's reply to `system` and `user`, at most `max_tokens` long,
    /// with PR #326's sampling (greedy, seed 1, no thinking).
    ///
    /// Dropping the future cancels the request.
    ///
    /// # Errors
    ///
    /// [`LlamaError::Request`], [`LlamaError::Status`] or
    /// [`LlamaError::Reply`].
    fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> impl Future<Output = Result<String, LlamaError>> + Send;

    /// Whether the runner can still answer; a helper that died after it
    /// started can't, and is started again.
    fn alive(&self) -> bool {
        true
    }
}

impl<R: ChatRunner> ChatRunner for Arc<R> {
    fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> impl Future<Output = Result<String, LlamaError>> + Send {
        (**self).complete(system, user, max_tokens)
    }

    fn alive(&self) -> bool {
        (**self).alive()
    }
}

/// The on-device [`Cleaner`] (Q4): Gemma with prompt v3, through a
/// [`ChatRunner`]. The pipeline's cleanup step adds the rest, as for the
/// cloud cleaner: the deadline ([`local_cleanup::TIMEOUT`], from
/// [`Cleaner::deadline`]), the guard (which checks the reply doesn't echo
/// prompt v3) and the raw-text fallback.
#[derive(Debug, Clone)]
pub struct LocalCleaner<R> {
    runner: R,
}

impl<R: ChatRunner> LocalCleaner<R> {
    /// Cleans with `runner`.
    #[must_use]
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: ChatRunner> Cleaner for LocalCleaner<R> {
    async fn clean(&self, text: &str) -> Result<String, CleanupError> {
        self.runner
            .complete(
                local_cleanup::SYSTEM_PROMPT,
                &cleanup::user_message(text),
                local_cleanup::max_tokens(text),
            )
            .await
            .map_err(|error| match error {
                LlamaError::Status(status) => CleanupError::Provider(status),
                LlamaError::Reply => CleanupError::BadReply,
                LlamaError::HelperMissing
                | LlamaError::Start(_)
                | LlamaError::StartTimeout
                | LlamaError::Cancelled
                | LlamaError::Request(_) => CleanupError::Network,
            })
    }

    fn deadline(&self) -> Option<Duration> {
        Some(local_cleanup::TIMEOUT)
    }

    fn prompt(&self) -> Option<&'static str> {
        Some(local_cleanup::SYSTEM_PROMPT)
    }
}

/// The running helper with one model loaded. Clones aren't offered: one
/// owner stops it ([`stop`](Self::stop) or drop).
pub struct LlamaServer {
    base_url: String,
    key: String,
    client: Client,
    supervisor: Mutex<Option<Supervised>>,
}

/// The supervisor process and the pipe that keeps the server alive.
struct Supervised {
    child: Child,
    stdin: Option<ChildStdin>,
    server_pid: u32,
}

impl std::fmt::Debug for LlamaServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the key.
        f.debug_struct("LlamaServer")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

/// The helper next to `exe` (the app's own executable), if it is there.
#[must_use]
pub fn helper_next_to(exe: &Path) -> Option<PathBuf> {
    let helper = exe.parent()?.join(HELPER_NAME);
    helper.is_file().then_some(helper)
}

/// The command-line arguments for `model` on `port`: PR #326's settings,
/// loopback only, no web UI, slots endpoint or log. The key is not here.
#[must_use]
pub fn server_args(model: &Path, port: u16) -> Vec<String> {
    let model = model.to_string_lossy().into_owned();
    let (port, context, seed) = (
        port.to_string(),
        local_cleanup::CONTEXT_TOKENS.to_string(),
        local_cleanup::SEED.to_string(),
    );
    [
        "-m",
        &model,
        "--host",
        "127.0.0.1",
        "--port",
        &port,
        "-ngl",
        "99",
        "-c",
        &context,
        "-np",
        "1",
        "--no-webui",
        "--no-slots",
        "--reasoning",
        "off",
        "--temp",
        "0",
        "--seed",
        &seed,
        "--log-disable",
    ]
    .iter()
    .map(|&arg| arg.to_owned())
    .collect()
}

/// A free loopback port. Another program could take it before the helper
/// binds it; the helper then fails to start and the next try picks another.
fn free_port() -> Result<u16, LlamaError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .map_err(|e| LlamaError::Start(format!("no free port: {e}")))?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|e| LlamaError::Start(format!("no free port: {e}")))
}

/// 32 random bytes as hex, for this launch's API key.
fn new_key() -> Result<String, LlamaError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| LlamaError::Start(format!("no randomness: {e}")))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl LlamaServer {
    /// Starts `helper` with `model` loaded and waits until it answers
    /// `/health`. Slow (seconds): the model is read and set up on the GPU.
    ///
    /// # Errors
    ///
    /// [`LlamaError::HelperMissing`], [`LlamaError::Start`] (it couldn't be
    /// launched, or exited) or [`LlamaError::StartTimeout`]. The helper is
    /// stopped on every error.
    pub async fn start(helper: &Path, model: &Path) -> Result<Self, LlamaError> {
        Self::start_until(helper, model, std::future::pending()).await
    }

    /// As [`start`](Self::start), given up when `cancel` finishes first:
    /// the helper is stopped and has exited (its model's memory is free)
    /// before this returns [`LlamaError::Cancelled`], so a start that
    /// follows never loads a second model beside it.
    ///
    /// # Errors
    ///
    /// As [`start`](Self::start), or [`LlamaError::Cancelled`].
    pub async fn start_until(
        helper: &Path,
        model: &Path,
        cancel: impl Future<Output = ()> + Send,
    ) -> Result<Self, LlamaError> {
        Self::start_within(helper, model, STARTUP_TIMEOUT, cancel).await
    }

    /// As [`start_until`](Self::start_until), giving the helper `limit` to
    /// get ready.
    async fn start_within(
        helper: &Path,
        model: &Path,
        limit: Duration,
        cancel: impl Future<Output = ()> + Send,
    ) -> Result<Self, LlamaError> {
        if !helper.is_file() {
            return Err(LlamaError::HelperMissing);
        }
        let port = free_port()?;
        let key = new_key()?;
        let base_url = format!("http://127.0.0.1:{port}");
        // Before the spawn: no error after it may leave a helper running.
        let client = crate::http::client(&base_url)
            .map_err(|e| LlamaError::Start(format!("no HTTP client: {e}")))?;
        let supervised = spawn_supervised(helper, &server_args(model, port), &key).await?;
        let server = Self {
            base_url,
            key,
            client,
            supervisor: Mutex::new(Some(supervised)),
        };
        // The select only decides the outcome; it never stops a process.
        let outcome = tokio::select! {
            ready = server.wait_ready(limit) => ready,
            () = cancel => Err(LlamaError::Cancelled),
        };
        if outcome.is_ok() {
            return Ok(server);
        }
        // Timed out, cancelled or failed: one shutdown, outside the select,
        // so a cancellation can't cut it short.
        server.shutdown_and_reap().await;
        outcome.map(|()| server)
    }

    /// Stops the helper and returns only once it is gone: the pipe closes
    /// (the supervisor sends SIGTERM, then SIGKILL), and the supervisor,
    /// which exits only after reaping the server, is reaped here. If it
    /// hasn't exited within [`STOP_GRACE`], the server and then the
    /// supervisor get SIGKILL, which can't be ignored, and the supervisor
    /// is reaped.
    async fn shutdown_and_reap(&self) {
        let supervised = lock(&self.supervisor).take();
        if let Some(mut supervised) = supervised {
            supervised.stdin.take();
            if tokio::time::timeout(STOP_GRACE, supervised.child.wait())
                .await
                .is_err()
            {
                tracing::warn!("on-device cleanup didn't stop; killing it");
                // The supervisor hasn't reaped the server, so its pid is
                // still the server's.
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", &supervised.server_pid.to_string()])
                    .status()
                    .await;
                // Kills (if still needed) and reaps the supervisor.
                let _ = supervised.child.kill().await;
            }
        }
    }

    /// Waits until `/health` says ok, the helper exits, or `limit` passes.
    async fn wait_ready(&self, limit: Duration) -> Result<(), LlamaError> {
        let ready = async {
            loop {
                if let Some(status) = self.exited() {
                    return Err(LlamaError::Start(format!("the helper exited ({status})")));
                }
                if health_ok(&self.client, &self.base_url).await {
                    return Ok(());
                }
                tokio::time::sleep(HEALTH_POLL).await;
            }
        };
        match tokio::time::timeout(limit, ready).await {
            Ok(result) => result,
            Err(_) => Err(LlamaError::StartTimeout),
        }
    }

    /// The supervisor's exit status, if it has exited (the server is then
    /// gone too).
    fn exited(&self) -> Option<String> {
        let mut supervisor = lock(&self.supervisor);
        let supervised = supervisor.as_mut()?;
        match supervised.child.try_wait() {
            Ok(Some(status)) => Some(status.to_string()),
            Ok(None) => None,
            Err(error) => Some(error.to_string()),
        }
    }

    /// Whether the helper is still running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        lock(&self.supervisor).is_some() && self.exited().is_none()
    }

    /// The server's process id, while it runs (for tests and the log).
    #[must_use]
    pub fn server_pid(&self) -> Option<u32> {
        lock(&self.supervisor).as_ref().map(|s| s.server_pid)
    }

    /// Stops the helper: closes the supervisor's pipe, which kills the
    /// server and frees the model's memory. Doesn't wait; safe to call
    /// more than once.
    pub fn stop(&self) {
        if let Some(mut supervised) = lock(&self.supervisor).take() {
            supervised.stdin.take();
            tracing::debug!(pid = supervised.server_pid, "on-device cleanup stopped");
        }
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Launches `helper` with `args` under the supervisor, with `key` in its
/// environment, and reads the server's process id.
async fn spawn_supervised(
    helper: &Path,
    args: &[String],
    key: &str,
) -> Result<Supervised, LlamaError> {
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(SUPERVISOR)
        .arg(helper)
        .args(args)
        .env("LLAMA_API_KEY", key)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| LlamaError::Start(e.to_string()))?;
    let stdin = child.stdin.take();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| LlamaError::Start("no pipe from the helper".to_owned()))?;
    let mut line = String::new();
    let read = tokio::time::timeout(
        Duration::from_secs(5),
        BufReader::new(stdout).read_line(&mut line),
    )
    .await;
    let server_pid = match read {
        Ok(Ok(_)) => line.trim().parse::<u32>().ok(),
        _ => None,
    };
    let Some(server_pid) = server_pid else {
        // Gone and reaped before the error: the closed pipe stops a server
        // that did start, and a stuck supervisor is killed.
        drop(stdin);
        if tokio::time::timeout(STOP_GRACE, child.wait())
            .await
            .is_err()
        {
            let _ = child.kill().await;
        }
        return Err(LlamaError::Start("the helper didn't start".to_owned()));
    };
    Ok(Supervised {
        child,
        stdin,
        server_pid,
    })
}

/// Whether `/health` at `base_url` says the model is loaded.
async fn health_ok(client: &Client, base_url: &str) -> bool {
    let Ok(response) = client
        .get(format!("{base_url}/health"))
        .timeout(Duration::from_secs(2))
        .send()
        .await
    else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    let Ok(bytes) = response.bytes().await else {
        return false;
    };
    serde_json::from_slice::<Value>(&bytes)
        .is_ok_and(|body| body.get("status").and_then(Value::as_str) == Some("ok"))
}

/// The chat-completions body PR #326 sent (`clean.py`).
#[must_use]
pub fn request_body(system: &str, user: &str, max_tokens: u32) -> Value {
    json!({
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "temperature": local_cleanup::TEMPERATURE,
        "top_k": local_cleanup::TOP_K,
        "seed": local_cleanup::SEED,
        "max_tokens": max_tokens,
        "chat_template_kwargs": {"enable_thinking": false},
        "cache_prompt": true,
        "stream": false,
    })
}

/// Sends one chat completion to the helper at `base_url` with `key`.
async fn complete_at(
    client: &Client,
    base_url: &str,
    key: &str,
    body: &Value,
) -> Result<String, LlamaError> {
    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .bearer_auth(key)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| LlamaError::Request(e.without_url().to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(LlamaError::Status(status.as_u16()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| LlamaError::Request(e.without_url().to_string()))?;
    // The cloud cleaner's reading: a finished reply only, without the tags.
    crate::cleanup::parse_reply(&bytes).ok_or(LlamaError::Reply)
}

impl ChatRunner for LlamaServer {
    async fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<String, LlamaError> {
        let body = request_body(system, user, max_tokens);
        complete_at(&self.client, &self.base_url, &self.key, &body).await
    }

    fn alive(&self) -> bool {
        self.is_running()
    }
}

#[cfg(test)]
mod tests;
