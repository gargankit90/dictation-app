//! [`check_key`]: does this API key work with this provider?
//!
//! The call that proves a key is the one dictation makes: a short silent
//! clip sent through [`OpenRouter`] with the dictation model and its privacy
//! filter. Unlike `GET /key` it also shows that the account has credits and
//! that a provider takes the request with the filter. It costs next to
//! nothing: 0.25 s of audio. Nothing is retried, so a button press gets its
//! answer in one round trip, not after three.
//!
//! The key goes only into the request's `Authorization` header, as for
//! dictation; the result is a [`TranscribeError`], whose messages never
//! contain the key, the audio or a transcript.

use std::time::Duration;

use dictation_core::retry::RetryPolicy;
use dictation_ports::{Audio, Secret, TranscribeError, Transcriber};

use crate::openrouter::{Config, OpenRouter};

/// How long the test clip lasts. Providers reject clips under 0.1 s.
const CLIP: Duration = Duration::from_millis(250);

/// The sample rate of the test clip, the one dictation uses.
const SAMPLE_RATE: u32 = 16_000;

/// How long the check may take before it reports a timeout. Shorter than a
/// dictation's, because someone is watching a button.
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// Sends a silent clip to `config`'s endpoint with `key`.
///
/// `config`'s timeout and retry policy are replaced: [`TIMEOUT`] and a
/// single attempt.
///
/// # Errors
///
/// [`TranscribeError::InvalidKey`] if the provider rejects the key (401 or
/// 403), [`TranscribeError::OutOfCredits`] for a 402,
/// [`TranscribeError::NoRoute`] for a 404, [`TranscribeError::RateLimited`]
/// for a 429,
/// [`TranscribeError::Timeout`] or [`TranscribeError::Network`] if it can't
/// be reached, and [`TranscribeError::Engine`] for a server error or any
/// other reply.
pub async fn check_key(config: Config, key: Secret) -> Result<(), TranscribeError> {
    check_key_within(config, key, TIMEOUT).await
}

/// [`check_key`] with another time budget; tests use a short one instead of
/// waiting out the real 15 seconds. The app calls [`check_key`].
///
/// # Errors
///
/// As [`check_key`].
pub async fn check_key_within(
    config: Config,
    key: Secret,
    timeout: Duration,
) -> Result<(), TranscribeError> {
    let engine = OpenRouter::new(check_config(config, timeout), key)?;
    engine.transcribe(&clip()).await.map(|_| ())
}

/// What the check sends, whatever the dictation `config` says: its own
/// `timeout` and a single attempt.
fn check_config(config: Config, timeout: Duration) -> Config {
    Config {
        timeout,
        // The check's budget is fixed, whatever the (tiny) clip.
        timeout_grows_with_audio: false,
        retry: RetryPolicy {
            max_attempts: 1,
            ..config.retry
        },
        ..config
    }
}

/// [`CLIP`] of silence at [`SAMPLE_RATE`]: 4,000 samples.
fn clip() -> Audio {
    let samples = usize::try_from(CLIP.as_millis() * u128::from(SAMPLE_RATE) / 1000).unwrap_or(0);
    Audio {
        sample_rate: SAMPLE_RATE,
        samples: vec![0.0; samples],
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests fail on purpose")]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// Obviously fake.
    const KEY: &str = "test-key-NOT-REAL-check-1a2b3c";

    fn config(server: &MockServer) -> Config {
        Config {
            base_url: format!("{}/api/v1", server.uri()),
            ..Config::openrouter()
        }
    }

    async fn serve(response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/audio/transcriptions"))
            .and(header("authorization", format!("Bearer {KEY}").as_str()))
            .respond_with(response)
            .expect(1) // never retried
            .mount(&server)
            .await;
        server
    }

    #[test]
    fn the_budget_is_fifteen_seconds_and_the_clip_is_a_quarter_second() {
        assert_eq!(TIMEOUT, Duration::from_secs(15));
        let clip = clip();
        assert_eq!(clip.sample_rate, 16_000);
        assert_eq!(clip.samples.len(), 4_000);
        assert!(clip.samples.iter().all(|s| *s == 0.0));
    }

    /// The check's own policy replaces the dictation's, field by field.
    #[test]
    fn the_check_replaces_timeout_and_retries() {
        let dictation = Config {
            timeout: Duration::from_secs(600),
            retry: RetryPolicy {
                max_attempts: 5,
                ..RetryPolicy::default()
            },
            ..Config::openrouter()
        };
        let checked = check_config(dictation.clone(), TIMEOUT);
        assert_eq!(checked.timeout, TIMEOUT);
        assert_eq!(checked.retry.max_attempts, 1);
        // Everything else is the caller's.
        assert_eq!(checked.base_url, dictation.base_url);
        assert_eq!(checked.model, dictation.model);
        assert_eq!(checked.privacy, dictation.privacy);
        assert_eq!(checked.retry.base_delay, dictation.retry.base_delay);
    }

    #[tokio::test]
    async fn a_good_key_succeeds_even_when_the_clip_has_no_words() {
        let server =
            serve(ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": ""}))).await;
        assert_eq!(check_key(config(&server), Secret::new(KEY)).await, Ok(()));
    }

    /// Standard base64 back to bytes, for reading the upload.
    fn unbase64(text: &str) -> Vec<u8> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bits = 0u32;
        let mut count = 0;
        let mut out = Vec::new();
        for c in text.bytes().filter(|&c| c != b'=') {
            let value = ALPHABET.iter().position(|&a| a == c).unwrap() as u32;
            bits = (bits << 6) | value;
            count += 6;
            if count >= 8 {
                count -= 8;
                out.push((bits >> count) as u8);
            }
        }
        out
    }

    /// The upload is exactly what the provider needs to judge the key: 0.25 s
    /// of 16 kHz mono 16-bit silence, the check's model and privacy filter,
    /// and no language.
    #[tokio::test]
    async fn check_key_uploads_exactly_250ms_mono_silence() {
        let server =
            serve(ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": ""}))).await;
        let dictation = Config {
            timeout: Duration::from_secs(600),
            ..config(&server)
        };
        check_key(dictation, Secret::new(KEY)).await.unwrap();
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        let mut fields: Vec<&str> = body
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(fields, ["input_audio", "model", "provider"], "no language");
        assert_eq!(body["model"], "openai/gpt-4o-mini-transcribe");
        assert_eq!(body["provider"]["data_collection"], "deny");
        assert_eq!(body["input_audio"]["format"], "wav");
        let wav = unbase64(body["input_audio"]["data"].as_str().unwrap());
        let mut reader = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 16_000);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples.len(), 4_000, "250 ms at 16 kHz");
        assert!(samples.iter().all(|s| *s == 0), "silence");
    }

    #[tokio::test]
    async fn each_failure_maps_to_its_error_without_a_retry() {
        for (status, expected) in [
            (401, TranscribeError::InvalidKey),
            (402, TranscribeError::OutOfCredits),
            (403, TranscribeError::InvalidKey),
            (404, TranscribeError::NoRoute),
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
            base_url: format!("http://127.0.0.1:{port}/api/v1"),
            ..Config::openrouter()
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
            // A 401 body may echo part of the key; it must stay unread.
            ResponseTemplate::new(401).set_body_string(format!("bad key {KEY}")),
        )
        .await;
        let error = check_key(config(&server), Secret::new(KEY))
            .await
            .unwrap_err();
        assert!(!format!("{error} {error:?}").contains(KEY));
    }
}
