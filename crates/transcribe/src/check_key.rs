//! [`check_key`]: does this API key work with this provider?
//!
//! The cheapest call that proves a key is the one dictation makes: a short
//! silent clip sent through [`OpenAiCompat`]. Unlike `GET /models` it also
//! shows that the key may use the transcription endpoint and the model
//! (OpenAI keys can be restricted to some endpoints, which `/models`
//! wouldn't notice). It costs next to nothing: 0.25 s of audio, which Groq
//! bills as its 10 s minimum (a fraction of a cent) and OpenAI bills as a
//! quarter of a second. Nothing is retried, so a button press gets its
//! answer in one round trip, not after three.
//!
//! The key goes only into the request's `Authorization` header, as for
//! dictation; the result is a [`TranscribeError`], whose messages never
//! contain the key, the audio or a transcript.

use std::time::Duration;

use dictation_core::retry::RetryPolicy;
use dictation_ports::{Audio, Secret, TranscribeError};

use crate::openai_compat::{Config, OpenAiCompat};

/// How long the test clip lasts. Providers reject clips under 0.1 s.
const CLIP: Duration = Duration::from_millis(250);

/// The sample rate of the test clip, the one dictation uses.
const SAMPLE_RATE: u32 = 16_000;

/// How long the check may take before it reports a timeout. Shorter than a
/// dictation's, because someone is watching a button.
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// Sends a silent clip to `config`'s endpoint with `key`.
///
/// `config`'s language, timeout and retry policy are replaced: no
/// language, [`TIMEOUT`] and a single attempt.
///
/// # Errors
///
/// [`TranscribeError::InvalidKey`] if the provider rejects the key (401 or
/// 403), [`TranscribeError::RateLimited`] for a 429,
/// [`TranscribeError::Timeout`] or [`TranscribeError::Network`] if it can't
/// be reached, and [`TranscribeError::Engine`] for a server error or any
/// other reply.
pub async fn check_key(config: Config, key: Secret) -> Result<(), TranscribeError> {
    let config = Config {
        language: None,
        timeout: TIMEOUT,
        retry: RetryPolicy {
            max_attempts: 1,
            ..config.retry
        },
        ..config
    };
    let engine = OpenAiCompat::new(config, key)?;
    let samples = usize::try_from(CLIP.as_millis() * u128::from(SAMPLE_RATE) / 1000).unwrap_or(0);
    let clip = Audio {
        sample_rate: SAMPLE_RATE,
        samples: vec![0.0; samples],
    };
    engine.transcribe_verbose(&clip).await.map(|_| ())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests fail on purpose")]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::openai_compat::GROQ;

    /// Obviously fake.
    const KEY: &str = "test-key-NOT-REAL-check-1a2b3c";

    fn config(server: &MockServer) -> Config {
        Config {
            base_url: format!("{}/openai/v1", server.uri()),
            ..Config::from_preset(GROQ)
        }
    }

    async fn serve(response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .and(header("authorization", format!("Bearer {KEY}").as_str()))
            .respond_with(response)
            .expect(1) // never retried
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn a_good_key_succeeds_even_when_the_clip_has_no_words() {
        let server = serve(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"text": "", "segments": []})),
        )
        .await;
        assert_eq!(check_key(config(&server), Secret::new(KEY)).await, Ok(()));
    }

    #[tokio::test]
    async fn the_request_is_a_short_silent_wav_with_the_provider_model() {
        let server =
            serve(ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": ""}))).await;
        check_key(config(&server), Secret::new(KEY)).await.unwrap();
        let requests = server.received_requests().await.unwrap();
        let body = String::from_utf8_lossy(&requests[0].body).into_owned();
        assert!(body.contains("whisper-large-v3-turbo"), "the model");
        assert!(body.contains("RIFF"), "a WAV file");
        // 0.25 s of 16-bit mono at 16 kHz plus the header, nothing bigger.
        assert!(
            requests[0].body.len() < 10_000,
            "{}",
            requests[0].body.len()
        );
        assert!(!body.contains("name=\"language\""), "no language");
    }

    #[tokio::test]
    async fn each_failure_maps_to_its_error_without_a_retry() {
        for (status, expected) in [
            (401, TranscribeError::InvalidKey),
            (403, TranscribeError::InvalidKey),
            (429, TranscribeError::RateLimited),
        ] {
            let server = serve(ResponseTemplate::new(status)).await;
            let result = check_key(config(&server), Secret::new(KEY)).await;
            assert_eq!(result, Err(expected), "HTTP {status}");
        }
        for status in [500, 503] {
            let server = serve(ResponseTemplate::new(status)).await;
            let result = check_key(config(&server), Secret::new(KEY)).await;
            assert!(
                matches!(&result, Err(TranscribeError::Engine(m)) if m.contains(&status.to_string())),
                "HTTP {status}: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn an_unreachable_server_is_a_network_error() {
        // Bind a port and release it, so nothing listens there.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let config = Config {
            base_url: format!("http://127.0.0.1:{port}/openai/v1"),
            ..Config::from_preset(GROQ)
        };
        let result = check_key(config, Secret::new(KEY)).await;
        assert!(
            matches!(result, Err(TranscribeError::Network(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn errors_never_contain_the_key() {
        let server = serve(
            // OpenAI's 401 body echoes part of the key; it must stay unread.
            ResponseTemplate::new(401).set_body_string(format!("bad key {KEY}")),
        )
        .await;
        let error = check_key(config(&server), Secret::new(KEY))
            .await
            .unwrap_err();
        assert!(!format!("{error} {error:?}").contains(KEY));
    }
}
