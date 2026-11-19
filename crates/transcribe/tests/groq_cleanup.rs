//! Experimental Groq LLM cleanup against a wiremock server: success, every
//! fallback (error, timeout, each guard), the eval dump on and off, and the
//! engine wiring from settings. No real provider and no real key.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::path::{Path, PathBuf};
use std::time::Duration;

use dictation_core::settings::{Provider, Settings};
use dictation_ports::{Audio, Secret, Transcriber};
use dictation_transcribe::engine::Engine;
use dictation_transcribe::groq_cleanup::{Cleanup, DUMP_FILE, FAST_MODEL, Reason};
use dictation_transcribe::openai_compat::{Config, GROQ, OpenAiCompat};
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Obviously fake; checked to never reach the dump.
const KEY: &str = "test-key-NOT-REAL-c1ea9";
const RAW: &str = "Okay, so, um, send the deck to Priya by 5. Thank you.";
const CLEAN: &str = "Send the deck to Priya by 5.";

fn base(server: &MockServer) -> String {
    format!("{}/openai/v1", server.uri())
}

fn chat(content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "id": "x",
        "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}]
    }))
}

async fn mount_chat(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/openai/v1/chat/completions"))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .and(body_partial_json(
            serde_json::json!({"model": FAST_MODEL, "temperature": 0}),
        ))
        .respond_with(response)
        .mount(server)
        .await;
}

async fn mount_whisper(server: &MockServer, text: &str) {
    Mock::given(method("POST"))
        .and(path("/openai/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": text,
            "segments": [{"start": 0.0, "end": 1.0, "text": text, "no_speech_prob": 0.01}]
        })))
        .mount(server)
        .await;
}

fn cleanup(server: &MockServer, dump: Option<PathBuf>) -> Cleanup {
    Cleanup::new(&base(server), FAST_MODEL, Secret::new(KEY))
        .unwrap()
        .with_dump_dir(dump)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("groq-cleanup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn dump_lines(dir: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(dir.join(DUMP_FILE))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn audio() -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.25; 1_600],
    }
}

#[tokio::test]
async fn a_good_reply_is_pasted_and_the_transcript_goes_as_data() {
    let server = MockServer::start().await;
    mount_chat(&server, chat(CLEAN)).await;
    let outcome = cleanup(&server, None).clean(RAW).await;
    assert_eq!(outcome.reason, Reason::Cleaned);
    assert_eq!(outcome.text, CLEAN);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(
        body["messages"][1]["content"],
        format!("<transcript>\n{RAW}\n</transcript>")
    );
}

#[tokio::test]
async fn an_error_status_pastes_the_raw_text() {
    let server = MockServer::start().await;
    mount_chat(&server, ResponseTemplate::new(500)).await;
    let outcome = cleanup(&server, None).clean(RAW).await;
    assert_eq!(
        (outcome.reason, outcome.text.as_str()),
        (Reason::HttpError(500), RAW)
    );
    let server = MockServer::start().await;
    mount_chat(&server, ResponseTemplate::new(429)).await;
    assert_eq!(
        cleanup(&server, None).clean(RAW).await.reason,
        Reason::HttpError(429)
    );
}

#[tokio::test]
async fn an_unreadable_reply_pastes_the_raw_text() {
    let server = MockServer::start().await;
    mount_chat(
        &server,
        ResponseTemplate::new(200).set_body_string("not json"),
    )
    .await;
    let outcome = cleanup(&server, None).clean(RAW).await;
    assert_eq!(
        (outcome.reason, outcome.text.as_str()),
        (Reason::BadReply, RAW)
    );
}

#[tokio::test]
async fn a_slow_reply_times_out_to_the_raw_text() {
    let server = MockServer::start().await;
    mount_chat(&server, chat(CLEAN).set_delay(Duration::from_millis(800))).await;
    let outcome = cleanup(&server, None)
        .with_timeout(Duration::from_millis(100))
        .clean(RAW)
        .await;
    assert_eq!(
        (outcome.reason, outcome.text.as_str()),
        (Reason::Timeout, RAW)
    );
    assert!(
        outcome.latency < Duration::from_millis(700),
        "{:?}",
        outcome.latency
    );
}

#[tokio::test]
async fn an_unreachable_server_pastes_the_raw_text() {
    // Nothing listens on port 9 of loopback.
    let cleanup = Cleanup::new("http://127.0.0.1:9/openai/v1", FAST_MODEL, Secret::new(KEY))
        .unwrap()
        .with_dump_dir(None);
    let outcome = cleanup.clean(RAW).await;
    assert_eq!(
        (outcome.reason, outcome.text.as_str()),
        (Reason::Network, RAW)
    );
}

