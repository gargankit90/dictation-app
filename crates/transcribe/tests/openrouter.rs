//! M3.2: `OpenRouter` against a wiremock server. No real provider, no
//! real key, and no real sleeping between retries (`RecordWait`).

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

mod common;

use common::Captured;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_core::retry::RetryPolicy;
use dictation_ports::{Audio, Secret, TranscribeError, Transcriber};
use dictation_transcribe::openrouter::{Config, MAX_RETRY_AFTER, OpenRouter, Wait};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Obviously fake; the leak test looks for it everywhere.
const KEY: &str = "test-key-NOT-REAL-7f3a9c";
const TRANSCRIPT: &str = "The quick brown fox jumps over the lazy dog.";

fn reply() -> serde_json::Value {
    serde_json::json!({ "text": format!(" {TRANSCRIPT}") })
}

/// Records each wait instead of sleeping.
#[derive(Clone, Default)]
struct RecordWait(Arc<Mutex<Vec<Duration>>>);

impl RecordWait {
    fn waits(&self) -> Vec<Duration> {
        self.0.lock().unwrap().clone()
    }
}

impl Wait for RecordWait {
    async fn wait(&self, duration: Duration) {
        self.0.lock().unwrap().push(duration);
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn config(server: &MockServer) -> Config {
    Config {
        base_url: format!("{}/api/v1/", server.uri()),
        model: "openai/gpt-4o-mini-transcribe".into(),
        privacy: dictation_core::cloud_models::PrivacyFilter::DenyDataCollection,
        timeout: Duration::from_secs(5),
        timeout_grows_with_audio: false,
        long_clip: Duration::MAX,
        retry: RetryPolicy {
            max_attempts: 3,
            base_delay: ms(100),
            multiplier: 2,
            max_delay: ms(1_000),
            jitter: 0.0,
        },
    }
}

fn engine(config: Config) -> (OpenRouter<RecordWait>, RecordWait) {
    let wait = RecordWait::default();
    let engine = OpenRouter::with_wait(config, Secret::new(KEY), wait.clone(), 7).unwrap();
    (engine, wait)
}

fn audio() -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.25; 1_600],
    }
}

const ENDPOINT: &str = "/api/v1/audio/transcriptions";

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

// Criterion 1 (Q2: a JSON body with the WAV in base64).
#[tokio::test]
async fn request_carries_audio_model_and_filter_and_no_language() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ENDPOINT))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply()))
        .mount(&server)
        .await;
    let (engine, _) = engine(config(&server));
    engine.transcribe(&audio()).await.unwrap();

    let request = &server.received_requests().await.unwrap()[0];
    let content_type = request
        .headers
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(content_type, "application/json");
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["model"], "openai/gpt-4o-mini-transcribe");
    assert!(body.get("language").is_none(), "{body}");
    assert_eq!(body["input_audio"]["format"], "wav");
    assert_eq!(
        body["provider"],
        serde_json::json!({"data_collection": "deny"})
    );
    // 1600 samples of 16-bit PCM plus the 44-byte header: 3,244 bytes, so
    // 4 * ceil(3244 / 3) base64 characters, starting with "RIFF".
    let data = body["input_audio"]["data"].as_str().unwrap();
    assert_eq!(data.len(), 4 * 3_244_usize.div_ceil(3));
    assert!(data.starts_with("UklGR"), "RIFF");
}

// Criterion 2.
#[tokio::test]
async fn success_parses_the_text() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_json(reply()),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server));

    assert_eq!(engine.transcribe(&audio()).await.unwrap(), TRANSCRIPT);
    assert!(wait.waits().is_empty());
}

// Criterion 3.
#[tokio::test]
async fn unauthorized_is_invalid_key_without_retrying() {
    let server = MockServer::start().await;
    // A 401 body may echo part of the key.
    let body =
        serde_json::json!({"error": {"message": format!("Incorrect API key provided: {KEY}")}});
    mount(
        &server,
        ResponseTemplate::new(401).set_body_json(body),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server));
    assert_eq!(
        engine.transcribe(&audio()).await,
        Err(TranscribeError::InvalidKey)
    );
    assert_eq!(requests(&server).await, 1);
    assert!(wait.waits().is_empty());
}

