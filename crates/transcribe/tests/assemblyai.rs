//! The `AssemblyAI` Dictation API engine against a wiremock server: no real
//! provider, no real key, no real sleeping between retries (`RecordWait`).

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_core::retry::RetryPolicy;
use dictation_core::settings::{Provider, Settings};
use dictation_ports::{Audio, Secret, TranscribeError, Transcriber};
use dictation_transcribe::assemblyai::AssemblyAi;
use dictation_transcribe::assemblyai_prompt::{FAST_INSTRUCTION, FAST_MODEL, SMART_INSTRUCTION};
use dictation_transcribe::check_key::check_key_for;
use dictation_transcribe::engine::Engine;
use dictation_transcribe::openai_compat::{ASSEMBLYAI, Config, Wait};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// Obviously fake; the leak test looks for it everywhere.
const KEY: &str = "test-key-NOT-REAL-aai-5e1b77";
const ENDPOINT: &str = "/v1/transcribe";

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

fn config(server: &MockServer, language: Option<&str>) -> Config {
    Config {
        base_url: server.uri(),
        language: language.map(str::to_owned),
        timeout: Duration::from_secs(5),
        timeout_grows_with_audio: false,
        retry: RetryPolicy {
            max_attempts: 3,
            base_delay: ms(100),
            multiplier: 2,
            max_delay: ms(1_000),
            jitter: 0.0,
        },
        ..Config::from_preset(ASSEMBLYAI)
    }
}

fn engine(config: Config) -> (AssemblyAi<RecordWait>, RecordWait) {
    let wait = RecordWait::default();
    let engine = AssemblyAi::with_wait(config, Secret::new(KEY), wait.clone(), 7).unwrap();
    (engine, wait)
}

fn audio() -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.25; 1_600],
    }
}

fn clip(seconds: usize) -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.0; 16_000 * seconds],
    }
}

fn ok(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

async fn mount(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path(ENDPOINT))
        .respond_with(response)
        .mount(server)
        .await;
}

/// The JSON of the request's `config` part.
fn config_json(request: &Request) -> serde_json::Value {
    let body = String::from_utf8_lossy(&request.body);
    let at = body.find(r#"name="config""#).unwrap();
    let rest = &body[at..];
    let start = rest.find("\r\n\r\n").unwrap() + 4;
    let end = rest[start..].find("\r\n--").unwrap();
    serde_json::from_str(&rest[start..start + end]).unwrap()
}

async fn mount_fn(
    server: &MockServer,
    respond: impl Fn(&Request) -> ResponseTemplate + Send + Sync + 'static,
) {
    Mock::given(method("POST"))
        .and(path(ENDPOINT))
        .respond_with(respond)
        .mount(server)
        .await;
}

fn engine_for(server: &MockServer) -> AssemblyAi<RecordWait> {
    engine(config(server, None)).0
}

async fn count(server: &MockServer) -> usize {
    server.received_requests().await.unwrap().len()
}

#[tokio::test]
async fn success_pastes_the_rewrite_and_sends_the_documented_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ENDPOINT))
        // The bare key, no "Bearer".
        .and(header("authorization", KEY))
        .respond_with(ok(serde_json::json!({
            "text": "um so can we move the meeting to thursday",
            "llm_response": "Can we move the meeting to Thursday?",
            "llm_error": null,
        })))
        .mount(&server)
        .await;
    let (engine, _) = engine(config(&server, Some("de")));
    assert_eq!(
        engine.transcribe(&audio()).await.unwrap(),
        "Can we move the meeting to Thursday?"
    );

    let request = &server.received_requests().await.unwrap()[0];
    let body = String::from_utf8_lossy(&request.body);
    for expected in [
        r#"name="audio"; filename="audio.wav""#,
        "Content-Type: audio/wav",
        "RIFF",
        r#"name="config""#,
        "Content-Type: application/json",
    ] {
        assert!(body.contains(expected), "body lacks {expected:?}");
    }
    // Smart by default, with Smart's instruction.
    let config = config_json(request);
    assert_eq!(config["rewrite_mode"], "smart");
    assert_eq!(config["llm_instruction"], SMART_INSTRUCTION);
    assert_eq!(config["language_codes"], serde_json::json!(["de"]));
}

