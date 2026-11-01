//! M4.4 Tester: the key reaches no log line and no user message through the
//! engine layer (`Engine`, `Transcribers`, `update`), on success and on every
//! provider error, including a provider body that echoes the key.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::io::Write;
use std::sync::{Arc, Mutex};

use dictation_core::settings::Settings;
use dictation_ports::{Audio, Secret, SecretError, Transcriber};
use dictation_transcribe::engine::{Engine, Transcribers};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "gsk_leakcheck-NOT-REAL-77aa";
const KEY2: &str = "gsk_leakcheck2-NOT-REAL-88bb";
const TRANSCRIPT: &str = "zebra marmalade transcript";

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn audio() -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: vec![0.1; 1_600],
    }
}

fn built(server: &MockServer, key: &str) -> Engine {
    Engine::from_settings_at(
        &Settings::default(),
        Ok(Some(Secret::new(key))),
        &server.uri(),
    )
}

#[tokio::test]
async fn the_key_and_transcript_stay_out_of_logs_and_messages_through_transcribers() {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(captured.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let echo = serde_json::json!({"error": {"message": format!("bad key {KEY} / {KEY2}")}});
    let responses = vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!({ "text": TRANSCRIPT })),
        ResponseTemplate::new(401).set_body_json(echo.clone()),
        ResponseTemplate::new(429)
            .insert_header("retry-after", "0")
            .set_body_json(echo.clone()),
        ResponseTemplate::new(503)
            .insert_header("retry-after", "0")
            .set_body_json(echo.clone()),
        ResponseTemplate::new(400).set_body_json(echo),
    ];
    let mut shown = String::new();
    for response in responses {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(response)
            .mount(&server)
            .await;
        let transcribers = Transcribers::new(built(&server, KEY));
        transcribers.update(|| built(&server, KEY2));
        match transcribers.transcribe(&audio()).await {
            Ok(_) => {}
            Err(error) => {
                shown.push_str(&format!("{error} | {error:?} | {}\n", error.user_message()));
            }
        }
        shown.push_str(&format!("{transcribers:?}\n"));
    }
    // An unreadable keychain and a missing key, too.
    for key in [Err(SecretError::Store("keychain locked".into())), Ok(None)] {
        let unavailable = Transcribers::new(Engine::from_settings(&Settings::default(), key));
        let error = unavailable.transcribe(&audio()).await.unwrap_err();
        shown.push_str(&format!("{error} | {error:?} | {}\n", error.user_message()));
    }

    let logs = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    for (what, text) in [("logs", &logs), ("messages", &shown)] {
        assert!(
            !text.contains("gsk_"),
            "a key leaked into the {what}: {text}"
        );
        assert!(
            !text.contains("zebra"),
            "the transcript leaked into the {what}"
        );
    }
}
