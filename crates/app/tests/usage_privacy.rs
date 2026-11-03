//! A1 criterion 3: secrets and text planted in every pipeline input never
//! appear in a usage event, and no event holds a key-shaped string.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail the test on purpose"
)]

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dictation_core::coordinator::Phase::Idle;
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::usage::UsageTracker;
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, FakeTelemetry, FileAudioSource,
};
use dictation_ports::{Audio, EngineKind, HotkeyEvent, InsertError, TranscribeError, Transcriber};

const WAIT: Duration = Duration::from_secs(5);

/// Words, keys, an app name and a path, planted everywhere they could enter.
const TRANSCRIPT: &str = "my password is hunter2-TRANSCRIPT-SECRET";
const API_KEY: &str = "gsk_AbCdEf0123456789AbCdEf0123456789XYZ";
const APP_NAME: &str = "com.apple.Safari";
const PATH: &str = "/Users/someone/Documents/secret-plans.txt";
const HOST: &str = "someones-macbook.local";

const PLANTED: [&str; 7] = [
    "hunter2",
    "TRANSCRIPT-SECRET",
    "gsk_",
    "AbCdEf0123456789",
    "Safari",
    "/Users/someone",
    "someones-macbook",
];

struct Scripted(std::sync::Mutex<Option<Result<String, TranscribeError>>>);

impl Transcriber for Scripted {
    fn transcribe(
        &self,
        _audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
        let result = self.0.lock().unwrap().take().expect("one call");
        async move { result }
    }
}

fn wav() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "dictation-usage-privacy-{}.wav",
        std::process::id()
    ));
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for i in 0..4_800 {
        writer
            .write_sample(if i % 2 == 0 { 16_384i16 } else { -16_384 })
            .unwrap();
    }
    writer.finalize().unwrap();
    path
}

/// One dictation through the real pipeline; its UI events go to `tracker`.
async fn dictate(
    result: Result<String, TranscribeError>,
    insert_failure: Option<InsertError>,
    tracker: &mut UsageTracker,
) {
    let path = wav();
    let hotkey = FakeHotkey::new();
    let clock = FakeClock::at(Duration::from_secs(100));
    let inserter = FakeInserter::new();
    if let Some(error) = insert_failure {
        inserter.fail_next(error);
    }
    let ports = Ports {
        hotkey: hotkey.clone(),
        audio: FileAudioSource::new(path.clone()),
        transcriber: Scripted(std::sync::Mutex::new(Some(result))),
        inserter,
        permissions: FakePermissions::all_granted(),
        clock: clock.clone(),
    };
    let (pipeline, mut ui) = Pipeline::spawn(ports, Config::default()).unwrap();
    let start = Instant::now();
    assert!(hotkey.emit(HotkeyEvent::Pressed));
    clock.advance(Duration::from_millis(500));
    assert!(hotkey.emit(HotkeyEvent::Released));
    loop {
        let event = tokio::time::timeout(WAIT, ui.recv())
            .await
            .expect("no UI event in time")
            .expect("pipeline stopped");
        tracker.on_event(&event, start.elapsed());
        if event == UiEvent::StateChanged(Idle) {
            break;
        }
    }
    pipeline.shutdown().await;
    let _ = std::fs::remove_file(path);
}

/// A run of 24 or more key-like characters.
fn key_shaped(text: &str) -> Option<&str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .find(|run| run.len() >= 24)
}

#[tokio::test]
async fn no_event_holds_a_transcript_key_app_name_path_or_host() {
    let telemetry = FakeTelemetry::new();
    let mut tracker = UsageTracker::new(Arc::new(telemetry.clone()), EngineKind::Cloud);

    // Pasted: the transcript is the secret.
    dictate(Ok(TRANSCRIPT.into()), None, &mut tracker).await;
    // The provider's reply echoes the key, the transcript and a path.
    dictate(
        Err(TranscribeError::Engine(format!(
            "401 for key {API_KEY} on {HOST}: {TRANSCRIPT} in {PATH}"
        ))),
        None,
        &mut tracker,
    )
    .await;
    // The paste fails, and the reason names the app, the text and a path.
    dictate(
        Ok(TRANSCRIPT.into()),
        Some(InsertError::Failed(format!(
            "{APP_NAME} refused {TRANSCRIPT} at {PATH}"
        ))),
        &mut tracker,
    )
    .await;
    dictate(
        Ok(TRANSCRIPT.into()),
        Some(InsertError::SecureInput),
        &mut tracker,
    )
    .await;

    let events = telemetry.events();
    assert_eq!(events.len(), 4, "{events:?}");
    let wire = serde_json::to_string(&events).unwrap();
    for planted in PLANTED {
        assert!(!wire.contains(planted), "{planted} leaked into {wire}");
    }
    assert_eq!(key_shaped(&wire), None, "{wire}");
    for line in [&wire, &format!("{events:?}")] {
        assert!(!line.contains("hunter2"), "{line}");
    }
}

#[test]
fn the_key_shape_check_catches_a_key() {
    assert!(key_shaped(API_KEY).is_some());
    assert!(key_shaped("{\"latency_bucket\":\"from500_to1000ms\"}").is_none());
}