#[tokio::test]
async fn fast_sends_fast_s_instruction_and_no_rewrite_mode() {
    let server = MockServer::start().await;
    mount(
        &server,
        ok(serde_json::json!({"text": "hi", "llm_response": "Hi there."})),
    )
    .await;
    let fast = Config {
        model: FAST_MODEL.to_owned(),
        ..config(&server, None)
    };
    assert_eq!(
        engine(fast).0.transcribe(&audio()).await.unwrap(),
        "Hi there."
    );
    let config = config_json(&server.received_requests().await.unwrap()[0]);
    assert_eq!(config["llm_instruction"], FAST_INSTRUCTION);
    assert!(config.get("rewrite_mode").is_none(), "{config}");
    assert!(config.get("language_codes").is_none());
}

/// 400 for the key that nobody has verified: Smart is sent once more
/// without it, the request succeeds, and later ones skip it.
#[tokio::test]
async fn a_400_to_rewrite_mode_is_retried_once_without_it_and_remembered() {
    let server = MockServer::start().await;
    mount_fn(&server, |request: &Request| {
        if String::from_utf8_lossy(&request.body).contains("rewrite_mode") {
            ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "status": 400, "title": "Bad Request", "detail": "unknown field rewrite_mode",
            }))
        } else {
            ok(serde_json::json!({"text": "raw words", "llm_response": "Clean words here."}))
        }
    })
    .await;
    let (engine, wait) = engine(config(&server, None));
    assert_eq!(
        engine.transcribe(&audio()).await.unwrap(),
        "Clean words here."
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(config_json(&requests[0])["rewrite_mode"], "smart");
    let second = config_json(&requests[1]);
    assert!(second.get("rewrite_mode").is_none());
    // The instruction stays, so the rewrite is still the tuned one.
    assert_eq!(second["llm_instruction"], SMART_INSTRUCTION);
    // Not a retry of the failing kind: no wait in between.
    assert!(wait.waits().is_empty());
    // Remembered: the next dictation sends one request, without the key.
    assert_eq!(
        engine.transcribe(&audio()).await.unwrap(),
        "Clean words here."
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert!(config_json(&requests[2]).get("rewrite_mode").is_none());
}

#[tokio::test]
async fn a_400_that_is_not_about_rewrite_mode_is_an_error_and_is_not_remembered() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(400)).await;
    let (engine, _) = engine(config(&server, None));
    for round in 1..=2 {
        let error = engine.transcribe(&audio()).await.unwrap_err();
        assert!(
            matches!(&error, TranscribeError::Engine(m) if m.contains("HTTP 400")),
            "{error:?}"
        );
        // Smart, then the same request without the key; both refused.
        assert_eq!(count(&server).await, round * 2);
    }
    // Every Smart attempt still carried the key (nothing was remembered).
    let requests = server.received_requests().await.unwrap();
    assert_eq!(config_json(&requests[2])["rewrite_mode"], "smart");
}

#[tokio::test]
async fn fast_never_retries_a_400() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(400)).await;
    let fast = Config {
        model: FAST_MODEL.to_owned(),
        ..config(&server, None)
    };
    assert!(engine(fast).0.transcribe(&audio()).await.is_err());
    assert_eq!(count(&server).await, 1);
}

/// The rewrite model answers with its own instruction, or a bare ".", when
/// cleanup empties the speech: the verbatim text is pasted instead.
#[tokio::test]
async fn a_rewrite_that_is_blank_null_or_an_echo_pastes_the_verbatim_text() {
    let echo_all = SMART_INSTRUCTION.to_owned();
    let echo_sentence = "Collapse a stammered immediate repeat of a single word to one copy.";
    for (rewrite, why) in [
        (serde_json::Value::Null, "null"),
        (serde_json::json!(""), "blank"),
        (serde_json::json!("  \n"), "whitespace"),
        (serde_json::json!("."), "a bare full stop"),
        (serde_json::json!(echo_all), "the whole instruction"),
        (serde_json::json!(echo_sentence), "one instruction sentence"),
        (
            serde_json::json!(format!("Okay. {echo_sentence}")),
            "an instruction sentence after a word",
        ),
    ] {
        let server = MockServer::start().await;
        mount(
            &server,
            ok(serde_json::json!({"text": "Yeah.", "llm_response": rewrite})),
        )
        .await;
        let (engine, _) = engine(config(&server, None));
        assert_eq!(engine.transcribe(&audio()).await.unwrap(), "Yeah.", "{why}");
        // Not an error, not retried.
        assert_eq!(count(&server).await, 1, "{why}");
    }
}

