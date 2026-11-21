//! QT1: what goes over the wire to `OpenRouter` on every path: the privacy
//! filter and the attribution headers on EVERY request (retries and the key
//! check included), and the key in no log line on the speech, cleanup and
//! key-check paths, even when the provider echoes it. The key is fake.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

mod common;

use common::Captured;
use dictation_core::settings::Settings;
use dictation_ports::{Audio, Cleaner, Secret, Transcriber};
use dictation_transcribe::check_key::check_key;
use dictation_transcribe::cleanup::OpenRouterCleanup;
use dictation_transcribe::engine::Engine;
use dictation_transcribe::openrouter::Config;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const KEY: &str = "sk-or-v1-NOT-A-REAL-KEY-qt1-9876543210";
const TRANSCRIPT: &str = "zebra marmalade transcript";

fn audio() -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.25; 1_600],
    }
}

fn base(server: &MockServer) -> String {
    format!("{}/api/v1", server.uri())
}

fn speech_engine(server: &MockServer) -> Engine {
    Engine::from_settings_at(
        &Settings::default(),
        Ok(Some(Secret::new(KEY))),
        &base(server),
    )
}

/// Answers every POST with the next response, the last one repeating.
async fn serve(responses: Vec<ResponseTemplate>) -> MockServer {
    let server = MockServer::start().await;
    let next = std::sync::atomic::AtomicUsize::new(0);
    Mock::given(method("POST"))
        .respond_with(move |_: &Request| {
            let n = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            responses[n.min(responses.len() - 1)].clone()
        })
        .mount(&server)
        .await;
    server
}

fn retry_then_ok() -> Vec<ResponseTemplate> {
    vec![
        ResponseTemplate::new(503).insert_header("retry-after", "0"),
        ResponseTemplate::new(429).insert_header("retry-after", "0"),
        ResponseTemplate::new(200).set_body_json(serde_json::json!({ "text": TRANSCRIPT })),
    ]
}

/// Every request had the key, the attribution and a deny filter.
async fn assert_every_request_is_attributed_and_filtered(server: &MockServer, want: usize) {
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), want, "requests");
    for request in requests {
        let header = |name: &str| {
            request
                .headers
                .get(name)
                .map(|v| v.to_str().unwrap().to_owned())
        };
        assert_eq!(header("authorization"), Some(format!("Bearer {KEY}")));
        assert_eq!(
            header("http-referer").as_deref(),
            Some("https://vocetlabs.com")
        );
        assert_eq!(header("x-title").as_deref(), Some("Vocet"));
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["provider"]["data_collection"], "deny", "{body}");
    }
}

#[tokio::test]
async fn speech_retries_keep_the_attribution_and_the_filter() {
    let server = serve(retry_then_ok()).await;
    let text = speech_engine(&server).transcribe(&audio()).await.unwrap();
    assert_eq!(text, TRANSCRIPT);
    assert_every_request_is_attributed_and_filtered(&server, 3).await;
}

#[tokio::test]
async fn the_key_check_keeps_the_attribution_and_the_filter() {
    let server = serve(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!({ "text": "" })),
    ])
    .await;
    let config = Config {
        base_url: base(&server),
        ..Config::openrouter()
    };
    check_key(config, Secret::new(KEY)).await.unwrap();
    assert_every_request_is_attributed_and_filtered(&server, 1).await;
}

#[tokio::test]
async fn cleanup_sends_zdr_and_deny_with_the_attribution() {
    let server = serve(vec![ResponseTemplate::new(200).set_body_json(
        serde_json::json!({
            "choices": [{"message": {"content": "Hello."}, "finish_reason": "stop"}]
        }),
    )])
    .await;
    let cleaner = OpenRouterCleanup::at(&base(&server), Secret::new(KEY)).unwrap();
    assert_eq!(cleaner.clean("um hello").await.unwrap(), "Hello.");
    assert_every_request_is_attributed_and_filtered(&server, 1).await;
    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["provider"]["zdr"], true);
}

/// The key and the transcript reach no log line on any path, at any level,
/// even when the provider's error body echoes the key.
#[tokio::test]
async fn the_key_stays_out_of_logs_on_the_speech_cleanup_and_key_check_paths() {
    let (captured, _guard) = Captured::install();
    let mut shown = String::new();
    for status in [200, 401, 402, 404, 429, 500] {
        let response = if status == 200 {
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": TRANSCRIPT,
                "choices": [{"message": {"content": TRANSCRIPT}, "finish_reason": "stop"}]
            }))
        } else {
            ResponseTemplate::new(status)
                .insert_header("retry-after", "0")
                .set_body_json(serde_json::json!({"error": {"message": format!("bad key {KEY}")}}))
        };
        let server = serve(vec![response]).await;
        if let Err(error) = speech_engine(&server).transcribe(&audio()).await {
            shown.push_str(&format!("{error} {error:?}\n"));
        }
        let cleaner = OpenRouterCleanup::at(&base(&server), Secret::new(KEY)).unwrap();
        shown.push_str(&format!("{cleaner:?}\n"));
        if let Err(error) = cleaner.clean(TRANSCRIPT).await {
            shown.push_str(&format!("{error} {error:?}\n"));
        }
        let config = Config {
            base_url: base(&server),
            ..Config::openrouter()
        };
        if let Err(error) = check_key(config, Secret::new(KEY)).await {
            shown.push_str(&format!("{error} {error:?}\n"));
        }
    }
    let logs = captured.text();
    assert!(logs.contains("cleanup"), "nothing was captured: {logs}");
    for (what, text) in [("logs", &logs), ("messages", &shown)] {
        assert!(!text.contains("sk-or-"), "the key is in the {what}: {text}");
        assert!(!text.contains("zebra"), "the transcript is in the {what}");
    }
}
