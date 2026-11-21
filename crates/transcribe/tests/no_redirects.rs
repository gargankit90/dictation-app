//! Issue #76: the shared HTTP client never follows a redirect, so a
//! provider can't send us to (and have us accept a transcript from) a
//! server the user didn't configure. No real key: the key is fake.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail the test on purpose"
)]

use dictation_ports::{Audio, Secret, TranscribeError, Transcriber};
use dictation_transcribe::check_key::check_key;
use dictation_transcribe::openrouter::{Config, OpenRouter};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "test-key-NOT-REAL-redirect-4d5e6f";
const STATUSES: [u16; 5] = [301, 302, 303, 307, 308];

/// `(provider, other)`: the provider answers `status` with a `Location` on
/// `other`, which would answer 200 with a transcript to anything.
async fn servers(status: u16) -> (MockServer, MockServer) {
    let provider = MockServer::start().await;
    let other = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(status)
                .insert_header("location", format!("{}/other", other.uri()).as_str()),
        )
        .mount(&provider)
        .await;
    Mock::given(path("/other"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"text": "from other host"})),
        )
        .mount(&other)
        .await;
    (provider, other)
}

fn config(provider: &MockServer) -> Config {
    let mut config = Config::openrouter();
    config.base_url = format!("{}/v1", provider.uri());
    config
}

/// A redirect is an engine error that names the status.
fn assert_redirect_error<T: std::fmt::Debug>(result: Result<T, TranscribeError>, status: u16) {
    let Err(TranscribeError::Engine(detail)) = result else {
        panic!("{status}: {result:?}");
    };
    assert!(
        detail.contains(&format!("redirect (HTTP {status})")),
        "{detail}"
    );
}

async fn assert_untouched(provider: &MockServer, other: &MockServer) {
    assert!(other.received_requests().await.unwrap().is_empty());
    // A redirect is a provider error, not a retryable one: one request only.
    assert_eq!(provider.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn transcribe_does_not_follow_redirects() {
    for status in STATUSES {
        let (provider, other) = servers(status).await;
        let engine = OpenRouter::new(config(&provider), Secret::new(KEY)).unwrap();
        let audio = Audio {
            sample_rate: 16_000,
            samples: vec![0.0; 1_600],
        };
        let result = engine.transcribe(&audio).await;
        assert_redirect_error(result, status);
        assert_untouched(&provider, &other).await;
    }
}

#[tokio::test]
async fn check_key_does_not_follow_redirects() {
    for status in STATUSES {
        let (provider, other) = servers(status).await;
        let result = check_key(config(&provider), Secret::new(KEY)).await;
        assert_redirect_error(result, status);
        assert_untouched(&provider, &other).await;
    }
}

/// A 3xx's error names the HTTP status and says redirects aren't followed
/// (#86), and holds neither the redirect target nor the key.
#[tokio::test]
async fn a_redirect_error_names_the_status_and_leaks_nothing() {
    for status in STATUSES {
        let (provider, other) = servers(status).await;
        let error = check_key(config(&provider), Secret::new(KEY))
            .await
            .unwrap_err();
        let shown = error.to_string();
        assert!(shown.contains(&format!("HTTP {status}")), "{shown}");
        assert!(shown.contains("redirect"), "{shown}");
        assert!(shown.contains("isn't followed"), "{shown}");
        assert!(!shown.contains(&other.uri()), "{shown}");
        assert!(!shown.contains(KEY), "{shown}");
        assert!(!format!("{error:?}").contains(KEY));
    }
}