#[tokio::test]
async fn a_real_rewrite_is_kept_and_silence_pastes_nothing_even_with_an_echo() {
    let server = MockServer::start().await;
    mount(
        &server,
        ok(serde_json::json!({"text": "um hey marcus so just letting you know", "llm_response": "Hey Marcus, just letting you know."})),
    )
    .await;
    let (engine, _) = engine(config(&server, None));
    assert_eq!(
        engine.transcribe(&audio()).await.unwrap(),
        "Hey Marcus, just letting you know."
    );
    let server = MockServer::start().await;
    mount(
        &server,
        ok(serde_json::json!({"text": "", "llm_response": SMART_INSTRUCTION})),
    )
    .await;
    assert_eq!(engine_for(&server).transcribe(&audio()).await.unwrap(), "");
}

#[tokio::test]
async fn a_missing_rewrite_falls_back_to_the_verbatim_text() {
    for reply in [
        serde_json::json!({"text": " hello there ", "llm_response": null, "llm_error": "timeout"}),
        serde_json::json!({"text": "hello there", "llm_response": "  ", "llm_error": null}),
        serde_json::json!({"text": "hello there"}),
    ] {
        let server = MockServer::start().await;
        mount(&server, ok(reply)).await;
        let (engine, _) = engine(config(&server, None));
        assert_eq!(engine.transcribe(&audio()).await.unwrap(), "hello there");
        // A failed rewrite is a successful request: one call, no retry.
        assert_eq!(count(&server).await, 1);
    }
}

#[tokio::test]
async fn a_language_the_api_does_not_know_is_left_out() {
    // "th" would be a 400 ("unknown language"); none at all is fine too.
    for language in [Some("th"), Some("EN"), None] {
        let server = MockServer::start().await;
        mount(&server, ok(serde_json::json!({"text": "hi"}))).await;
        let (engine, _) = engine(config(&server, language));
        engine.transcribe(&audio()).await.unwrap();
        let request = &server.received_requests().await.unwrap()[0];
        assert!(
            config_json(request).get("language_codes").is_none(),
            "{language:?}"
        );
    }
}

#[tokio::test]
async fn an_empty_transcript_is_empty_text_not_an_error() {
    for reply in [
        serde_json::json!({"text": "", "llm_response": "", "llm_error": null}),
        serde_json::json!({"text": "", "llm_response": null}),
        // Silence never pastes a rewrite's invention.
        serde_json::json!({"text": "", "llm_response": "Thank you."}),
    ] {
        let server = MockServer::start().await;
        mount(&server, ok(reply)).await;
        let (engine, _) = engine(config(&server, None));
        assert_eq!(engine.transcribe(&audio()).await.unwrap(), "");
        let detailed = engine.transcribe_detailed(&audio()).await.unwrap();
        assert_eq!(
            (detailed.text.as_str(), detailed.no_speech_prob),
            ("", None)
        );
    }
}

#[tokio::test]
async fn a_bad_key_is_invalid_and_not_retried() {
    let server = MockServer::start().await;
    mount(
        &server,
        ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "status": 401, "title": "Unauthorized", "detail": format!("bad key {KEY}"),
        })),
    )
    .await;
    let (engine, wait) = engine(config(&server, None));
    let error = engine.transcribe(&audio()).await.unwrap_err();
    assert_eq!(error, TranscribeError::InvalidKey);
    assert_eq!(count(&server).await, 1);
    assert!(wait.waits().is_empty());
    assert!(!format!("{error} {error:?}").contains(KEY));
}

