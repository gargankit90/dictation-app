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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_core::coordinator::Phase::{Idle, Inserting, Recording, Transcribing};
use dictation_core::error::DictationError;
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, FileAudioSource,
};
use dictation_ports::{
    Audio, HotkeyError, HotkeyEvent, InsertError, Permission, PermissionStatus, TextInserter,
    TranscribeError, Transcriber,
};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{Notify, Semaphore};

const HOLD: Duration = Duration::from_millis(500);
const WAIT: Duration = Duration::from_secs(5);

/// The test-only transcriber (the real mock is M3.1): replies with queued
/// results, then with fixed text, and records the audio it was given. The
/// next transcription can also be made to panic, or to wait for `open()`.
#[derive(Clone)]
struct FixedTranscriber {
    replies: Arc<Mutex<VecDeque<Result<String, TranscribeError>>>>,
    calls: Arc<Mutex<Vec<Audio>>>,
    crash_next: Arc<AtomicBool>,
    hold_next: Arc<AtomicBool>,
    gate: Arc<Semaphore>,
    /// Notified when a held transcription starts waiting.
    started: Arc<Notify>,
    /// Held transcriptions dropped before they answered.
    dropped: Arc<AtomicUsize>,
}

impl Default for FixedTranscriber {
    fn default() -> Self {
        Self {
            replies: Arc::default(),
            calls: Arc::default(),
            crash_next: Arc::default(),
            hold_next: Arc::default(),
            gate: Arc::new(Semaphore::new(0)),
            started: Arc::default(),
            dropped: Arc::default(),
        }
    }
}

/// Counts a held transcription as dropped unless it answered.
struct Unanswered(Option<Arc<AtomicUsize>>);

impl Drop for Unanswered {
    fn drop(&mut self) {
        if let Some(dropped) = self.0.take() {
            dropped.fetch_add(1, Ordering::SeqCst);
        }
    }
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

    /// The next transcription panics, as a crash in an HTTP library would.
    fn crash_next(&self) {
        self.crash_next.store(true, Ordering::SeqCst);
    }

    /// The next transcription waits for `open()` before it answers.
    fn hold_next(&self) {
        self.hold_next.store(true, Ordering::SeqCst);
    }

    /// Waits until the held transcription is waiting.
    async fn wait_until_held(&self) {
        tokio::time::timeout(WAIT, self.started.notified())
            .await
            .expect("the transcription never started");
    }

    fn open(&self) {
        self.gate.add_permits(1);
    }

    fn dropped(&self) -> usize {
        self.dropped.load(Ordering::SeqCst)
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
        let crash = self.crash_next.swap(false, Ordering::SeqCst);
        let hold = self.hold_next.swap(false, Ordering::SeqCst);
        let (gate, started, dropped) = (
            Arc::clone(&self.gate),
            Arc::clone(&self.started),
            Arc::clone(&self.dropped),
        );
        async move {
            assert!(!crash, "the transcriber crashed (on purpose)");
            if hold {
                let mut unanswered = Unanswered(Some(dropped));
                started.notify_one();
                gate.acquire().await.unwrap().forget();
                unanswered.0 = None;
            }
            reply
        }
    }
}

/// A `FakeInserter` whose next insert can be made to panic.
struct CrashingInserter {
    fake: FakeInserter,
    crash_next: Arc<AtomicBool>,
}

impl TextInserter for CrashingInserter {
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        assert!(
            !self.crash_next.swap(false, Ordering::SeqCst),
            "the inserter crashed (on purpose)"
        );
        self.fake.insert(text)
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
    crash_insert: Arc<AtomicBool>,
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
        let crash_insert = Arc::new(AtomicBool::new(false));
        let transcriber = FixedTranscriber::default();
        let audio = FileAudioSource::new(setup.audio_path.unwrap_or_else(|| wav.0.clone()));
        let ports = Ports {
            hotkey: hotkey.clone(),
            audio,
            transcriber: transcriber.clone(),
            inserter: CrashingInserter {
                fake: inserter.clone(),
                crash_next: Arc::clone(&crash_insert),
            },
            permissions: setup.permissions.clone(),
            clock: clock.clone(),
        };
        let (pipeline, ui) = Pipeline::spawn(ports, setup.config).unwrap();
        Self {
            hotkey,
            clock,
            inserter,
            crash_insert,
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
                reason: TranscribeError::Timeout.user_message()
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
async fn an_empty_transcript_inserts_nothing_and_shows_no_error() {
    let mut harness = Harness::start();
    harness.transcriber.reply(Ok("  "));
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::StateChanged(Idle),
        ]
    );
    // The next dictation works.
    harness.dictate();
    harness.until_idle().await;
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["fixed text"]);
}

#[tokio::test]
async fn the_transcript_is_trimmed_before_it_is_inserted() {
    let mut harness = Harness::start();
    harness.transcriber.reply(Ok("  hello world \n"));
    harness.dictate();
    harness.until_idle().await;
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["hello world"]);
}

