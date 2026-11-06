//! H1: a long dictation on a slow connection. The request's time limit grows
//! with the clip, a server that never answers still fails with the timeout
//! error, and a long clip that times out is retried once, not twice.
//!
//! Against local servers only: no Internet, no real key. The slow link is
//! stood in for by a server that waits before it answers, which the client
//! sees exactly as a slow upload.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use dictation_core::retry::RetryPolicy;
use dictation_core::upload_limits::LONG_CLIP;
use dictation_ports::{Audio, Secret, TranscribeError, Transcriber};
use dictation_transcribe::openai_compat::{Config, OpenAiCompat};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "test-key-NOT-REAL-long-upload-5d2c";
const BASE: Duration = Duration::from_millis(400);

fn clip(seconds: usize) -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.0; 16_000 * seconds],
    }
}

fn config(base_url: String, grows: bool, attempts: u32, long_clip: Duration) -> Config {
    Config {
        base_url,
        model: "whisper-large-v3-turbo".into(),
        language: None,
        timeout: BASE,
        timeout_grows_with_audio: grows,
        long_clip,
        retry: RetryPolicy {
            max_attempts: attempts,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(1),
            jitter: 0.0,
            ..RetryPolicy::default()
        },
    }
}

async fn slow_server(delay: Duration) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"text": "a long dictation"}))
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    server
}

/// A 4 s clip gets 400 ms + 2 s. The server needs 1 s: past the base limit,
/// inside the grown one.
#[tokio::test]
async fn a_clip_that_needs_longer_than_the_base_limit_now_succeeds() {
    let server = slow_server(Duration::from_secs(1)).await;
    let url = format!("{}/v1", server.uri());

    // Control: the old fixed limit fails this request, all attempts.
    let fixed =
        OpenAiCompat::new(config(url.clone(), false, 2, LONG_CLIP), Secret::new(KEY)).unwrap();
    assert_eq!(
        fixed.transcribe(&clip(4)).await,
        Err(TranscribeError::Timeout)
    );

    let grown = OpenAiCompat::new(config(url, true, 2, LONG_CLIP), Secret::new(KEY)).unwrap();
    assert_eq!(
        grown.transcribe(&clip(4)).await.unwrap(),
        "a long dictation"
    );
}

/// A server that never answers fails with the timeout error, at the grown
/// limit (400 ms + 2 s) and not much later.
#[tokio::test]
async fn a_server_that_never_answers_times_out_within_the_grown_limit() {
    let server = slow_server(Duration::from_secs(120)).await;
    let engine = OpenAiCompat::new(
        config(format!("{}/v1", server.uri()), true, 1, LONG_CLIP),
        Secret::new(KEY),
    )
    .unwrap();
    let start = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(20), engine.transcribe(&clip(4))).await;
    assert_eq!(result.unwrap(), Err(TranscribeError::Timeout));
    let took = start.elapsed();
    assert!(
        took >= Duration::from_millis(2_400),
        "grown limit: {took:?}"
    );
    assert!(
        took < Duration::from_secs(10),
        "and not much later: {took:?}"
    );
}

/// A listener that reads each request, counts it and never answers.
async fn silent_server() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&requests);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let seen = Arc::clone(&seen);
            tokio::spawn(async move {
                // The request line is enough to count it; keep reading so
                // the client's upload completes, then hold the connection.
                let mut buf = vec![0u8; 64 * 1024];
                let mut counted = false;
                while matches!(stream.read(&mut buf).await, Ok(n) if n > 0) {
                    if !counted {
                        counted = true;
                        seen.fetch_add(1, Ordering::SeqCst);
                    }
                }
                let _ = stream.shutdown().await;
            });
        }
    });
    (port, requests)
}

/// A clip at or past `long_clip` that times out is tried twice, not three
/// times; a shorter one keeps all three attempts. The threshold is set to
/// 3 s here so the test waits out two or three real 400 ms + 0.5 s/s limits
/// instead of a real 30 s limit.
#[tokio::test]
async fn a_long_clip_is_retried_once_after_a_timeout_and_a_short_one_keeps_its_attempts() {
    let long_clip = Duration::from_secs(3);
    for (seconds, expected) in [(4, 2), (2, 3)] {
        let (port, requests) = silent_server().await;
        let engine = OpenAiCompat::new(
            config(format!("http://127.0.0.1:{port}/v1"), true, 3, long_clip),
            Secret::new(KEY),
        )
        .unwrap();
        let result =
            tokio::time::timeout(Duration::from_secs(30), engine.transcribe(&clip(seconds))).await;
        assert_eq!(
            result.unwrap(),
            Err(TranscribeError::Timeout),
            "{seconds} s clip"
        );
        assert_eq!(
            requests.load(Ordering::SeqCst),
            expected,
            "{seconds} s clip"
        );
    }
}

/// Other failures keep the old policy at any clip length: 5xx and 429 (with
/// a `Retry-After`) use all attempts for a short and a long clip alike, and a
/// refused connection fails fast, with all attempts.
#[tokio::test]
async fn other_failures_keep_every_attempt_at_any_clip_length() {
    for seconds in [1, 59, 60, 61, 300] {
        for status in [429u16, 500, 503] {
            let server = MockServer::start().await;
            let mut response = ResponseTemplate::new(status);
            if status == 429 {
                response = response.insert_header("retry-after", "0");
            }
            Mock::given(method("POST"))
                .respond_with(response)
                .mount(&server)
                .await;
            let engine = OpenAiCompat::new(
                config(format!("{}/v1", server.uri()), true, 3, LONG_CLIP),
                Secret::new(KEY),
            )
            .unwrap();
            let result = engine.transcribe(&clip(seconds)).await;
            assert!(result.is_err(), "{status} at {seconds} s");
            assert_ne!(result, Err(TranscribeError::Timeout));
            assert_eq!(
                server.received_requests().await.unwrap().len(),
                3,
                "{status} at {seconds} s"
            );
            let text = format!("{result:?}");
            assert!(!text.contains(KEY));
        }
    }
    let engine = OpenAiCompat::new(
        config("http://127.0.0.1:9/v1".into(), true, 3, LONG_CLIP),
        Secret::new(KEY),
    )
    .unwrap();
    let start = Instant::now();
    let result = engine.transcribe(&clip(300)).await;
    assert!(
        matches!(result, Err(TranscribeError::Network(_))),
        "{result:?}"
    );
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(!format!("{result:?}").contains(KEY));
}

