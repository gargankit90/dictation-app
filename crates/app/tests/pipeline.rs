//! M1.3: the pipeline against the M1.2 fakes. Time comes from `FakeClock`;
//! the only waits are timeouts on the UI channel, so a hang fails instead
//! of blocking.

// clippy.toml allows these in #[test] fns but not in the helpers they share.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail the test on purpose"
)]

use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_core::coordinator::Phase::{Idle, Inserting, Recording, Transcribing};
use dictation_core::error::DictationError;
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, FileAudioSource,
};
use dictation_ports::{
    Audio, HotkeyError, HotkeyEvent, InsertError, Permission, PermissionStatus, TranscribeError,
    Transcriber,
};
use tokio::sync::mpsc::UnboundedReceiver;

const HOLD: Duration = Duration::from_millis(500);
const WAIT: Duration = Duration::from_secs(5);

/// The test-only transcriber (the real mock is M3.1): replies with queued
/// results, then with fixed text, and records the audio it was given.
#[derive(Clone, Default)]
struct FixedTranscriber {
    replies: Arc<Mutex<VecDeque<Result<String, TranscribeError>>>>,
    calls: Arc<Mutex<Vec<Audio>>>,
}

impl FixedTranscriber {
    fn reply(&self, result: Result<&str, TranscribeError>) {
        self.replies
            .lock()
            .unwrap()
            .push_back(result.map(str::to_owned));
    }

    fn calls(&self) -> Vec<Audio> {
        self.calls.lock().unwrap().clone()
    }
}

impl Transcriber for FixedTranscriber {
    fn transcribe(
        &self,
        audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
        self.calls.lock().unwrap().push(audio.clone());
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok("fixed text".to_owned()));
        async move { reply }
    }
}

/// A 16 kHz mono WAV in the temp dir, deleted on drop.
struct TempWav(PathBuf);

impl TempWav {
    fn new(samples: &[i16]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dictation-pipeline-{}-{}.wav",
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
        for &sample in samples {
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

struct Harness {
    hotkey: FakeHotkey,
    clock: FakeClock,
    inserter: FakeInserter,
    transcriber: FixedTranscriber,
    permissions: FakePermissions,
    pipeline: Pipeline,
    ui: UnboundedReceiver<UiEvent>,
    _wav: TempWav,
}

struct Setup {
    audio_path: Option<PathBuf>,
    permissions: FakePermissions,
    config: Config,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            audio_path: None,
            permissions: FakePermissions::all_granted(),
            config: Config::default(),
        }
    }
}

impl Harness {
    fn start() -> Self {
        Self::with(Setup::default())
    }

    fn with(setup: Setup) -> Self {
        let wav = TempWav::new(&[0, 16_384, -16_384]);
        let hotkey = FakeHotkey::new();
        let clock = FakeClock::at(Duration::from_secs(100));
        let inserter = FakeInserter::new();
        let transcriber = FixedTranscriber::default();
        let audio = FileAudioSource::new(setup.audio_path.unwrap_or_else(|| wav.0.clone()));
        let ports = Ports {
            hotkey: hotkey.clone(),
            audio,
            transcriber: transcriber.clone(),
            inserter: inserter.clone(),
            permissions: setup.permissions.clone(),
            clock: clock.clone(),
        };
        let (pipeline, ui) = Pipeline::spawn(ports, setup.config).unwrap();
        Self {
            hotkey,
            clock,
            inserter,
            transcriber,
            permissions: setup.permissions,
            pipeline,
            ui,
            _wav: wav,
        }
    }

    fn press(&self) {
        assert!(self.hotkey.emit(HotkeyEvent::Pressed));
    }

    fn release_after(&self, hold: Duration) {
        self.clock.advance(hold);
        assert!(self.hotkey.emit(HotkeyEvent::Released));
    }

    fn dictate(&self) {
        self.press();
        self.release_after(HOLD);
    }

