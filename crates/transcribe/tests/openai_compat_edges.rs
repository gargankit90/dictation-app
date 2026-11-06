//! M3.2 edge cases for `OpenAiCompat` (Tester, run 1): `Retry-After` at the
//! limit, on 503 and on every reply, empty replies, and a TLS smoke test
//! against the real Groq endpoint (ignored by default: it uses the network).
//! No real key anywhere: the keys here are obviously fake.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_core::retry::RetryPolicy;
use dictation_ports::{Audio, Secret, TranscribeError, Transcriber};
use dictation_transcribe::openai_compat::{Config, GROQ, MAX_RETRY_AFTER, OpenAiCompat, Wait};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "tester-fake-key-0000";
const ENDPOINT: &str = "/v1/audio/transcriptions";

#[derive(Clone, Default)]
struct RecordWait(Arc<Mutex<Vec<Duration>>>);

impl Wait for RecordWait {
    async fn wait(&self, duration: Duration) {
        self.0.lock().unwrap().push(duration);
    }
}

fn config(server: &MockServer, max_attempts: u32) -> Config {
    Config {
        base_url: format!("{}/v1", server.uri()),
        model: "m".into(),
        language: None,
        timeout: Duration::from_secs(5),
        timeout_grows_with_audio: false,
        long_clip: dictation_core::upload_limits::LONG_CLIP,
        retry: RetryPolicy {
            max_attempts,
            base_delay: Duration::from_millis(100),
            multiplier: 2,
            max_delay: Duration::from_secs(1),
            jitter: 0.0,
        },
    }
}

fn engine(config: Config) -> (OpenAiCompat<RecordWait>, RecordWait) {
    let wait = RecordWait::default();
    let engine = OpenAiCompat::with_wait(config, Secret::new(KEY), wait.clone(), 1).unwrap();
    (engine, wait)
}

fn audio() -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.0; 1_600],
    }
}

async fn mount(server: &MockServer, response: ResponseTemplate, times: Option<u64>) {
    let mock = Mock::given(method("POST"))
        .and(path(ENDPOINT))
        .respond_with(response);
    match times {
        Some(n) => mock.up_to_n_times(n).mount(server).await,
        None => mock.mount(server).await,
    }
}

async fn requests(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

fn ok() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": "ok"}))
}

#[tokio::test]
async fn retry_after_exactly_at_the_limit_is_honoured() {
    let server = MockServer::start().await;
    let limit = MAX_RETRY_AFTER.as_secs().to_string();
    mount(
        &server,
        ResponseTemplate::new(429).insert_header("retry-after", limit.as_str()),
        Some(1),
    )
    .await;
    mount(&server, ok(), None).await;
    let (engine, wait) = engine(config(&server, 3));
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), "ok");
    assert_eq!(*wait.0.lock().unwrap(), [MAX_RETRY_AFTER]);
}

#[tokio::test]
async fn retry_after_on_a_503_is_honoured() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(503).insert_header("retry-after", "4"),
        Some(1),
    )
    .await;
    mount(&server, ok(), None).await;
    let (engine, wait) = engine(config(&server, 3));
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), "ok");
    assert_eq!(*wait.0.lock().unwrap(), [Duration::from_secs(4)]);
}

/// A provider that keeps sending `Retry-After` still gets only
/// `max_attempts` requests.
#[tokio::test]
async fn retry_after_every_time_still_stops_at_max_attempts() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(429).insert_header("retry-after", "0"),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server, 4));
    assert_eq!(
        engine.transcribe(&audio()).await,
        Err(TranscribeError::RateLimited)
    );
    assert_eq!(requests(&server).await, 4);
    assert_eq!(*wait.0.lock().unwrap(), [Duration::ZERO; 3]);
}

#[tokio::test]
async fn a_401_with_retry_after_is_still_not_retried() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(401).insert_header("retry-after", "1"),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server, 3));
    assert_eq!(
        engine.transcribe(&audio()).await,
        Err(TranscribeError::InvalidKey)
    );
    assert_eq!(requests(&server).await, 1);
    assert!(wait.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn empty_text_and_empty_segments_are_fine() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": "  ", "segments": []})),
        None,
    )
    .await;
    let (engine, _) = engine(config(&server, 1));
    let t = engine.transcribe_verbose(&audio()).await.unwrap();
    assert_eq!(t.text, "");
    assert!(t.segments.is_empty());
}