#[tokio::test]
async fn rate_limited_then_retried_succeeds_after_the_backoff() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(429), Some(2)).await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_json(reply()),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server));
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), TRANSCRIPT);
    assert_eq!(requests(&server).await, 3);
    assert_eq!(wait.waits(), [ms(100), ms(200)]);
}

#[tokio::test]
async fn retry_after_replaces_the_backoff() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(429).insert_header("retry-after", "2"),
        Some(1),
    )
    .await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_json(reply()),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server));
    engine.transcribe(&audio()).await.unwrap();
    assert_eq!(wait.waits(), [Duration::from_secs(2)]);
}

#[tokio::test]
async fn retry_after_longer_than_the_limit_gives_up_at_once() {
    let server = MockServer::start().await;
    let too_long = (MAX_RETRY_AFTER.as_secs() + 1).to_string();
    mount(
        &server,
        ResponseTemplate::new(429).insert_header("retry-after", too_long.as_str()),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server));
    assert_eq!(
        engine.transcribe(&audio()).await,
        Err(TranscribeError::RateLimited)
    );
    assert_eq!(requests(&server).await, 1);
    assert!(wait.waits().is_empty());
}

#[tokio::test]
async fn rate_limited_every_time_fails_after_max_attempts() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(429), None).await;
    let (engine, wait) = engine(config(&server));
    assert_eq!(
        engine.transcribe(&audio()).await,
        Err(TranscribeError::RateLimited)
    );
    assert_eq!(requests(&server).await, 3);
    assert_eq!(wait.waits(), [ms(100), ms(200)]);
}

#[tokio::test]
async fn server_error_is_retried_then_succeeds() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(503), Some(1)).await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_json(reply()),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server));
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), TRANSCRIPT);
    assert_eq!(wait.waits(), [ms(100)]);
}

#[tokio::test]
async fn server_error_every_time_fails_after_retries() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(500).set_body_string(TRANSCRIPT),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server));
    let error = engine.transcribe(&audio()).await.unwrap_err();
    assert_eq!(
        error,
        TranscribeError::Engine("the provider failed (HTTP 500)".into())
    );
    assert_eq!(requests(&server).await, 3);
    assert_eq!(wait.waits(), [ms(100), ms(200)]);
}

#[tokio::test]
async fn other_client_errors_are_not_retried() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(413), None).await;
    let (engine, _) = engine(config(&server));
    let error = engine.transcribe(&audio()).await.unwrap_err();
    assert_eq!(
        error,
        TranscribeError::Engine("the provider rejected the request (HTTP 413)".into())
    );
    assert_eq!(requests(&server).await, 1);
}

#[tokio::test]
async fn a_slow_reply_times_out_and_is_retried() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(200)
            .set_body_json(reply())
            .set_delay(Duration::from_secs(5)),
        None,
    )
    .await;
    let (engine, wait) = engine(Config {
        timeout: ms(150),
        retry: RetryPolicy {
            max_attempts: 2,
            ..config(&server).retry
        },
        ..config(&server)
    });
    assert_eq!(
        engine.transcribe(&audio()).await,
        Err(TranscribeError::Timeout)
    );
    assert_eq!(wait.waits(), [ms(100)]);
}

#[tokio::test]
async fn malformed_json_is_an_error_without_retrying() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(200).set_body_string(format!("{{\"text\": \"{TRANSCRIPT}")),
        None,
    )
    .await;
    let (engine, wait) = engine(config(&server));
    let error = engine.transcribe(&audio()).await.unwrap_err();
    assert!(
        matches!(&error, TranscribeError::Engine(m) if m.starts_with("unreadable reply")),
        "{error}"
    );
    assert!(!error.to_string().contains("quick brown fox"));
    assert_eq!(requests(&server).await, 1);
    assert!(wait.waits().is_empty());
}

