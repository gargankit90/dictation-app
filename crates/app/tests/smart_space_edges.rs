//! Tester edge cases for M5.5a (#93) that `smart_space.rs` leaves out: a
//! reader that panics, and a hung read followed by a healthy one.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test on purpose"
)]

use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dictation_core::coordinator::Phase::Idle;
use dictation_core::text::Transcript;
use dictation_pipeline::pipeline::{Config, CursorReader, Pipeline, Ports, UiEvent};
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, FakeTextBeforeCursor, FileAudioSource,
};
use dictation_ports::{
    Audio, BeforeCursor, CursorReadError, HotkeyEvent, TextBeforeCursor, TranscribeError,
    Transcriber,
};

const WAIT: Duration = Duration::from_secs(5);

/// Answers `text`, or fails.
struct Answering(Result<Transcript, TranscribeError>);

impl Answering {
    fn text(text: &str) -> Self {
        Self(Ok(Transcript::from_text(text)))
    }
}

impl Transcriber for Answering {
    fn transcribe(
        &self,
        _audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
        let result = self.0.clone().map(|transcript| transcript.text);
        async move { result }
    }

    fn transcribe_detailed(
        &self,
        _audio: &Audio,
    ) -> impl Future<Output = Result<Transcript, TranscribeError>> + Send {
        let result = self.0.clone();
        async move { result }
    }
}

struct TempWav(PathBuf);

impl TempWav {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dictation-smart-space-{}-{}.wav",
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
        // 300 ms of loud square wave: reaches the transcriber.
        for i in 0..4_800 {
            writer
                .write_sample(if i % 2 == 0 { 16_384i16 } else { -16_384 })
                .unwrap();
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

/// One dictation of `transcriber` with `cursor` reading through `reader`;
/// what was inserted.
async fn dictate_with(transcriber: Answering, cursor: CursorReader) -> Vec<String> {
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
    let (pipeline, mut ui) = Pipeline::spawn_with_cursor(ports, cursor, Config::default()).unwrap();
    assert!(hotkey.emit(HotkeyEvent::Pressed));
    clock.advance(Duration::from_millis(500));
    assert!(hotkey.emit(HotkeyEvent::Released));
    loop {
        let event = tokio::time::timeout(WAIT, ui.recv())
            .await
            .expect("no UI event in time")
            .expect("pipeline stopped");
        if event == UiEvent::StateChanged(Idle) {
            break;
        }
    }
    pipeline.shutdown().await;
    inserter.inserted()
}

/// A reader that panics, as a buggy adapter might.
struct Panicking;

impl TextBeforeCursor for Panicking {
    fn char_before_cursor(&self) -> Result<BeforeCursor, CursorReadError> {
        panic!("adapter bug");
    }
}

#[tokio::test]
async fn a_reader_that_panics_gives_no_space_and_the_paste_still_happens() {
    let inserted = dictate_with(Answering::text("world"), CursorReader::new(Panicking)).await;
    assert_eq!(inserted, ["world"]);
}

#[tokio::test]
async fn a_hung_read_does_not_spoil_the_next_dictation() {
    let reader = FakeTextBeforeCursor::char('o');
    reader.delay(Duration::from_millis(600));
    let slow = CursorReader::new(reader.clone()).with_timeout(Duration::from_millis(30));
    assert_eq!(
        dictate_with(Answering::text("world"), slow).await,
        ["world"]
    );
    // The hung app recovers; the next dictation reads again and gets its space.
    reader.delay(Duration::ZERO);
    let healthy = CursorReader::new(reader.clone());
    assert_eq!(
        dictate_with(Answering::text("world"), healthy).await,
        [" world"]
    );
    assert_eq!(reader.reads(), 2);
}
