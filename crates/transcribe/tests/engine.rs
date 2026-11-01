//! M4.4: the engine built from settings and the keychain, against wiremock.
//! No real provider and no real key; `Retry-After: 0` keeps the retries from
//! really waiting.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::time::Duration;

use dictation_core::settings::{Provider, Settings};
use dictation_ports::{Audio, Secret, SecretError, TranscribeError, Transcriber};
use dictation_transcribe::engine::{Engine, Transcribers};
use dictation_transcribe::mock::MockTranscriber;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// Obviously fake; the leak checks look for it.
const KEY: &str = "gsk_test-key-NOT-REAL-4c1e";
const OTHER_KEY: &str = "gsk_other-key-NOT-REAL-9a2d";
const TRANSCRIPT: &str = "hello from the fake provider";

fn audio() -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.1; 1_600],
    }
}

fn settings(model: &str, language: Option<&str>) -> Settings {
    Settings {
        model: model.into(),
        language: language.map(str::to_owned),
        ..Settings::default()
    }
}

fn key(value: &str) -> Result<Option<Secret>, SecretError> {
    Ok(Some(Secret::new(value)))
}

fn engine_at(server: &MockServer, settings: &Settings, key: &str) -> Engine {
    Engine::from_settings_at(settings, self::key(key), &server.uri())
}

fn reply() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({ "text": TRANSCRIPT }))
}

async fn provider(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn with_a_key_the_settings_decide_the_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .and(|request: &Request| {
            // The body holds the WAV bytes too, so it isn't valid UTF-8.
            let body = String::from_utf8_lossy(&request.body);
            body.contains("whisper-test-model") && body.contains("name=\"language\"")
        })
        .respond_with(reply())
        .expect(1)
        .mount(&server)
        .await;
    let engine = engine_at(&server, &settings("whisper-test-model", Some("de")), KEY);
    assert_eq!(engine.name(), "remote");
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), TRANSCRIPT);
}

#[tokio::test]
async fn no_language_setting_lets_the_provider_detect_it() {
    let server = provider(reply()).await;
    let engine = engine_at(&server, &settings("m", None), KEY);
    engine.transcribe(&audio()).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(!body.contains("name=\"language\""), "{body}");
}

#[tokio::test]
async fn without_a_key_dictation_fails_at_once_and_sends_nothing() {
    let server = provider(reply()).await;
    let engine = Engine::from_settings_at(&Settings::default(), Ok(None), &server.uri());
    assert_eq!(engine.name(), "unavailable");
    assert_eq!(
        engine.transcribe(&audio()).await,
        Err(TranscribeError::MissingKey)
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_unreadable_keychain_is_a_keychain_error_that_says_why() {
    for (denied, words) in [
        (SecretError::AccessDenied, "keychain access was denied"),
        (
            SecretError::Store("locked".into()),
            "keychain error: locked",
        ),
    ] {
        let engine = Engine::from_settings(&Settings::default(), Err(denied));
        let Err(error @ TranscribeError::KeychainUnavailable(_)) =
            engine.transcribe(&audio()).await
        else {
            panic!("expected a keychain error");
        };
        assert!(error.to_string().contains(words), "{error}");
        assert!(error.user_message().contains("from the Keychain"));
    }
}

#[tokio::test]
async fn each_provider_error_maps_to_its_user_message_and_the_key_stays_out() {
    let cases = [
        (401, TranscribeError::InvalidKey, 1),
        (403, TranscribeError::InvalidKey, 1),
        (429, TranscribeError::RateLimited, 3),
        (
            400,
            TranscribeError::Engine("the provider rejected the request (HTTP 400)".into()),
            1,
        ),
        (
            503,
            TranscribeError::Engine("the provider failed (HTTP 503)".into()),
            3,
        ),
    ];
    for (status, error, attempts) in cases {
        let server = provider(
            ResponseTemplate::new(status)
                .insert_header("retry-after", "0")
                // Real providers can echo part of the key in an error body.
                .set_body_string(format!("bad key {KEY}")),
        )
        .await;
        let engine = engine_at(&server, &Settings::default(), KEY);
        let got = engine.transcribe(&audio()).await.unwrap_err();
        assert_eq!(got, error, "HTTP {status}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            attempts,
            "HTTP {status}"
        );
        for text in [got.to_string(), got.user_message()] {
            assert!(!text.contains(KEY), "HTTP {status}: {text}");
            assert!(!text.contains("bad key"), "HTTP {status}: {text}");
        }
        assert!(!got.user_message().is_empty());
    }
}

#[tokio::test]
async fn a_provider_hiccup_is_retried_and_the_dictation_still_works() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(503).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(reply())
        .mount(&server)
        .await;
    let engine = engine_at(&server, &Settings::default(), KEY);
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), TRANSCRIPT);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn an_unreachable_provider_is_a_network_error() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let engine = Engine::from_settings_at(
        &Settings::default(),
        key(KEY),
        &format!("http://127.0.0.1:{port}"),
    );
    let error = engine.transcribe(&audio()).await.unwrap_err();
    assert!(matches!(error, TranscribeError::Network(_)), "{error:?}");
    assert!(!error.to_string().contains(KEY));
}