/// Needs a route that swallows packets (no reply): run by hand with
/// `--ignored`. A 300 s clip must give up after the 10 s connect limit
/// (twice), not after its 180 s request limit.
#[tokio::test]
#[ignore = "needs an unroutable address and about 20 s"]
async fn a_dead_network_fails_fast_for_a_five_minute_clip() {
    let engine = OpenAiCompat::new(
        config("http://10.255.255.1/v1".into(), true, 3, LONG_CLIP),
        Secret::new(KEY),
    )
    .unwrap();
    let start = Instant::now();
    let result = engine.transcribe(&clip(300)).await;
    let took = start.elapsed();
    eprintln!("{result:?} after {took:?}");
    assert!(result.is_err());
    assert!(took < Duration::from_secs(35), "{took:?}");
}

/// A listener that takes a request slowly: it reads the head and the first
/// 64 KB of the body, then stalls for `stall` (reading nothing, so the
/// client's upload is held back by socket buffers), then reads the rest and
/// answers 200. `stalled` is signalled when it starts to stall, `answered`
/// when the whole upload has been read.
async fn backpressured_server(
    stall: Duration,
) -> (u16, Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let stalled = Arc::new(tokio::sync::Notify::new());
    let answered = Arc::new(tokio::sync::Notify::new());
    let (s, a) = (Arc::clone(&stalled), Arc::clone(&answered));
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let (s, a) = (Arc::clone(&s), Arc::clone(&a));
            tokio::spawn(async move {
                let mut data = Vec::new();
                let mut chunk = vec![0u8; 16 * 1024];
                let head_end = loop {
                    let Ok(n) = stream.read(&mut chunk).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&chunk[..n]);
                    if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        break at + 4;
                    }
                };
                let head = String::from_utf8_lossy(&data[..head_end]).to_ascii_lowercase();
                let length: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap();
                let total = head_end + length;
                while data.len() < head_end + 64 * 1024 {
                    let Ok(n) = stream.read(&mut chunk).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&chunk[..n]);
                }
                s.notify_one();
                tokio::time::sleep(stall).await;
                while data.len() < total {
                    let Ok(n) = stream.read(&mut chunk).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&chunk[..n]);
                }
                a.notify_one();
                let body = br#"{"text":"a slow upload"}"#;
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
                let _ = stream.write_all(body).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (port, stalled, answered)
}

/// The upload itself is slow: a 20 s clip (1.3 MB, more than the socket
/// buffers hold) whose upload the server holds back for 1 s, more than three
/// times the base limit of 300 ms. With the fixed limit the request times
/// out while the upload is still incomplete; with the grown limit (300 ms +
/// 10 s) the same upload finishes and the dictation succeeds. The real
/// production numbers (30 s base) are scaled down so the test takes about a
/// second; what is proved is that no limit other than the per-request one
/// ends a slow upload. Not timing-sensitive: the control fails because
/// 300 ms passes long before the server's 1 s stall ends, the success has a
/// 9 s margin, and each step is fenced on the server's own progress.
#[tokio::test]
async fn an_upload_held_back_past_the_base_limit_succeeds_only_with_the_grown_limit() {
    const STALL: Duration = Duration::from_secs(1);
    let audio = Audio {
        sample_rate: 32_000,
        samples: vec![0.1; 32_000 * 20],
    };
    let fence = Duration::from_secs(30);
    let base = |url: String, grows| Config {
        timeout: Duration::from_millis(300),
        ..config(url, grows, 1, LONG_CLIP)
    };

    // Control: the old fixed limit. The request ends while the server is
    // still stalled, before it has read the upload.
    let (port, stalled, answered) = backpressured_server(STALL).await;
    let url = format!("http://127.0.0.1:{port}/v1");
    let fixed = OpenAiCompat::new(base(url, false), Secret::new(KEY)).unwrap();
    let result = tokio::time::timeout(fence, fixed.transcribe(&audio)).await;
    assert_eq!(result.unwrap(), Err(TranscribeError::Timeout));
    tokio::time::timeout(fence, stalled.notified())
        .await
        .unwrap();
    let early = tokio::time::timeout(Duration::from_millis(200), answered.notified()).await;
    assert!(early.is_err(), "the fixed limit ended the upload midway");

    // The grown limit: the same stalled upload completes.
    let (port, stalled, _answered) = backpressured_server(STALL).await;
    let url = format!("http://127.0.0.1:{port}/v1");
    let grown = OpenAiCompat::new(base(url, true), Secret::new(KEY)).unwrap();
    let start = Instant::now();
    let run = tokio::spawn(async move { grown.transcribe(&audio).await });
    tokio::time::timeout(fence, stalled.notified())
        .await
        .unwrap();
    assert!(
        !run.is_finished(),
        "still uploading while the server stalls"
    );
    let result = tokio::time::timeout(fence, run).await.unwrap().unwrap();
    assert_eq!(result.unwrap(), "a slow upload");
    assert!(
        start.elapsed() >= STALL,
        "the upload outlasted the base limit"
    );
}
