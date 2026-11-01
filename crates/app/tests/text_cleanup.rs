//! M4.2: the Whisper-on-silence filter end to end through the pipeline.
//! A transcriber that reports `no_speech_prob` answers with a stock phrase;
//! it is dropped only when the probability says there was no speech.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail the test on purpose"
)]

use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dictation_core::coordinator::Phase::{Idle, Recording, Transcribing};
use dictation_core::text::Transcript;
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, FileAudioSource,
};
use dictation_ports::{Audio, HotkeyEvent, TranscribeError, Transcriber};

const WAIT: Duration = Duration::from_secs(5);

/// Answers with `text` and `no_speech_prob` through `transcribe_detailed`.
struct Reporting {
    text: &'static str,
    no_speech_prob: Option<f64>,
}

impl Transcriber for Reporting {
    fn transcribe(
        &self,
        _audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
        let text = self.text.to_owned();
        async move { Ok(text) }
    }

    fn transcribe_detailed(
        &self,
        _audio: &Audio,
    ) -> impl Future<Output = Result<Transcript, TranscribeError>> + Send {
        let transcript = Transcript {
            text: self.text.to_owned(),
            no_speech_prob: self.no_speech_prob,
        };
        async move { Ok(transcript) }
    }
}

struct TempWav(PathBuf);

impl TempWav {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dictation-text-cleanup-{}-{}.wav",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for sample in [0i16, 16_384, -16_384] {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        Self(path)
    }
}

impl Drop for TempWav {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// One dictation with `transcriber`: the UI events up to Idle, and what was
/// inserted.
async fn dictate(transcriber: Reporting) -> (Vec<UiEvent>, Vec<String>) {
    let wav = TempWav::new();
    let hotkey = FakeHotkey::new();
    let clock = FakeClock::at(Duration::from_secs(100));
    let inserter = FakeInserter::new();
    let ports = Ports {
        hotkey: hotkey.clone(),
        audio: FileAudioSource::new(wav.0.clone()),
        transcriber,
        inserter: inserter.clone(),
        permissions: FakePermissions::all_granted(),
        clock: clock.clone(),
    };
    let (pipeline, mut ui) = Pipeline::spawn(ports, Config::default()).unwrap();
    assert!(hotkey.emit(HotkeyEvent::Pressed));
    clock.advance(Duration::from_millis(500));
    assert!(hotkey.emit(HotkeyEvent::Released));

    let mut events = Vec::new();
    loop {
        let event = tokio::time::timeout(WAIT, ui.recv())
            .await
            .expect("no UI event in time")
            .expect("pipeline stopped");
        let idle = event == UiEvent::StateChanged(Idle);
        events.push(event);
        if idle {
            break;
        }
    }
    pipeline.shutdown().await;
    assert_eq!(ui.recv().await, None, "unexpected extra UI event");
    (events, inserter.inserted())
}

#[tokio::test]
async fn a_stock_phrase_with_a_high_no_speech_prob_is_dropped_silently() {
    let (events, inserted) = dictate(Reporting {
        text: "Thank you.",
        no_speech_prob: Some(0.92),
    })
    .await;
    assert_eq!(
        events,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::StateChanged(Idle),
        ]
    );
    assert!(inserted.is_empty());
}

#[tokio::test]
async fn the_same_phrase_with_a_low_no_speech_prob_is_pasted() {
    let (_, inserted) = dictate(Reporting {
        text: "Thank you.",
        no_speech_prob: Some(0.04),
    })
    .await;
    assert_eq!(inserted, ["Thank you."]);
}

#[tokio::test]
async fn the_phrase_is_pasted_when_the_engine_reports_no_probability() {
    let (_, inserted) = dictate(Reporting {
        text: " you ",
        no_speech_prob: None,
    })
    .await;
    assert_eq!(inserted, ["you"]);
}

#[tokio::test]
async fn other_text_is_pasted_even_when_the_engine_thinks_it_was_silent() {
    let (_, inserted) = dictate(Reporting {
        text: "Thank you for the update.",
        no_speech_prob: Some(0.95),
    })
    .await;
    assert_eq!(inserted, ["Thank you for the update."]);
}
