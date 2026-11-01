//! #41: a provider's error body never reaches a `TranscribeError`, so it
//! cannot reach the info log (the pipeline logs the error's text) or the
//! user. The body here echoes a key and transcript-like text on purpose.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use dictation_ports::{Audio, Secret, TranscribeError, Transcriber};
use dictation_transcribe::openai_compat::{Config, GROQ, OpenAiCompat};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "gsk_bodycheck-NOT-REAL-12ab";
const BODY: &str = "{\"error\":{\"message\":\"bad key gsk_bodycheck-NOT-REAL-12ab; heard: pineapple budget meeting\"}}";

async fn error_for(status: u16, body: &str) -> TranscribeError {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .mount(&server)
        .await;
    let mut config = Config::from_preset(GROQ);
    config.base_url = format!("{}/v1", server.uri());
    // No retries to wait for.
    config.retry.max_attempts = 1;
    let engine = OpenAiCompat::new(config, Secret::new(KEY)).unwrap();
    let audio = Audio {
        sample_rate: 16_000,
        samples: vec![0.0; 1_600],
    };
    engine.transcribe(&audio).await.unwrap_err()
}

#[tokio::test]
async fn provider_bodies_never_reach_the_error() {
    // Errors by status class, and a 200 whose body is not the JSON we expect.
    for (status, body) in [
        (400, BODY),
        (401, BODY),
        (404, BODY),
        (413, BODY),
        (429, BODY),
        (500, BODY),
        (503, BODY),
        (200, BODY),
        (200, "pineapple budget meeting"),
    ] {
        let error = error_for(status, body).await;
        for text in [
            error.to_string(),
            format!("{error:?}"),
            error.user_message(),
        ] {
            assert!(!text.contains("pineapple"), "{status}: {text}");
            assert!(!text.contains("gsk_bodycheck"), "{status}: {text}");
            assert!(text.len() < 200, "{status}: {text}");
        }
    }
}