#[tokio::test]
async fn an_unreachable_server_is_a_network_error_after_retries() {
    // Port 9 (discard): nothing listens there. wiremock pools its
    // servers, so a dropped server's address may come back.
    let uri = "http://127.0.0.1:9".to_owned();
    let (engine, wait) = engine(Config {
        base_url: uri,
        ..config(&MockServer::start().await)
    });
    let error = engine.transcribe(&audio()).await.unwrap_err();
    assert!(matches!(error, TranscribeError::Network(_)), "{error:?}");
    assert_eq!(wait.waits(), [ms(100), ms(200)]);
}

#[tokio::test]
async fn audio_that_cant_be_encoded_is_an_engine_error() {
    let server = MockServer::start().await;
    let (engine, _) = engine(config(&server));
    let bad = Audio {
        sample_rate: 0,
        samples: vec![0.0; 10],
    };
    assert!(matches!(
        engine.transcribe(&bad).await,
        Err(TranscribeError::Engine(_))
    ));
    assert_eq!(requests(&server).await, 0);
}

// Criterion 4: across success and every error path, at every log level,
// neither logs nor errors (Display or Debug) contain the key, and the
// transcript never reaches the log.
#[tokio::test]
async fn the_key_never_appears_in_logs_or_errors() {
    let (captured, _guard) = Captured::install();

    let key_body =
        serde_json::json!({"error": {"message": format!("Incorrect API key provided: {KEY}")}});
    let cases: Vec<(ResponseTemplate, Option<Duration>)> = vec![
        (ResponseTemplate::new(200).set_body_json(reply()), None),
        (
            ResponseTemplate::new(401).set_body_json(key_body.clone()),
            None,
        ),
        (
            ResponseTemplate::new(403).set_body_json(key_body.clone()),
            None,
        ),
        (
            ResponseTemplate::new(429).insert_header("retry-after", "1"),
            None,
        ),
        (
            ResponseTemplate::new(500).set_body_json(key_body.clone()),
            None,
        ),
        (ResponseTemplate::new(400).set_body_json(key_body), None),
        (ResponseTemplate::new(200).set_body_string("{oops"), None),
        (
            ResponseTemplate::new(200)
                .set_body_json(reply())
                .set_delay(Duration::from_secs(5)),
            Some(ms(100)),
        ),
    ];
    let mut shown = String::new();
    for (response, timeout) in cases {
        let server = MockServer::start().await;
        mount(&server, response, None).await;
        let mut config = config(&server);
        if let Some(timeout) = timeout {
            config.timeout = timeout;
        }
        let (engine, _) = engine(config);
        shown.push_str(&format!("{engine:?}\n"));
        match engine.transcribe(&audio()).await {
            Ok(text) => assert_eq!(text, TRANSCRIPT),
            Err(error) => shown.push_str(&format!("{error}\n{error:?}\n")),
        }
    }
    // Unreachable server, too.
    let (engine, _) = engine(Config {
        base_url: "http://127.0.0.1:9".into(),
        ..config(&MockServer::start().await)
    });
    if let Err(error) = engine.transcribe(&audio()).await {
        shown.push_str(&format!("{error}\n{error:?}\n"));
    }

    let logs = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(
        logs.contains("transcribing"),
        "nothing was captured:\n{logs}"
    );
    assert!(
        logs.contains("retrying") && logs.contains("giving up"),
        "{logs}"
    );
    assert!(!logs.contains(KEY), "the key is in the logs:\n{logs}");
    assert!(!shown.contains(KEY), "the key is in an error:\n{shown}");
    assert!(
        !logs.contains("quick brown fox"),
        "the transcript is in the logs:\n{logs}"
    );
    assert!(
        logs.lines().all(|l| !(l.contains(" INFO ")
            || l.contains(" WARN ")
            || l.contains(" ERROR "))
            || !l.contains("dictation_transcribe")),
        "the engine logged above debug:\n{logs}"
    );
}