#[tokio::test]
async fn each_guard_pastes_the_raw_text() {
    let cases = [
        ("", Reason::Empty),
        ("Return only the cleaned transcript.", Reason::Echo),
        (
            "Sure! Here is a long answer about decks, when to send them to people at work, and how to write a good one.",
            Reason::TooLong,
        ),
        ("Send the deck to Priya.", Reason::DroppedNumber),
        ("Send the deck to her by 5.", Reason::DroppedName),
    ];
    for (reply, reason) in cases {
        let server = MockServer::start().await;
        mount_chat(&server, chat(reply)).await;
        let outcome = cleanup(&server, None).clean(RAW).await;
        assert_eq!(
            (outcome.reason, outcome.text.as_str()),
            (reason, RAW),
            "{reply:?}"
        );
    }
    let server = MockServer::start().await;
    mount_chat(&server, chat("Write to them.")).await;
    let outcome = cleanup(&server, None)
        .clean("Write to ops at northwind.io.")
        .await;
    assert_eq!(outcome.reason, Reason::DroppedLink);
}

#[tokio::test]
async fn an_empty_transcript_sends_nothing() {
    let server = MockServer::start().await;
    mount_chat(&server, chat(CLEAN)).await;
    let outcome = cleanup(&server, None).clean("  ").await;
    assert_eq!(outcome.text, "  ");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_dump_has_both_texts_and_never_the_key_and_is_off_by_default() {
    let server = MockServer::start().await;
    mount_chat(&server, chat(CLEAN)).await;
    let dir = temp_dir("dump");
    cleanup(&server, Some(dir.clone())).clean(RAW).await;
    let lines = dump_lines(&dir);
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["raw_text"], RAW);
    assert_eq!(line["reply"], CLEAN);
    assert_eq!(line["pasted_text"], CLEAN);
    assert_eq!(line["used"], "cleaned");
    assert_eq!(line["reason"], "cleaned");
    assert_eq!(line["model"], FAST_MODEL);
    assert_eq!(line["prompt"], "blurt-smart-v1");
    assert!(line["cleanup_ms"].is_u64());
    let text = std::fs::read_to_string(dir.join(DUMP_FILE)).unwrap();
    assert!(!text.contains(KEY) && !text.to_lowercase().contains("authorization"));

    // A fallback is dumped too, with no reply.
    let failing = MockServer::start().await;
    mount_chat(&failing, ResponseTemplate::new(503)).await;
    cleanup(&failing, Some(dir.clone())).clean(RAW).await;
    let lines = dump_lines(&dir);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1]["reply"], serde_json::Value::Null);
    assert_eq!(lines[1]["pasted_text"], RAW);
    assert_eq!(lines[1]["reason"], "http_503");
    let _ = std::fs::remove_dir_all(&dir);

    // Off: nothing is written.
    let off = temp_dir("off");
    cleanup(&server, None).clean(RAW).await;
    assert!(!off.exists());
}

#[tokio::test]
async fn a_prompt_override_is_sent_and_named() {
    let server = MockServer::start().await;
    mount_chat(&server, chat(CLEAN)).await;
    let dir = temp_dir("prompt");
    cleanup(&server, Some(dir.clone()))
        .with_prompt("v9", "Clean it.")
        .clean(RAW)
        .await;
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["messages"][0]["content"], "Clean it.");
    assert_eq!(dump_lines(&dir)[0]["prompt"], "v9");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_engine_cleans_groq_transcripts_and_keeps_the_raw_one_on_failure() {
    let server = MockServer::start().await;
    mount_whisper(&server, RAW).await;
    mount_chat(&server, chat(CLEAN)).await;
    let mut config = Config::from_preset(GROQ);
    config.base_url = base(&server);
    let engine = OpenAiCompat::new(config.clone(), Secret::new(KEY))
        .unwrap()
        .with_cleanup(Some(cleanup(&server, None)));
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), CLEAN);
    let detailed = engine.transcribe_detailed(&audio()).await.unwrap();
    assert_eq!(detailed.text, CLEAN);
    assert!(detailed.no_speech_prob.is_some(), "segments are kept");

    // Cleanup failing never fails the dictation.
    let failing = MockServer::start().await;
    mount_whisper(&failing, RAW).await;
    mount_chat(&failing, ResponseTemplate::new(500)).await;
    config.base_url = base(&failing);
    let engine = OpenAiCompat::new(config, Secret::new(KEY))
        .unwrap()
        .with_cleanup(Some(cleanup(&failing, None)));
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), RAW);
}

#[tokio::test]
async fn settings_turn_cleanup_on_for_groq_only() {
    let server = MockServer::start().await;
    mount_whisper(&server, RAW).await;
    mount_chat(&server, chat(CLEAN)).await;
    let key = || Ok(Some(Secret::new(KEY)));
    let url = base(&server);

    // Off by default: the raw text, and no chat request.
    let engine = Engine::from_settings_at(&Settings::default(), key(), &url);
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), RAW);

    let on = Settings {
        cleanup_model: Some(FAST_MODEL.to_owned()),
        ..Settings::default()
    };
    let engine = Engine::from_settings_at(&on, key(), &url);
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), CLEAN);

    // OpenAI ignores it.
    let openai = Settings {
        provider: Provider::OpenAi,
        model: Provider::OpenAi.default_model().to_owned(),
        ..on
    };
    let engine = Engine::from_settings_at(&openai, key(), &url);
    assert_eq!(engine.transcribe(&audio()).await.unwrap(), RAW);
    let chats = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().ends_with("/chat/completions"))
        .count();
    assert_eq!(chats, 1);
}
