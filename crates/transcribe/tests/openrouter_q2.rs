//! Q2 criterion 3: the `OpenRouter` speech and cleanup clients against a
//! local HTTP fake. Request bodies carry the privacy filters and every
//! request the attribution headers; 401/402/429/5xx/no-route map to typed
//! errors; the key never appears in an error. The key is obviously fake.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::time::Duration;

use dictation_core::cleanup;
use dictation_core::retry::RetryPolicy;
use dictation_ports::{Audio, Cleaner, CleanupError, Secret, TranscribeError, Transcriber};
use dictation_transcribe::cleanup::OpenRouterCleanup;
use dictation_transcribe::engine::{Engine, Transcribers};
use dictation_transcribe::mock::MockTranscriber;
use dictation_transcribe::openrouter::{Config, OpenRouter};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-or-v1-NOT-A-REAL-KEY-q2-0123456789";

fn audio() -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.25; 1_600],
    }
}

fn speech(server: &MockServer) -> OpenRouter {
    let config = Config {
        base_url: format!("{}/api/v1", server.uri()),
        retry: RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        },
        ..Config::openrouter()
    };
    OpenRouter::new(config, Secret::new(KEY)).unwrap()
}

fn cleaner(server: &MockServer) -> OpenRouterCleanup {
    OpenRouterCleanup::at(&format!("{}/api/v1", server.uri()), Secret::new(KEY)).unwrap()
}

/// Answers `path` with `response`, but only for a request with the key and
/// both attribution headers.
async fn serve(path_: &str, response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(path_))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .and(header("http-referer", "https://vocetlabs.com"))
        .and(header("x-title", "Vocet"))
        .and(header("content-type", "application/json"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

fn chat_reply(text: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": text}, "finish_reason": "stop"}]
    }))
}

async fn body_of(server: &MockServer) -> serde_json::Value {
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    serde_json::from_slice(&requests[0].body).unwrap()
}

#[tokio::test]
async fn speech_sends_the_filter_and_the_attribution_headers() {
    let server = serve(
        "/api/v1/audio/transcriptions",
        ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": "Hello."})),
    )
    .await;
    assert_eq!(
        speech(&server).transcribe(&audio()).await.unwrap(),
        "Hello."
    );
    let body = body_of(&server).await;
    assert_eq!(body["model"], "openai/gpt-4o-mini-transcribe");
    // gpt-4o-mini-transcribe has no zero-retention route; no training.
    assert_eq!(
        body["provider"],
        serde_json::json!({"data_collection": "deny"})
    );
    assert!(body.get("prompt").is_none(), "nothing sets a prompt yet");
}

#[tokio::test]
async fn cleanup_sends_zdr_low_reasoning_and_the_prompt() {
    let server = serve("/api/v1/chat/completions", chat_reply("Hello there.")).await;
    let reply = cleaner(&server).clean("um, hello there").await.unwrap();
    assert_eq!(reply, "Hello there.");
    let body = body_of(&server).await;
    assert_eq!(body["model"], "z-ai/glm-5.3-flash");
    assert_eq!(
        body["provider"],
        serde_json::json!({"zdr": true, "data_collection": "deny"})
    );
    assert_eq!(body["reasoning"]["effort"], "low");
    assert_eq!(body["temperature"], 0);
    assert_eq!(body["messages"][0]["content"], cleanup::prompt());
    assert_eq!(
        body["messages"][1]["content"],
        "<transcript>\num, hello there\n</transcript>"
    );
}

#[tokio::test]
async fn speech_statuses_map_to_typed_errors_without_the_key() {
    for (status, expected) in [
        (401, TranscribeError::InvalidKey),
        (402, TranscribeError::OutOfCredits),
        (404, TranscribeError::NoRoute),
        (429, TranscribeError::RateLimited),
        (
            503,
            TranscribeError::Engine("the provider failed (HTTP 503)".into()),
        ),
    ] {
        // Bodies that echo the key are never read.
        let server = serve(
            "/api/v1/audio/transcriptions",
            ResponseTemplate::new(status).set_body_string(format!("bad key {KEY}")),
        )
        .await;
        let engine = speech(&server);
        let error = engine.transcribe(&audio()).await.unwrap_err();
        assert_eq!(error, expected, "HTTP {status}");
        let shown = format!("{error} {error:?} {engine:?}");
        assert!(!shown.contains(KEY), "{shown}");
    }
}

#[tokio::test]
async fn out_of_credits_tells_the_user_to_add_credits() {
    assert_eq!(
        TranscribeError::OutOfCredits.code().user_text(),
        "Your OpenRouter credits ran out. Add credits."
    );
}

#[tokio::test]
async fn cleanup_statuses_map_to_typed_errors_without_the_key() {
    for (status, expected) in [
        (401, CleanupError::InvalidKey),
        (402, CleanupError::OutOfCredits),
        (404, CleanupError::NoRoute),
        (429, CleanupError::RateLimited),
        (500, CleanupError::Provider(500)),
    ] {
        let server = serve(
            "/api/v1/chat/completions",
            ResponseTemplate::new(status).set_body_string(format!("bad key {KEY}")),
        )
        .await;
        let cleaner = cleaner(&server);
        let error = cleaner.clean("text").await.unwrap_err();
        assert_eq!(error, expected, "HTTP {status}");
        assert!(!format!("{error} {error:?} {cleaner:?}").contains(KEY));
    }
    // A reply that isn't a chat reply.
    let bad = serve(
        "/api/v1/chat/completions",
        ResponseTemplate::new(200).set_body_string("{}"),
    )
    .await;
    assert_eq!(cleaner(&bad).clean("t").await, Err(CleanupError::BadReply));
}

/// Review Q2 round 1: a reply cut off by the token limit is a bad reply,
/// so the pipeline pastes the raw text instead of a clipped sentence.
#[tokio::test]
async fn a_truncated_cleanup_is_a_bad_reply() {
    let server = serve(
        "/api/v1/chat/completions",
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "Please send the report"},
                         "finish_reason": "length"}]
        })),
    )
    .await;
    assert_eq!(
        cleaner(&server)
            .clean("Please send the report and cancel the meeting.")
            .await,
        Err(CleanupError::BadReply)
    );
}

#[tokio::test]
async fn a_slow_cleanup_times_out() {
    let server = serve(
        "/api/v1/chat/completions",
        chat_reply("late").set_delay(Duration::from_secs(5)),
    )
    .await;
    let cleaner = cleaner(&server).with_timeout(Duration::from_millis(100));
    assert_eq!(cleaner.clean("text").await, Err(CleanupError::Timeout));
}

/// The pipeline's cleaner is the current engine's: `OpenRouter` cleans, the
/// mock engine is off.
#[tokio::test(flavor = "multi_thread")]
async fn transcribers_clean_with_the_current_engine() {
    let server = serve("/api/v1/chat/completions", chat_reply("Cleaned.")).await;
    let engine = Engine::remote_at(
        Ok(Some(Secret::new(KEY))),
        &format!("{}/api/v1", server.uri()),
    );
    let transcribers = Transcribers::new(engine);
    assert_eq!(transcribers.clean("raw").await.unwrap(), "Cleaned.");
    transcribers.update(|| Engine::Mock(MockTranscriber::new("mocked")));
    assert_eq!(transcribers.clean("raw").await, Err(CleanupError::Off));
    let none = Engine::remote(Ok(None));
    transcribers.update(|| none);
    assert_eq!(transcribers.clean("raw").await, Err(CleanupError::NoKey));
}