    async fn next(&mut self) -> UiEvent {
        tokio::time::timeout(WAIT, self.ui.recv())
            .await
            .expect("no UI event in time")
            .expect("pipeline stopped")
    }

    /// UI events up to and including the next `StateChanged(Idle)`.
    async fn until_idle(&mut self) -> Vec<UiEvent> {
        let mut events = Vec::new();
        loop {
            let event = self.next().await;
            let idle = event == UiEvent::StateChanged(Idle);
            events.push(event);
            if idle {
                return events;
            }
        }
    }

    /// Stops the pipeline and checks no UI event was left unread.
    async fn finish(mut self) -> (FakeInserter, FixedTranscriber) {
        self.pipeline.shutdown().await;
        assert_eq!(self.ui.recv().await, None, "unexpected extra UI event");
        (self.inserter, self.transcriber)
    }
}

fn states(phases: &[dictation_core::coordinator::Phase]) -> Vec<UiEvent> {
    phases.iter().copied().map(UiEvent::StateChanged).collect()
}

#[tokio::test]
async fn criterion_1_press_release_inserts_the_transcript() {
    let mut harness = Harness::start();
    harness.transcriber.reply(Ok("hello world"));
    harness.dictate();
    harness.until_idle().await;

    let (inserter, transcriber) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["hello world"]);
    // The pipeline handed the captured audio to the transcriber.
    assert_eq!(
        transcriber.calls(),
        [Audio {
            sample_rate: 16_000,
            samples: vec![0.0, 0.5, -0.5],
        }]
    );
}

#[tokio::test]
async fn criterion_2_ui_events_arrive_in_order() {
    let mut harness = Harness::start();
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        states(&[Recording, Transcribing, Inserting, Idle])
    );
    harness.finish().await;
}

#[tokio::test]
async fn criterion_3_transcriber_error_shows_error_and_inserts_nothing() {
    let mut harness = Harness::start();
    harness.transcriber.reply(Err(TranscribeError::Timeout));
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::Error(DictationError::TranscriptionFailed {
                reason: "transcription timed out".into()
            }),
            UiEvent::StateChanged(Idle),
        ]
    );
    let (inserter, _) = harness.finish().await;
    assert!(inserter.inserted().is_empty());
}

#[tokio::test]
async fn criterion_4_cancel_during_recording_never_transcribes_or_inserts() {
    let mut harness = Harness::start();
    harness.press();
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    assert!(harness.hotkey.emit(HotkeyEvent::CancelPressed));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
    // The release after Esc is ignored.
    harness.release_after(HOLD);

    let (inserter, transcriber) = harness.finish().await;
    assert!(transcriber.calls().is_empty());
    assert!(inserter.inserted().is_empty());
}

#[tokio::test]
async fn criterion_5_back_to_back_dictations_both_insert_in_order() {
    let mut harness = Harness::start();
    harness.transcriber.reply(Ok("first"));
    harness.transcriber.reply(Ok("second"));
    for _ in 0..2 {
        harness.dictate();
        assert_eq!(
            harness.until_idle().await,
            states(&[Recording, Transcribing, Inserting, Idle])
        );
    }
    let (inserter, transcriber) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["first", "second"]);
    assert_eq!(transcriber.calls().len(), 2);
}

#[tokio::test]
async fn a_tap_is_discarded_without_transcribing() {
    let mut harness = Harness::start();
    harness.press();
    harness.release_after(Duration::from_millis(50));
    assert_eq!(harness.until_idle().await, states(&[Recording, Idle]));
    let (_, transcriber) = harness.finish().await;
    assert!(transcriber.calls().is_empty());
}

#[tokio::test]
async fn an_empty_transcript_is_an_error_and_inserts_nothing() {
    let mut harness = Harness::start();
    harness.transcriber.reply(Ok("  "));
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::Error(DictationError::EmptyTranscript),
            UiEvent::StateChanged(Idle),
        ]
    );
    let (inserter, _) = harness.finish().await;
    assert!(inserter.inserted().is_empty());
}

