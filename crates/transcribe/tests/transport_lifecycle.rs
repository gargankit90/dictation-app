//! T1 (P2-02): what happens to a request when the app stops, the provider
//! stalls or the reply is cut off, against a raw local listener (no Internet).
//!
//! No sleep stands in for "it happened": the test waits on a fence (the
//! listener has read the request, the client has gone, the engine has asked
//! for its backoff) and every wait has an upper bound, so a broken engine
//! fails the test instead of hanging it. Deadlines are short real ones (a few
//! hundred milliseconds); paused time is used only for the pure sleeper,
//! because the clock would jump ahead of a real socket.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use dictation_core::retry::RetryPolicy;
use dictation_ports::{Audio, Secret, TranscribeError};
use dictation_transcribe::check_key::check_key_within;
use dictation_transcribe::openai_compat::{Config, GROQ, OpenAiCompat, TokioWait, Wait};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

const KEY: &str = "test-key-NOT-REAL-lifecycle-9f8e7d";

/// How the listener answers once it has read a whole request.
#[derive(Clone, Copy)]
enum Reply {
    /// Never answers and keeps the connection open.
    Silence,
    /// HTTP 503, then closes.
    ServerError,
    /// Promises 1,000 body bytes, sends 10, then stalls with the
    /// connection open.
    StalledBody,
    /// Promises 1,000 body bytes, sends 10, then closes.
    CutOffBody,
}

/// A listener on 127.0.0.1 that counts what it sees.
struct Server {
    port: u16,
    /// Requests read in full.
    requests: Arc<AtomicUsize>,
    /// Signalled each time a request has been read in full.
    request_read: Arc<Notify>,
    /// Signalled when a client closes a connection we were still holding.
    client_gone: Arc<Notify>,
}

impl Server {
    async fn start(reply: Reply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(AtomicUsize::new(0));
        let request_read = Arc::new(Notify::new());
        let client_gone = Arc::new(Notify::new());
        let server = Self {
            port,
            requests: Arc::clone(&requests),
            request_read: Arc::clone(&request_read),
            client_gone: Arc::clone(&client_gone),
        };
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let (requests, request_read, client_gone) = (
                    Arc::clone(&requests),
                    Arc::clone(&request_read),
                    Arc::clone(&client_gone),
                );
                tokio::spawn(async move {
                    serve(stream, reply, &requests, &request_read, &client_gone).await;
                });
            }
        });
        server
    }

    fn config(&self) -> Config {
        Config {
            base_url: format!("http://127.0.0.1:{}/openai/v1", self.port),
            ..Config::from_preset(GROQ)
        }
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

async fn serve(
    mut stream: TcpStream,
    reply: Reply,
    requests: &AtomicUsize,
    request_read: &Notify,
    client_gone: &Notify,
) {
    if read_request(&mut stream).await.is_none() {
        return;
    }
    requests.fetch_add(1, Ordering::SeqCst);
    request_read.notify_one();
    match reply {
        Reply::Silence => {}
        Reply::ServerError => {
            let _ = stream
                .write_all(b"HTTP/1.1 503 Service Unavailable\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
            return;
        }
        Reply::StalledBody | Reply::CutOffBody => {
            let head = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 1000\r\n\r\n{\"text\": \"";
            let _ = stream.write_all(head).await;
            if matches!(reply, Reply::CutOffBody) {
                return;
            }
        }
    }
    // Hold the connection until the client closes it.
    let mut buf = [0u8; 64];
    while matches!(stream.read(&mut buf).await, Ok(n) if n > 0) {}
    client_gone.notify_one();
}

/// Reads one request: the head, then `content-length` bytes of body.
async fn read_request(stream: &mut TcpStream) -> Option<()> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = stream.read(&mut chunk).await.ok().filter(|n| *n > 0)?;
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
        let n = stream.read(&mut chunk).await.ok().filter(|n| *n > 0)?;
        data.extend_from_slice(&chunk[..n]);
    }
    Some(())
}

fn audio() -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.0; 1_600],
    }
}

/// Retries allowed, so a missing stop shows up as a second request.
fn retries(max_attempts: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        ..RetryPolicy::default()
    }
}