#[tokio::test]
async fn a_rate_limit_is_retried_with_the_backoff_then_reported() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(429)).await;
    let (engine, wait) = engine(config(&server, None));
    assert_eq!(
        engine.transcribe(&audio()).await,
        Err(TranscribeError::RateLimited)
    );
    assert_eq!(count(&server).await, 3);
    assert_eq!(wait.waits(), [ms(100), ms(200)]);
}

#[tokio::test]
async fn retry_after_replaces_the_backoff_and_a_later_success_wins() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ENDPOINT))
        .respond_with(ResponseTemplate::new(503).insert_header("retry-after", "2"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount(&server, ok(serde_json::json!({"text": "back again"}))).await;
    let (engine, wait) = engine(config(&server, None));
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), "back again");
    assert_eq!(wait.waits(), [Duration::from_secs(2)]);
}

#[tokio::test]
async fn a_server_error_is_retried_then_an_engine_error() {
    for status in [500u16, 502, 504] {
        let server = MockServer::start().await;
        mount(&server, ResponseTemplate::new(status)).await;
        let (engine, _) = engine(config(&server, None));
        let error = engine.transcribe(&audio()).await.unwrap_err();
        assert!(
            matches!(&error, TranscribeError::Engine(m) if m.contains(&status.to_string())),
            "{status}: {error:?}"
        );
        assert_eq!(count(&server).await, 3, "HTTP {status}");
    }
}

#[tokio::test]
async fn a_clip_over_the_limit_is_a_clear_error_and_not_retried() {
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(413)).await;
    let (engine, _) = engine(config(&server, None));
    let error = engine.transcribe(&audio()).await.unwrap_err();
    assert!(
        matches!(&error, TranscribeError::Engine(m) if m.contains("longer than the provider accepts")),
        "{error:?}"
    );
    assert_eq!(count(&server).await, 1);
}

#[tokio::test]
async fn an_unreadable_reply_is_an_engine_error_without_the_body() {
    for body in [
        r#"{"text":"secret words""#.to_owned(),
        "not json".to_owned(),
        r#"{"text": 5}"#.to_owned(),
    ] {
        let server = MockServer::start().await;
        mount(&server, ResponseTemplate::new(200).set_body_string(body)).await;
        let (engine, _) = engine(config(&server, None));
        let error = engine.transcribe(&audio()).await.unwrap_err();
        assert!(matches!(error, TranscribeError::Engine(_)), "{error:?}");
        assert!(!error.to_string().contains("secret words"));
        assert_eq!(count(&server).await, 1, "an unreadable reply isn't retried");
    }
}

/// H1: the limit grows with the clip. A 4 s clip gets 400 ms + 2 s; the
/// server needs 1 s.
#[tokio::test]
async fn the_time_limit_grows_with_the_clip() {
    let server = MockServer::start().await;
    mount(
        &server,
        ok(serde_json::json!({"text": "a long dictation"})).set_delay(Duration::from_secs(1)),
    )
    .await;
    let base = Config {
        timeout: ms(400),
        retry: RetryPolicy {
            max_attempts: 2,
            base_delay: ms(1),
            max_delay: ms(1),
            jitter: 0.0,
            ..RetryPolicy::default()
        },
        ..config(&server, None)
    };
    // Control: a fixed limit fails the same request.
    let fixed = Config {
        timeout_grows_with_audio: false,
        ..base.clone()
    };
    assert_eq!(
        engine(fixed).0.transcribe(&clip(4)).await,
        Err(TranscribeError::Timeout)
    );
    let grows = Config {
        timeout_grows_with_audio: true,
        ..base
    };
    assert_eq!(
        engine(grows).0.transcribe(&clip(4)).await.unwrap(),
        "a long dictation"
    );
}

/// H1: a long clip that times out is retried once; a short one uses all
/// its attempts.
#[tokio::test]
async fn a_long_clip_that_times_out_is_retried_once() {
    for (long_clip, expected) in [(Duration::ZERO, 2), (Duration::from_secs(60), 3)] {
        let server = MockServer::start().await;
        mount(
            &server,
            ok(serde_json::json!({"text": "never"})).set_delay(Duration::from_secs(5)),
        )
        .await;
        let config = Config {
            timeout: ms(150),
            long_clip,
            retry: RetryPolicy {
                max_attempts: 3,
                base_delay: ms(1),
                max_delay: ms(1),
                jitter: 0.0,
                ..RetryPolicy::default()
            },
            ..config(&server, None)
        };
        assert_eq!(
            engine(config).0.transcribe(&clip(1)).await,
            Err(TranscribeError::Timeout)
        );
        assert_eq!(count(&server).await, expected, "long_clip {long_clip:?}");
    }
}