#[tokio::test]
async fn a_new_engine_replaces_the_old_one_for_the_next_dictation() {
    let first = provider(ResponseTemplate::new(401)).await;
    let second = provider(reply()).await;
    let transcribers = Transcribers::new(Engine::Unavailable(TranscribeError::MissingKey));
    assert_eq!(transcribers.name(), "unavailable");
    assert_eq!(
        transcribers.transcribe(&audio()).await,
        Err(TranscribeError::MissingKey)
    );

    // A wrong key is saved: the provider says no.
    transcribers.update(|| engine_at(&first, &Settings::default(), OTHER_KEY));
    assert_eq!(transcribers.name(), "remote");
    assert_eq!(
        transcribers.transcribe(&audio()).await,
        Err(TranscribeError::InvalidKey)
    );

    // The settings change (another server stands in for another provider):
    // the very next dictation uses it, with the new key.
    transcribers.update(|| engine_at(&second, &settings("whisper-1", Some("en")), KEY));
    assert_eq!(transcribers.transcribe(&audio()).await.unwrap(), TRANSCRIPT);
    assert_eq!(first.received_requests().await.unwrap().len(), 1);
    let sent = second.received_requests().await.unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        format!("Bearer {KEY}")
    );
    assert!(String::from_utf8_lossy(&sent[0].body).contains("whisper-1"));
}

#[tokio::test]
async fn a_transcription_in_flight_finishes_with_the_engine_it_started_with() {
    let slow = provider(reply().set_delay(Duration::from_millis(300))).await;
    let transcribers = Transcribers::new(engine_at(&slow, &Settings::default(), KEY));
    let running = {
        let transcribers = transcribers.clone();
        tokio::spawn(async move { transcribers.transcribe(&audio()).await })
    };
    // Wait until the request is really in flight.
    while slow.received_requests().await.unwrap().is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    transcribers.update(|| Engine::Unavailable(TranscribeError::MissingKey));
    assert_eq!(running.await.unwrap().unwrap(), TRANSCRIPT);
    assert_eq!(
        transcribers.transcribe(&audio()).await,
        Err(TranscribeError::MissingKey),
        "the next one uses the new engine"
    );
}

#[tokio::test]
async fn clones_share_the_engine_and_the_mock_passes_through() {
    let transcribers = Transcribers::new(Engine::Mock(MockTranscriber::new("mocked")));
    let clone = transcribers.clone();
    assert_eq!(clone.transcribe(&audio()).await.unwrap(), "mocked");
    transcribers.update(|| Engine::Mock(MockTranscriber::new("changed")));
    assert_eq!(clone.transcribe(&audio()).await.unwrap(), "changed");
    assert_eq!(clone.name(), "mock");
}

#[tokio::test]
async fn the_debug_output_never_shows_the_key() {
    let server = provider(reply()).await;
    let transcribers = Transcribers::new(engine_at(&server, &Settings::default(), KEY));
    assert!(!format!("{transcribers:?}").contains(KEY));
    if let Engine::Remote(engine) = engine_at(&server, &Settings::default(), KEY) {
        assert!(!format!("{engine:?}").contains(KEY));
    }
}

#[test]
fn the_provider_picks_the_default_endpoint() {
    // Built, not called: no request leaves the test.
    for provider in Provider::ALL {
        let settings = Settings {
            provider,
            model: provider.default_model().to_owned(),
            ..Settings::default()
        };
        let engine = Engine::from_settings(&settings, key(KEY));
        assert_eq!(engine.name(), "remote", "{provider}");
    }
}

#[tokio::test]
async fn concurrent_updates_leave_the_last_one_to_finish_in_place() {
    let transcribers = Transcribers::new(Engine::Unavailable(TranscribeError::MissingKey));
    let handles: Vec<_> = (0..8)
        .map(|n| {
            let transcribers = transcribers.clone();
            std::thread::spawn(move || {
                transcribers.update(|| Engine::Mock(MockTranscriber::new(format!("engine {n}"))));
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let text = transcribers.transcribe(&audio()).await.unwrap();
    assert!(text.starts_with("engine "), "{text}");
}

#[tokio::test]
async fn the_swappable_engine_passes_no_speech_prob_on_for_the_hallucination_filter() {
    let mock = MockTranscriber::default().with_no_speech_prob(0.9);
    let transcribers = Transcribers::new(Engine::Mock(mock));
    let detailed = transcribers.transcribe_detailed(&audio()).await.unwrap();
    assert_eq!(detailed.no_speech_prob, Some(0.9));
    assert_eq!(
        Engine::Unavailable(TranscribeError::MissingKey)
            .transcribe_detailed(&audio())
            .await,
        Err(TranscribeError::MissingKey)
    );
}