#[tokio::test]
async fn an_insert_failure_is_an_error_and_the_next_dictation_works() {
    let mut harness = Harness::start();
    harness.inserter.fail_next(InsertError::SecureInput);
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::StateChanged(Inserting),
            UiEvent::Error(DictationError::InsertFailed {
                reason: "a password field has focus".into()
            }),
            UiEvent::StateChanged(Idle),
        ]
    );
    harness.dictate();
    harness.until_idle().await;
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["fixed text"]);
}

#[tokio::test]
async fn a_missing_permission_shows_an_error_and_never_records() {
    let permissions = FakePermissions::all_granted();
    permissions.set(Permission::Accessibility, PermissionStatus::Denied);
    let mut harness = Harness::with(Setup {
        permissions,
        ..Setup::default()
    });
    harness.dictate();
    assert_eq!(
        harness.next().await,
        UiEvent::Error(DictationError::PermissionMissing(Permission::Accessibility))
    );
    // Denied (not undetermined), so no prompt.
    assert!(harness.permissions.requests().is_empty());
    let (_, transcriber) = harness.finish().await;
    assert!(transcriber.calls().is_empty());
}

#[tokio::test]
async fn an_undetermined_permission_prompts_and_the_next_press_records() {
    let mut harness = Harness::with(Setup {
        permissions: FakePermissions::new(),
        ..Setup::default()
    });
    harness.press();
    assert_eq!(
        harness.next().await,
        UiEvent::Error(DictationError::PermissionMissing(Permission::Microphone))
    );
    assert_eq!(harness.permissions.requests(), [Permission::Microphone]);

    // The fake granted the microphone; Accessibility is next.
    harness.press();
    assert_eq!(
        harness.next().await,
        UiEvent::Error(DictationError::PermissionMissing(Permission::Accessibility))
    );

    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        states(&[Recording, Transcribing, Inserting, Idle])
    );
    harness.finish().await;
}

#[tokio::test]
async fn a_microphone_failure_is_reported_when_the_recording_stops() {
    let mut harness = Harness::with(Setup {
        audio_path: Some(std::env::temp_dir().join("dictation-pipeline-missing.wav")),
        ..Setup::default()
    });
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::Error(DictationError::TranscriptionFailed {
                reason: "the microphone failed (no microphone found)".into()
            }),
            UiEvent::StateChanged(Idle),
        ]
    );
    let (_, transcriber) = harness.finish().await;
    assert!(transcriber.calls().is_empty());
}

#[tokio::test]
async fn the_recording_stops_by_itself_at_five_minutes() {
    let mut harness = Harness::with(Setup {
        config: Config {
            tick_interval: Duration::from_millis(5),
        },
        ..Setup::default()
    });
    harness.press();
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    harness.clock.advance(Duration::from_secs(5 * 60));
    // No release: a Tick ends the recording.
    assert_eq!(
        harness.until_idle().await,
        states(&[Transcribing, Inserting, Idle])
    );
    // The release that finally comes is ignored.
    harness.release_after(HOLD);
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["fixed text"]);
}

#[tokio::test]
async fn shutdown_while_recording_stops_the_hotkey() {
    let mut harness = Harness::start();
    harness.press();
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    let hotkey = harness.hotkey.clone();
    harness.finish().await;
    assert!(!hotkey.is_listening());
}

#[tokio::test]
async fn a_hotkey_that_fails_to_start_fails_spawn() {
    let hotkey = FakeHotkey::new();
    hotkey.fail_next_start(HotkeyError::PermissionDenied);
    let ports = Ports {
        hotkey,
        audio: FileAudioSource::new("unused.wav"),
        transcriber: FixedTranscriber::default(),
        inserter: FakeInserter::new(),
        permissions: FakePermissions::all_granted(),
        clock: FakeClock::new(),
    };
    let error = Pipeline::spawn(ports, Config::default()).err();
    assert_eq!(error, Some(HotkeyError::PermissionDenied));
}