#[tokio::test]
async fn an_unreachable_server_is_a_network_error() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let config = Config {
        base_url: format!("http://127.0.0.1:{port}"),
        retry: RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        },
        ..Config::from_preset(ASSEMBLYAI)
    };
    let error = engine(config).0.transcribe(&audio()).await.unwrap_err();
    assert!(matches!(error, TranscribeError::Network(_)), "{error:?}");
    assert!(!error.to_string().contains(KEY));
}

#[test]
fn debug_redacts_the_key() {
    let engine = AssemblyAi::new(Config::from_preset(ASSEMBLYAI), Secret::new(KEY)).unwrap();
    let shown = format!("{engine:?}");
    assert!(shown.contains("redacted"), "{shown}");
    assert!(!shown.contains(KEY));
}

#[tokio::test]
async fn test_and_save_makes_one_real_call_with_a_quarter_second_of_silence() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(ENDPOINT))
        .and(header("authorization", KEY))
        .respond_with(ok(serde_json::json!({"text": "", "llm_response": null})))
        .expect(1)
        .mount(&server)
        .await;
    // Even with a dictation language and a long limit set.
    let dictation = config(&server, Some("de"));
    check_key_for(Provider::AssemblyAi, dictation, Secret::new(KEY))
        .await
        .unwrap();
    let request = &server.received_requests().await.unwrap()[0];
    // 4,000 samples of 16-bit PCM plus the 44-byte header, plus the config.
    assert!(request.body.len() > 8_044 && request.body.len() < 13_000);
    assert!(config_json(request).get("language_codes").is_none());
}

#[tokio::test]
async fn test_and_save_maps_each_failure_without_a_retry() {
    for (status, expected) in [
        (401, TranscribeError::InvalidKey),
        (403, TranscribeError::InvalidKey),
        (429, TranscribeError::RateLimited),
    ] {
        let server = MockServer::start().await;
        mount(&server, ResponseTemplate::new(status)).await;
        let result = check_key_for(
            Provider::AssemblyAi,
            config(&server, None),
            Secret::new(KEY),
        )
        .await;
        assert_eq!(result, Err(expected), "HTTP {status}");
        assert_eq!(count(&server).await, 1);
    }
    let server = MockServer::start().await;
    mount(&server, ResponseTemplate::new(503)).await;
    let result = check_key_for(
        Provider::AssemblyAi,
        config(&server, None),
        Secret::new(KEY),
    )
    .await;
    assert!(
        matches!(result, Err(TranscribeError::Engine(_))),
        "{result:?}"
    );
    assert_eq!(count(&server).await, 1);
}

#[tokio::test]
async fn the_app_engine_for_this_provider_dictates_through_the_settings() {
    let server = MockServer::start().await;
    mount(
        &server,
        ok(serde_json::json!({"text": "raw", "llm_response": "Clean text here."})),
    )
    .await;
    let settings = Settings {
        provider: Provider::AssemblyAi,
        model: Provider::AssemblyAi.default_model().to_owned(),
        language: Some("en".into()),
        ..Settings::default()
    };
    let engine = Engine::from_settings_at(&settings, Ok(Some(Secret::new(KEY))), &server.uri());
    assert_eq!(engine.name(), "remote");
    assert_eq!(
        engine.transcribe(&audio()).await.unwrap(),
        "Clean text here."
    );
    let none = Engine::from_settings(&settings, Ok(None));
    assert_eq!(
        none.transcribe(&audio()).await,
        Err(TranscribeError::MissingKey)
    );
}

