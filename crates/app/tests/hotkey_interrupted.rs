//! M1.6 (#80): what the pipeline does when the hotkey source loses track
//! of the keyboard (`Interrupted`) or dies (`Failed`). The source is the
//! fake; the macOS adapter's side is tested in `dictation-platform`.

// clippy.toml allows these in #[test] fns but not in the helpers they share.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail the test on purpose"
)]

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dictation_core::coordinator::MAX_RECORDING;
use dictation_core::coordinator::Phase::{Idle, Inserting, Recording, Transcribing};
use dictation_core::error::DictationError;
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{FakeClock, FakeHotkey, FakeInserter, FakePermissions};
use dictation_ports::{Audio, AudioError, AudioSource, HotkeyEvent, TranscribeError, Transcriber};

const WAIT: Duration = Duration::from_secs(5);
/// Long enough for several 1 ms Ticks to reach the coordinator.
const QUIET: Duration = Duration::from_millis(200);

/// A microphone that records nothing, counting discards.
#[derive(Clone, Default)]
struct CountingAudio {
    discards: Arc<AtomicUsize>,
}

impl AudioSource for CountingAudio {
    fn start(&mut self) -> Result<(), AudioError> {
        Ok(())
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        Ok(Audio {
            sample_rate: 16_000,
            samples: vec![0.1; 16_000],
        })
    }

    fn discard(&mut self) {
        self.discards.fetch_add(1, Ordering::SeqCst);
    }

    fn is_recording(&self) -> bool {
        false
    }
}

/// Counts transcriptions; there must be none here.
#[derive(Clone, Default)]
struct CountingTranscriber {
    calls: Arc<AtomicUsize>,
}

impl Transcriber for CountingTranscriber {
    fn transcribe(
        &self,
        _audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
        self.calls.fetch_add(1, Ordering::SeqCst);
        async { Ok("text".to_owned()) }
    }
}

struct Harness {
    hotkey: FakeHotkey,
    clock: FakeClock,
    audio: CountingAudio,
    transcriber: CountingTranscriber,
    pipeline: Pipeline,
    ui: UiReceiver,
}

impl Harness {
    fn start() -> Self {
        let hotkey = FakeHotkey::new();
        let clock = FakeClock::at(Duration::from_secs(100));
        let audio = CountingAudio::default();
        let transcriber = CountingTranscriber::default();
        let ports = Ports {
            hotkey: hotkey.clone(),
            audio: audio.clone(),
            transcriber: transcriber.clone(),
            inserter: FakeInserter::new(),
            permissions: FakePermissions::all_granted(),
            clock: clock.clone(),
        };
        // Ticks as fast as they go, so a duration-limit Tick would come at
        // once if anything were still recording.
        let config = Config {
            tick_interval: Duration::ZERO,
        };
        let (pipeline, ui) = Pipeline::spawn(ports, config).unwrap();
        Self {
            hotkey,
            clock,
            audio,
            transcriber,
            pipeline,
            ui,
        }
    }

    fn emit(&self, event: HotkeyEvent) {
        assert!(self.hotkey.emit(event));
    }

    async fn next(&mut self) -> UiEvent {
        tokio::time::timeout(WAIT, self.ui.recv())
            .await
            .expect("no UI event in time")
            .expect("pipeline stopped")
    }

    async fn assert_quiet(&mut self) {
        let next = tokio::time::timeout(QUIET, self.ui.recv()).await;
        assert!(next.is_err(), "unexpected UI event: {next:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn criterion_2_an_interrupted_hold_is_discarded_never_transcribed() {
    let mut harness = Harness::start();
    harness.emit(HotkeyEvent::Pressed);
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));

    // The tap was disabled while the key was down and its release is lost;
    // no key arrives after the source reports it.
    harness.emit(HotkeyEvent::Interrupted);
    assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));

    // Past the duration limit: before M1.6 its Tick stopped and transcribed.
    harness.clock.advance(MAX_RECORDING * 2);
    harness.assert_quiet().await;
    assert_eq!(harness.transcriber.calls.load(Ordering::SeqCst), 0);

    // The hotkey still works: a new hold records and transcribes.
    harness.emit(HotkeyEvent::Pressed);
    harness.clock.advance(Duration::from_millis(500));
    harness.emit(HotkeyEvent::Released);
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Transcribing));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Inserting));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));

    harness.pipeline.shutdown().await;
    assert_eq!(harness.transcriber.calls.load(Ordering::SeqCst), 1);
    assert!(harness.audio.discards.load(Ordering::SeqCst) >= 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn criterion_2_an_interruption_while_idle_changes_nothing() {
    let mut harness = Harness::start();
    harness.emit(HotkeyEvent::Interrupted);
    harness.assert_quiet().await;
    harness.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn criterion_3_a_failed_hotkey_while_recording_discards_and_shows_the_error() {
    let mut harness = Harness::start();
    harness.emit(HotkeyEvent::Pressed);
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));

    harness.emit(HotkeyEvent::Failed);
    assert_eq!(
        harness.next().await,
        UiEvent::Error(DictationError::HotkeyFailed)
    );
    assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
    harness.clock.advance(MAX_RECORDING * 2);
    harness.assert_quiet().await;
    harness.pipeline.shutdown().await;
    assert_eq!(harness.transcriber.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn criterion_3_a_failed_hotkey_while_idle_shows_the_error() {
    let mut harness = Harness::start();
    harness.emit(HotkeyEvent::Failed);
    assert_eq!(
        harness.next().await,
        UiEvent::Error(DictationError::HotkeyFailed)
    );
    harness.assert_quiet().await;
    harness.pipeline.shutdown().await;
}
