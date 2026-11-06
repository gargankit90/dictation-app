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