/// A fresh directory under the system temp dir (no tempfile crate here).
fn scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vocet-dump-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[tokio::test]
async fn the_eval_dump_is_off_unless_asked_for_and_then_writes_nothing() {
    let server = MockServer::start().await;
    mount(
        &server,
        ok(serde_json::json!({"text": "hi", "llm_response": "Hi there."})),
    )
    .await;
    // Never set: nothing is created anywhere it could have been pointed.
    let dir = scratch_dir("off");
    let engine = engine_for(&server).with_dump_dir(None);
    engine.transcribe(&audio()).await.unwrap();
    assert!(!dir.exists());
}

#[tokio::test]
async fn the_eval_dump_appends_one_line_per_request_without_the_key() {
    let server = MockServer::start().await;
    mount_fn(&server, |request: &Request| {
        let body = String::from_utf8_lossy(&request.body);
        if body.contains("rewrite_mode") {
            return ResponseTemplate::new(400);
        }
        ok(serde_json::json!({
            "text": "um hello there",
            "llm_response": "Hello there.",
            "llm_error": null,
            "session_id": "11111111-2222-3333-4444-555555555555",
            "audio_duration_ms": 1500,
            "request_time_ms": 812.4,
            "sync_time_ms": 430.1,
        }))
    })
    .await;
    let dir = scratch_dir("on");
    let engine = engine_for(&server).with_dump_dir(Some(dir.clone()));
    // Smart is refused (400) and retried without the key: two requests,
    // two lines. A second dictation adds a third.
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), "Hello there.");
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), "Hello there.");
    let file =
        std::fs::read_to_string(dir.join(dictation_transcribe::assemblyai::DUMP_FILE)).unwrap();
    let lines: Vec<serde_json::Value> = file
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 3, "{file}");

    // The refused Smart request: what was sent and the status only.
    assert_eq!(lines[0]["http_status"], 400);
    assert_eq!(lines[0]["rewrite_mode_sent"], "smart");
    assert_eq!(lines[0]["instruction"], "smart");
    assert!(lines[0].get("verbatim_text").is_none());

    // The request that worked.
    let ok = &lines[1];
    assert_eq!(ok["http_status"], 200);
    assert!(ok["rewrite_mode_sent"].is_null());
    assert_eq!(ok["instruction"], "smart", "the name, never its text");
    assert_eq!(ok["verbatim_text"], "um hello there");
    assert_eq!(ok["llm_response"], "Hello there.");
    assert!(ok["llm_error"].is_null());
    assert_eq!(ok["pasted"], "rewrite");
    assert_eq!(ok["pasted_reason"], "rewrite");
    assert_eq!(ok["pasted_text"], "Hello there.");
    assert_eq!(ok["session_id"], "11111111-2222-3333-4444-555555555555");
    assert_eq!(ok["audio_duration_ms"], 1500);
    assert_eq!(ok["request_time_ms"], 812.4);
    assert_eq!(ok["sync_time_ms"], 430.1);
    let ts = ok["ts"].as_str().unwrap();
    assert!(
        ts.len() == 24 && ts.ends_with('Z') && ts.contains('T'),
        "{ts}"
    );

    // Neither the key nor any header, and no instruction text.
    assert!(!file.contains(KEY));
    assert!(!file.to_lowercase().contains("authorization"));
    assert!(!file.contains("Clean it by removing disfluencies"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_eval_dump_says_why_the_verbatim_text_was_pasted() {
    let server = MockServer::start().await;
    mount(
        &server,
        ok(serde_json::json!({"text": "Yeah.", "llm_response": null, "llm_error": "timeout"})),
    )
    .await;
    let dir = scratch_dir("why");
    let fast = Config {
        model: FAST_MODEL.to_owned(),
        ..config(&server, None)
    };
    let engine = engine(fast).0.with_dump_dir(Some(dir.clone()));
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), "Yeah.");
    let file =
        std::fs::read_to_string(dir.join(dictation_transcribe::assemblyai::DUMP_FILE)).unwrap();
    let line: serde_json::Value = serde_json::from_str(file.trim()).unwrap();
    assert_eq!(line["instruction"], "fast");
    assert!(line["rewrite_mode_sent"].is_null());
    assert_eq!(line["pasted"], "verbatim");
    assert_eq!(line["pasted_reason"], "no_rewrite");
    assert_eq!(line["llm_error"], "timeout");
    let _ = std::fs::remove_dir_all(&dir);
}