/// The longest any fence may take before the test calls it a hang.
const FENCE: Duration = Duration::from_secs(20);

/// Waits for `notify`, or fails the test.
async fn fenced(notify: &Notify, what: &str) {
    tokio::time::timeout(FENCE, notify.notified())
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

/// A deadline short enough to wait out and long enough that the loopback
/// exchange finishes first on a slow machine.
const SHORT: Duration = Duration::from_millis(400);

/// A fast retry policy with `max_attempts`, so retries show up at once.
fn quick_retries(max_attempts: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        base_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(1),
        jitter: 0.0,
        ..RetryPolicy::default()
    }
}

/// A [`Wait`] that says it was asked and then never finishes.
#[derive(Clone, Default)]
struct HeldWait {
    asked: Arc<Notify>,
    count: Arc<AtomicUsize>,
}

impl Wait for HeldWait {
    fn wait(&self, _duration: Duration) -> impl Future<Output = ()> + Send {
        self.count.fetch_add(1, Ordering::SeqCst);
        self.asked.notify_one();
        std::future::pending()
    }
}

#[tokio::test]
async fn shutdown_aborts_http_request_without_retry() {
    let server = Server::start(Reply::Silence).await;
    let config = Config {
        retry: retries(5),
        ..server.config()
    };
    let wait = HeldWait::default();
    let engine =
        Arc::new(OpenAiCompat::with_wait(config, Secret::new(KEY), wait.clone(), 1).unwrap());
    let running = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move { engine.transcribe_verbose(&audio()).await })
    };
    fenced(&server.request_read, "the request").await;
    // Shutdown drops the request.
    running.abort();
    assert!(running.await.unwrap_err().is_cancelled());
    // The connection closes: the provider sees the client leave.
    fenced(&server.client_gone, "the connection to close").await;
    // The engine is gone with the request: it asked for no backoff and sent
    // nothing more.
    assert_eq!(wait.count.load(Ordering::SeqCst), 0);
    assert_eq!(server.requests(), 1);
}

#[tokio::test]
async fn abort_during_backoff_sends_no_next_request() {
    let server = Server::start(Reply::ServerError).await;
    let config = Config {
        retry: retries(5),
        ..server.config()
    };
    let wait = HeldWait::default();
    let engine =
        Arc::new(OpenAiCompat::with_wait(config, Secret::new(KEY), wait.clone(), 1).unwrap());
    let running = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move { engine.transcribe_verbose(&audio()).await })
    };
    // The 503 came back and the engine is now backing off.
    fenced(&wait.asked, "the backoff").await;
    assert_eq!(server.requests(), 1);
    running.abort();
    assert!(running.await.unwrap_err().is_cancelled());
    assert_eq!(server.requests(), 1, "no next request after the abort");
    assert_eq!(wait.count.load(Ordering::SeqCst), 1);
}

/// Without the abort the same engine does retry, so the two tests above
/// prove the abort and not an engine that never retries.
#[tokio::test]
async fn without_an_abort_the_engine_retries_after_the_backoff() {
    let server = Server::start(Reply::ServerError).await;
    let config = Config {
        retry: quick_retries(3),
        ..server.config()
    };
    let engine = OpenAiCompat::new(config, Secret::new(KEY)).unwrap();
    let result = tokio::time::timeout(FENCE, engine.transcribe_verbose(&audio())).await;
    assert!(
        matches!(result, Ok(Err(TranscribeError::Engine(_)))),
        "{result:?}"
    );
    assert_eq!(server.requests(), 3);
}

#[tokio::test]
async fn no_headers_times_out() {
    let server = Server::start(Reply::Silence).await;
    let config = Config {
        timeout: SHORT,
        retry: retries(1),
        ..server.config()
    };
    let engine = OpenAiCompat::new(config, Secret::new(KEY)).unwrap();
    let start = Instant::now();
    let result = tokio::time::timeout(FENCE, engine.transcribe_verbose(&audio())).await;
    assert_eq!(result.unwrap().unwrap_err(), TranscribeError::Timeout);
    assert!(start.elapsed() >= SHORT, "not before the deadline");
    fenced(&server.request_read, "the request").await;
    assert_eq!(server.requests(), 1);
}