#[tokio::test]
async fn an_insert_failure_is_an_error_and_the_next_dictation_works() {
    let mut harness = Harness::start();
    harness
        .inserter
        .fail_next(InsertError::Failed("no focused app".into()));
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::StateChanged(Inserting),
            UiEvent::Error(DictationError::InsertFailed {
                reason: "insert failed: no focused app".into()
            }),
            UiEvent::StateChanged(Idle),
        ]
    );
    harness.dictate();
    harness.until_idle().await;
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["fixed text"]);
}

// M4.2 criterion 1: Secure Input on means no paste, an error event, Idle.
#[tokio::test]
async fn secure_input_shows_its_message_inserts_nothing_and_the_next_dictation_works() {
    let mut harness = Harness::start();
    harness.inserter.fail_next(InsertError::SecureInput);
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::StateChanged(Inserting),
            UiEvent::Error(DictationError::SecureInput),
            UiEvent::StateChanged(Idle),
        ]
    );
    harness.dictate();
    harness.until_idle().await;
    let (inserter, _) = harness.finish().await;
    // Only the second dictation was pasted.
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

#[tokio::test]
async fn a_transcriber_that_panics_is_an_error_and_the_next_dictation_works() {
    let mut harness = Harness::start();
    harness.transcriber.crash_next();
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::Error(DictationError::TranscriptionFailed {
                reason: "the transcriber crashed".into()
            }),
            UiEvent::StateChanged(Idle),
        ]
    );
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        states(&[Recording, Transcribing, Inserting, Idle])
    );
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["fixed text"]);
}

#[tokio::test]
async fn an_inserter_that_panics_is_an_error_and_the_next_dictation_works() {
    let mut harness = Harness::start();
    harness.crash_insert.store(true, Ordering::SeqCst);
    harness.transcriber.reply(Ok("lost"));
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::StateChanged(Inserting),
            UiEvent::Error(DictationError::InsertFailed {
                reason: "the inserter crashed".into()
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
async fn shutdown_drops_a_transcription_in_flight_and_inserts_nothing() {
    let mut harness = Harness::start();
    harness.transcriber.hold_next();
    harness.dictate();
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Transcribing));
    harness.transcriber.wait_until_held().await;

    let (inserter, transcriber) = harness.finish().await;
    // Dropped (its request cancelled), not left running.
    assert_eq!(transcriber.dropped(), 1);
    transcriber.open();
    tokio::task::yield_now().await;
    assert!(inserter.inserted().is_empty());
}

#[tokio::test]
async fn a_transcript_that_is_ready_at_shutdown_is_never_inserted() {
    // Each round, the transcript and the shutdown wake the pipeline task
    // together. Repeated, because an unbiased select would pick either.
    for _ in 0..16 {
        let mut harness = Harness::start();
        harness.transcriber.hold_next();
        harness.dictate();
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        assert_eq!(harness.next().await, UiEvent::StateChanged(Transcribing));
        harness.transcriber.wait_until_held().await;

        harness.transcriber.open();
        // finish() also checks no Inserting (or any other) UI event came.
        let (inserter, _) = harness.finish().await;
        assert!(inserter.inserted().is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn a_queued_tap_comes_before_a_tick_even_five_minutes_later() {
    // The press and release were stamped 50 ms apart, but the pipeline
    // task only gets to them when the clock is past the 5-minute limit,
    // after an idle gap that left a Tick overdue. Repeated, because an
    // unbiased select would pick that Tick half the time.
    for _ in 0..16 {
        let mut harness = Harness::start();
        tokio::time::advance(Duration::from_secs(1)).await;
        harness.press();
        harness.release_after(Duration::from_millis(50));
        harness.clock.advance(Duration::from_secs(6 * 60));
        assert_eq!(harness.until_idle().await, states(&[Recording, Idle]));
        let (inserter, transcriber) = harness.finish().await;
        assert!(transcriber.calls().is_empty());
        assert!(inserter.inserted().is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn the_first_tick_comes_a_full_interval_after_the_recording_starts() {
    let mut harness = Harness::start();
    // The press reaches the pipeline task when the clock is already past
    // the limit, so any Tick would end the recording.
    harness.press();
    harness.clock.advance(Duration::from_secs(6 * 60));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    // No tokio time has passed, so no Tick has come: Esc still cancels.
    assert!(harness.hotkey.emit(HotkeyEvent::CancelPressed));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
    let (_, transcriber) = harness.finish().await;
    assert!(transcriber.calls().is_empty());
}

#[tokio::test]
async fn a_zero_tick_interval_still_runs() {
    let mut harness = Harness::with(Setup {
        config: Config {
            tick_interval: Duration::ZERO,
        },
        ..Setup::default()
    });
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        states(&[Recording, Transcribing, Inserting, Idle])
    );
    harness.finish().await;
}
