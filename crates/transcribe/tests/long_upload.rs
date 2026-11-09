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

/// What the stalling server shows the test.
struct Stalling {
    port: u16,
    /// Signalled once the server has read a little of the body and stopped.
    stalled: Arc<tokio::sync::Notify>,
    /// Body bytes the server has read so far.
    body_read: Arc<AtomicUsize>,
    /// The test's explicit release: the server reads the rest only after it.
    release: Arc<tokio::sync::Notify>,
}

/// A listener with a tiny receive buffer (so the client's upload is held back
/// by TCP, not just by the server not reading) that reads the head and about
/// 16 KB of the body, signals `stalled`, then waits for `release` (no timer)
/// before it reads the rest and answers 200.
async fn stalling_server() -> Stalling {
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.set_recv_buffer_size(8 * 1024).unwrap();
    socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let listener = socket.listen(8).unwrap();
    let server = Stalling {
        port: listener.local_addr().unwrap().port(),
        stalled: Arc::default(),
        body_read: Arc::default(),
        release: Arc::default(),
    };
    let (stalled, body_read, release) = (
        Arc::clone(&server.stalled),
        Arc::clone(&server.body_read),
        Arc::clone(&server.release),
    );
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut data = Vec::new();
        let mut chunk = vec![0u8; 4 * 1024];
        let mut head_end = None;
        let mut length = 0usize;
        let mut stalled_once = false;
        loop {
            if stalled_once {
                // Reading again only because the test released the server.
            } else if head_end.is_some() && body_read.load(Ordering::SeqCst) >= 16 * 1024 {
                stalled.notify_one();
                release.notified().await;
                stalled_once = true;
            }
            let Ok(n) = stream.read(&mut chunk).await else {
                return;
            };
            if n == 0 {
                return;
            }
            if head_end.is_none() {
                data.extend_from_slice(&chunk[..n]);
                if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    head_end = Some(at + 4);
                    let head = String::from_utf8_lossy(&data[..at]).to_ascii_lowercase();
                    length = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap();
                    body_read.store(data.len() - (at + 4), Ordering::SeqCst);
                    data.clear();
                }
            } else {
                body_read.fetch_add(n, Ordering::SeqCst);
            }
            if head_end.is_some() && body_read.load(Ordering::SeqCst) >= length {
                break;
            }
        }
        let body = br#"{"text":"a slow upload"}"#;
        let reply = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(reply.as_bytes()).await;
        let _ = stream.write_all(body).await;
        let _ = stream.shutdown().await;
    });
    server
}

/// Criterion 4 with the real numbers: a real 5-minute clip (9.6 MB of WAV)
/// whose upload is held back by the server for more than 32 s, past the old
/// fixed 30 s limit. Two clients run at once against two such servers:
///
/// - the control, with the old fixed 30 s limit, times out while its upload
///   is still incomplete;
/// - the engine as shipped (30 s + 0.5 s per second of audio = 180 s) is
///   still uploading at that point, then finishes when the test releases
///   the server and returns the transcript.
///
/// That the upload itself is what is pending is shown from the server: it
/// holds under a tenth of the body when the test checks. The server has a
/// tiny receive buffer and releases only on the test's signal, so a slow or
/// loaded runner cannot make it finish early. The only clocks are the real
/// limits: the control must time out at 30 s (the test waits for that,
/// however long it takes, up to a generous fence) and the shipped engine has
/// 150 s of margin after the 32 s check. Takes about 35 s.
#[tokio::test]
async fn a_five_minute_upload_held_back_for_32_seconds_succeeds_and_fails_under_the_old_limit() {
    let audio = Audio {
        sample_rate: 16_000,
        samples: vec![0.1; 16_000 * 300],
    };
    let wav_len = dictation_core::wav::encode_f32(&audio.samples, audio.sample_rate)
        .unwrap()
        .len();
    assert!(wav_len > 9_000_000, "a 5-minute clip is about 9.6 MB");

    let engine = |server: &Stalling, grows| {
        let config = Config {
            base_url: format!("http://127.0.0.1:{}/v1", server.port),
            timeout_grows_with_audio: grows,
            retry: RetryPolicy {
                max_attempts: 1,
                ..RetryPolicy::default()
            },
            ..Config::from_preset(dictation_transcribe::openai_compat::GROQ)
        };
        OpenAiCompat::new(config, Secret::new(KEY)).unwrap()
    };
    assert_eq!(
        Config::from_preset(dictation_transcribe::openai_compat::GROQ).timeout,
        Duration::from_secs(30),
        "the real base limit"
    );

    let fence = Duration::from_secs(150);
    let (fixed_server, grown_server) = (stalling_server().await, stalling_server().await);
    let (fixed, grown) = (engine(&fixed_server, false), engine(&grown_server, true));
    let (fixed_audio, grown_audio) = (audio.clone(), audio);
    let start = Instant::now();
    let fixed_run = tokio::spawn(async move { fixed.transcribe(&fixed_audio).await });
    let grown_run = tokio::spawn(async move { grown.transcribe(&grown_audio).await });

    // Both uploads are held back by their servers.
    tokio::time::timeout(fence, fixed_server.stalled.notified())
        .await
        .unwrap();
    tokio::time::timeout(fence, grown_server.stalled.notified())
        .await
        .unwrap();
    while start.elapsed() < Duration::from_secs(32) {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if grown_run.is_finished() {
            let ended = grown_run.await;
            panic!(
                "diagnosis: grown run ended with {ended:?} at {:?}, server held {} of {wav_len}",
                start.elapsed(),
                grown_server.body_read.load(Ordering::SeqCst)
            );
        }
    }

    // The control: past 30 s, timed out, with its upload far from complete.
    let result = tokio::time::timeout(fence, fixed_run)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, Err(TranscribeError::Timeout));
    assert!(start.elapsed() >= Duration::from_secs(30));
    assert!(fixed_server.body_read.load(Ordering::SeqCst) < wav_len / 10);

    // The shipped engine: still uploading, the server holding a sliver.
    if grown_run.is_finished() {
        let ended = grown_run.await;
        panic!(
            "still uploading past 30 s: ended with {ended:?} after {:?}, server held {} of {wav_len}",
            start.elapsed(),
            grown_server.body_read.load(Ordering::SeqCst)
        );
    }
    let held = grown_server.body_read.load(Ordering::SeqCst);
    assert!(
        held < wav_len / 10,
        "the server holds {held} of {wav_len} bytes"
    );

    // Released by the test: the rest goes up and the transcript comes back.
    grown_server.release.notify_one();
    fixed_server.release.notify_one();
    let result = tokio::time::timeout(fence, grown_run)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.unwrap(), "a slow upload");
    // The whole body: the WAV and the multipart framing around it.
    assert!(grown_server.body_read.load(Ordering::SeqCst) >= wav_len);
}