#[tokio::test]
async fn empty_audio_is_still_sent() {
    let server = MockServer::start().await;
    mount(&server, ok(), None).await;
    let (engine, _) = engine(config(&server, 1));
    let empty = Audio {
        sample_rate: 16_000,
        samples: Vec::new(),
    };
    assert_eq!(engine.transcribe(&empty).await.unwrap(), "ok");
    assert_eq!(requests(&server).await, 1);
}

/// A non-ASCII key is a valid header byte-wise (obs-text), so it is sent;
/// the provider's 401 maps to `InvalidKey` and nothing echoes the key.
#[tokio::test]
async fn a_non_ascii_key_rejected_by_the_provider_is_not_echoed() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(401), None).await;
    let engine = OpenAiCompat::with_wait(
        config(&server, 3),
        Secret::new("ключ-fake"),
        RecordWait::default(),
        1,
    )
    .unwrap();
    let error = engine.transcribe(&audio()).await.unwrap_err();
    assert_eq!(error, TranscribeError::InvalidKey);
    assert!(!format!("{error} {error:?} {engine:?}").contains("ключ"));
    assert_eq!(requests(&server).await, 1);
}

/// TLS smoke test: the real Groq endpoint with a dummy key. Proves the
/// rustls + ring + OS trust store setup completes a handshake and that the
/// real 401 maps to `InvalidKey`. Uses the network, so ignored by default:
/// `cargo nextest run -p dictation-transcribe --run-ignored only groq_tls`.
#[tokio::test]
#[ignore = "network: calls https://api.groq.com with a dummy key"]
async fn groq_tls_handshake_and_dummy_key_is_invalid_key() {
    let mut config = Config::from_preset(GROQ);
    config.retry.max_attempts = 1;
    let engine = OpenAiCompat::new(config, Secret::new("invalid-test-key")).unwrap();
    let audio = Audio {
        sample_rate: 16_000,
        samples: vec![0.0; 1_600],
    };
    assert_eq!(
        engine.transcribe(&audio).await,
        Err(TranscribeError::InvalidKey)
    );
}

// Run 2: `Retry-After` as an HTTP date, end to end (124871d).

fn date_in(seconds: u64) -> String {
    httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(seconds))
}

#[tokio::test]
async fn retry_after_http_date_is_honoured() {
    let server = MockServer::start().await;
    let date = date_in(10);
    mount(
        &server,
        ResponseTemplate::new(429).insert_header("retry-after", date.as_str()),
        Some(1),
    )
    .await;
    mount(&server, ok(), None).await;
    let (engine, wait) = engine(config(&server, 3));
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), "ok");
    let waits = wait.0.lock().unwrap().clone();
    // Whole-second dates: between 8 and 10 s from now, never the backoff.
    assert_eq!(waits.len(), 1);
    assert!(
        waits[0] >= Duration::from_secs(8) && waits[0] <= Duration::from_secs(10),
        "{waits:?}"
    );
}

#[tokio::test]
async fn a_past_http_date_retries_at_once() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(503).insert_header("retry-after", "Wed, 21 Oct 2015 07:28:00 GMT"),
        Some(1),
    )
    .await;
    mount(&server, ok(), None).await;
    let (engine, wait) = engine(config(&server, 3));
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), "ok");
    assert_eq!(*wait.0.lock().unwrap(), [Duration::ZERO]);
}

#[tokio::test]
async fn an_http_date_beyond_the_cap_gives_up_at_once() {
    let server = MockServer::start().await;
    let date = date_in(MAX_RETRY_AFTER.as_secs() + 60);
    mount(
        &server,
        ResponseTemplate::new(429).insert_header("retry-after", date.as_str()),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server, 3));
    assert_eq!(
        engine.transcribe(&audio()).await,
        Err(TranscribeError::RateLimited)
    );
    assert_eq!(requests(&server).await, 1);
    assert!(wait.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn seconds_beyond_the_cap_give_up_and_garbage_uses_the_backoff() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(429).insert_header("retry-after", "31"),
        None,
    )
    .await;
    let (engine_a, wait_a) = engine(config(&server, 3));
    assert_eq!(
        engine_a.transcribe(&audio()).await,
        Err(TranscribeError::RateLimited)
    );
    assert!(wait_a.0.lock().unwrap().is_empty());

    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(429).insert_header("retry-after", "in a bit"),
        Some(2),
    )
    .await;
    mount(&server, ok(), None).await;
    let (engine_b, wait_b) = engine(config(&server, 3));
    assert_eq!(engine_b.transcribe(&audio()).await.unwrap(), "ok");
    assert_eq!(
        *wait_b.0.lock().unwrap(),
        [Duration::from_millis(100), Duration::from_millis(200)]
    );
}