#[tokio::test]
async fn partial_body_times_out() {
    let server = Server::start(Reply::StalledBody).await;
    let config = Config {
        timeout: SHORT,
        retry: retries(1),
        ..server.config()
    };
    let engine = OpenAiCompat::new(config, Secret::new(KEY)).unwrap();
    let start = Instant::now();
    let result = tokio::time::timeout(FENCE, engine.transcribe_verbose(&audio())).await;
    // The deadline covers the body, not just the headers.
    assert_eq!(result.unwrap().unwrap_err(), TranscribeError::Timeout);
    assert!(start.elapsed() >= SHORT);
    assert_eq!(server.requests(), 1);
}

#[tokio::test]
async fn truncated_body_has_safe_error() {
    let cut_off = TranscribeError::Network("the reply was cut off".into());
    let server = Server::start(Reply::CutOffBody).await;
    let config = Config {
        retry: retries(1),
        ..server.config()
    };
    let engine = OpenAiCompat::new(config, Secret::new(KEY)).unwrap();
    let result = tokio::time::timeout(FENCE, engine.transcribe_verbose(&audio())).await;
    // Fixed text: no key, no half-read transcript, no URL.
    assert_eq!(result.unwrap().unwrap_err(), cut_off);
    assert_eq!(server.requests(), 1);

    // A cut-off reply is retried like any network error, within the limit.
    let server = Server::start(Reply::CutOffBody).await;
    let config = Config {
        retry: quick_retries(3),
        ..server.config()
    };
    let engine = OpenAiCompat::new(config, Secret::new(KEY)).unwrap();
    let result = tokio::time::timeout(FENCE, engine.transcribe_verbose(&audio())).await;
    assert_eq!(result.unwrap().unwrap_err(), cut_off);
    assert_eq!(server.requests(), 3, "retried up to the limit, no more");
}

#[tokio::test(start_paused = true)]
async fn tokio_wait_honors_delay() {
    let done = Arc::new(AtomicUsize::new(0));
    let waiting = {
        let done = Arc::clone(&done);
        tokio::spawn(async move {
            TokioWait.wait(Duration::from_secs(10)).await;
            done.fetch_add(1, Ordering::SeqCst);
        })
    };
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert_eq!(done.load(Ordering::SeqCst), 0, "not at once");
    tokio::time::advance(Duration::from_millis(9_999)).await;
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert_eq!(done.load(Ordering::SeqCst), 0, "not one millisecond early");
    tokio::time::advance(Duration::from_millis(1)).await;
    waiting.await.unwrap();
    assert_eq!(done.load(Ordering::SeqCst), 1, "at the deadline");
}

/// The check has its own deadline, whatever the input config says, and
/// never retries (P1-05).
#[tokio::test]
async fn check_key_uses_its_own_deadline_and_never_retries() {
    let server = Server::start(Reply::Silence).await;
    let config = Config {
        timeout: Duration::from_secs(600),
        retry: quick_retries(5),
        ..server.config()
    };
    let start = Instant::now();
    let result =
        tokio::time::timeout(FENCE, check_key_within(config, Secret::new(KEY), SHORT)).await;
    assert_eq!(result.unwrap(), Err(TranscribeError::Timeout));
    assert!(start.elapsed() >= SHORT, "at the check's budget");
    fenced(&server.request_read, "the request").await;
    assert_eq!(server.requests(), 1, "one request, no retry");
}

/// A stalled check is reported and leaves nothing behind: the next one runs
/// at once, on a fresh connection.
#[tokio::test]
async fn check_key_can_be_retried_after_a_timeout() {
    let server = Server::start(Reply::Silence).await;
    for attempt in 1..=2 {
        let result = tokio::time::timeout(
            FENCE,
            check_key_within(server.config(), Secret::new(KEY), SHORT),
        )
        .await;
        assert_eq!(
            result.unwrap(),
            Err(TranscribeError::Timeout),
            "check {attempt}"
        );
        fenced(&server.request_read, "the request").await;
        assert_eq!(server.requests(), attempt);
    }
}
