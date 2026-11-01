//! M5.5a criteria 1 and 2 (#93): the smart leading space end to end through
//! the pipeline, with a fake "text before cursor" port. A space goes in
//! front of the transcript after a letter or punctuation, never after a
//! space, a newline or an opening bracket, at the start of a field, or when
//! the read fails, times out or meets a password field. What the port read
//! never reaches the log, at any level.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail the test on purpose"
)]

use std::future::Future;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use dictation_core::coordinator::Phase::Idle;
use dictation_core::text::Transcript;
use dictation_pipeline::pipeline::{
    CURSOR_READ_TIMEOUT, Config, CursorReader, Pipeline, Ports, UiEvent,
};
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, FakeTextBeforeCursor, FileAudioSource,
};
use dictation_ports::{
    Audio, BeforeCursor, CursorReadError, HotkeyEvent, TranscribeError, Transcriber,
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

/// "world" dictated with `reader` answering.
async fn dictate_world(reader: &FakeTextBeforeCursor) -> Vec<String> {
    dictate_with(Answering::text("world"), CursorReader::new(reader.clone())).await
}

#[tokio::test]
async fn a_space_is_added_after_a_letter_or_punctuation() {
    for before in ['o', 'Z', '9', '.', ',', '!', '?', ':', ')', '"', '\u{e9}'] {
        let reader = FakeTextBeforeCursor::char(before);
        assert_eq!(dictate_world(&reader).await, [" world"], "after {before:?}");
        assert_eq!(reader.reads(), 1, "read once per dictation");
    }
}

#[tokio::test]
async fn no_space_after_a_space_a_newline_or_an_opening_bracket() {
    for before in [
        ' ', '\n', '\t', '\r', '\u{a0}', '(', '[', '{', '\u{201c}', '\u{2018}',
    ] {
        let reader = FakeTextBeforeCursor::char(before);
        assert_eq!(dictate_world(&reader).await, ["world"], "after {before:?}");
    }
}

#[tokio::test]
async fn no_space_at_the_start_of_a_field() {
    let reader = FakeTextBeforeCursor::answering(Ok(BeforeCursor::StartOfField));
    assert_eq!(dictate_world(&reader).await, ["world"]);
    assert_eq!(reader.reads(), 1);
}

#[tokio::test]
async fn no_space_when_the_port_fails_or_meets_a_password_field() {
    for error in [
        CursorReadError::Unavailable,
        CursorReadError::SecureField,
        CursorReadError::PermissionDenied,
        CursorReadError::TimedOut,
        CursorReadError::Failed("AXError -25200".into()),
    ] {
        let reader = FakeTextBeforeCursor::answering(Err(error.clone()));
        assert_eq!(dictate_world(&reader).await, ["world"], "{error:?}");
    }
}

#[tokio::test]
async fn no_space_and_no_long_wait_when_the_port_times_out() {
    let reader = FakeTextBeforeCursor::char('o');
    reader.delay(Duration::from_millis(800));
    let started = Instant::now();
    let inserted = dictate_with(
        Answering::text("world"),
        CursorReader::new(reader.clone()).with_timeout(Duration::from_millis(50)),
    )
    .await;
    assert_eq!(inserted, ["world"]);
    assert!(
        started.elapsed() < Duration::from_millis(600),
        "the paste waited {:?} for a hung app",
        started.elapsed()
    );
}

#[test]
fn the_default_timeout_is_short() {
    assert!(CURSOR_READ_TIMEOUT <= Duration::from_millis(250));
}

#[tokio::test]
async fn nothing_is_read_when_there_is_nothing_to_paste() {
    let reader = FakeTextBeforeCursor::char('o');
    // A dropped transcript (blank).
    assert!(
        dictate_with(Answering::text("  "), CursorReader::new(reader.clone()))
            .await
            .is_empty()
    );
    // A failed transcription.
    assert!(
        dictate_with(
            Answering(Err(TranscribeError::Timeout)),
            CursorReader::new(reader.clone())
        )
        .await
        .is_empty()
    );
    assert_eq!(reader.reads(), 0);
}

#[tokio::test]
async fn the_transcript_is_still_trimmed_before_the_space() {
    let reader = FakeTextBeforeCursor::char('o');
    let inserted = dictate_with(
        Answering::text("  world \n"),
        CursorReader::new(reader.clone()),
    )
    .await;
    assert_eq!(inserted, [" world"]);
}

/// Every log line of this test binary, at every level, from every thread.
#[derive(Clone, Default)]
struct LogBuffer(Arc<Mutex<Vec<u8>>>);

impl Write for LogBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn global_log() -> &'static LogBuffer {
    static LOG: OnceLock<LogBuffer> = OnceLock::new();
    LOG.get_or_init(|| {
        let buffer = LogBuffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::set_global_default(subscriber).unwrap();
        buffer
    })
}

/// Criterion 2: a unique sentinel read from the "focused app" never reaches
/// the log, at any level, in any form; the dictation that used it is logged
/// (so the capture works).
#[tokio::test]
async fn the_text_read_before_the_cursor_never_reaches_any_log_level() {
    const SENTINEL: char = '\u{1d516}';
    let log = global_log();
    let reader = FakeTextBeforeCursor::char(SENTINEL);
    assert_eq!(dictate_world(&reader).await, [" world"]);
    // And the failure paths, which log the reason.
    reader.set(Err(CursorReadError::SecureField));
    assert_eq!(dictate_world(&reader).await, ["world"]);
    reader.set(Ok(BeforeCursor::Char(SENTINEL)));
    reader.delay(Duration::from_millis(500));
    let slow = dictate_with(
        Answering::text("world"),
        CursorReader::new(reader.clone()).with_timeout(Duration::from_millis(20)),
    )
    .await;
    assert_eq!(slow, ["world"]);

    let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("step"), "trace output was captured:\n{text}");
    assert!(text.contains("TRACE") || text.contains("DEBUG"), "{text}");
    for form in [
        SENTINEL.to_string(),
        format!("{SENTINEL:?}"),
        SENTINEL.escape_unicode().to_string(),
        "1d516".to_owned(),
        "1D516".to_owned(),
        u32::from(SENTINEL).to_string(),
    ] {
        assert!(!text.contains(&form), "{form:?} is in the log:\n{text}");
    }
}
