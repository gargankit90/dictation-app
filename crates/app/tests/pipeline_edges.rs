//! M1.3 Tester: edge cases the criteria tests don't reach. Time comes from
//! `FakeClock`. A gated transcriber holds a dictation in Transcribing until
//! the test opens the gate; `settle` lets the pipeline task run on the
//! test's current-thread runtime without sleeping.

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
    Audio, HotkeyEvent, Permission, PermissionStatus, TranscribeError, Transcriber,
};
use tokio::sync::Semaphore;
use tokio::sync::mpsc::UnboundedReceiver;

const HOLD: Duration = Duration::from_millis(500);
const WAIT: Duration = Duration::from_secs(5);

/// Replies with queued results (then "fixed text"). While gated, each
/// transcription waits for one `open()` before it answers.
#[derive(Clone)]
struct GatedTranscriber {
    replies: Arc<Mutex<VecDeque<Result<String, TranscribeError>>>>,
    calls: Arc<AtomicUsize>,
    gate: Option<Arc<Semaphore>>,
}

impl GatedTranscriber {
    fn new(gated: bool) -> Self {
        Self {
            replies: Arc::default(),
            calls: Arc::default(),
            gate: gated.then(|| Arc::new(Semaphore::new(0))),
        }
    }

    fn reply(&self, result: Result<&str, TranscribeError>) {
        self.replies
            .lock()
            .unwrap()
            .push_back(result.map(str::to_owned));
    }

    /// Lets one waiting (or the next) transcription answer.
    fn open(&self) {
        self.gate.as_ref().expect("not gated").add_permits(1);
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Transcriber for GatedTranscriber {
    fn transcribe(
        &self,
        _audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok("fixed text".to_owned()));
        let gate = self.gate.clone();
        async move {
            if let Some(gate) = gate {
                gate.acquire().await.unwrap().forget();
            }
            reply
        }
    }
}

/// A 16 kHz mono WAV in the temp dir, deleted on drop.
struct TempWav(PathBuf);

impl TempWav {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dictation-pipeline-edges-{}-{}.wav",
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
        // 300 ms of a 440 Hz tone: long and loud enough to be transcribed.
        for n in 0..4_800_u32 {
            let t = f64::from(n) / 16_000.0;
            #[allow(clippy::cast_possible_truncation)]
            let sample = (16_384.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i16;
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
    transcriber: GatedTranscriber,
    permissions: FakePermissions,
    pipeline: Pipeline,
    ui: UnboundedReceiver<UiEvent>,
    _wav: TempWav,
}

impl Harness {
    fn start(gated: bool) -> Self {
        Self::with(gated, FakePermissions::all_granted())
    }

    fn with(gated: bool, permissions: FakePermissions) -> Self {
        let wav = TempWav::new();
        let hotkey = FakeHotkey::new();
        let clock = FakeClock::at(Duration::from_secs(100));
        let inserter = FakeInserter::new();
        let transcriber = GatedTranscriber::new(gated);
        let ports = Ports {
            hotkey: hotkey.clone(),
            audio: FileAudioSource::new(wav.0.clone()),
            transcriber: transcriber.clone(),
            inserter: inserter.clone(),
            permissions: permissions.clone(),
            clock: clock.clone(),
        };
        let (pipeline, ui) = Pipeline::spawn(ports, Config::default()).unwrap();
        Self {
            hotkey,
            clock,
            inserter,
            transcriber,
            permissions,
            pipeline,
            ui,
            _wav: wav,
        }
    }

    fn emit(&self, event: HotkeyEvent) {
        assert!(self.hotkey.emit(event));
    }

    fn dictate(&self) {
        self.emit(HotkeyEvent::Pressed);
        self.clock.advance(HOLD);
        self.emit(HotkeyEvent::Released);
    }

    async fn next(&mut self) -> UiEvent {
        tokio::time::timeout(WAIT, self.ui.recv())
            .await
            .expect("no UI event in time")
            .expect("pipeline stopped")
    }

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

    /// Lets the pipeline task drain what it has been sent. The tests run on
    /// a current-thread runtime, so the task only runs while this yields.
    async fn settle(&self) {
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
    }

    async fn finish(mut self) -> (FakeInserter, GatedTranscriber) {
        self.pipeline.shutdown().await;
        assert_eq!(self.ui.recv().await, None, "unexpected extra UI event");
        (self.inserter, self.transcriber)
    }
}

fn states(phases: &[dictation_core::coordinator::Phase]) -> Vec<UiEvent> {
    phases.iter().copied().map(UiEvent::StateChanged).collect()
}

#[tokio::test]
async fn a_press_while_transcribing_is_ignored() {
    let mut harness = Harness::start(true);
    harness.transcriber.reply(Ok("one"));
    harness.dictate();
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Transcribing));

    // Press and hold again while the first dictation is still transcribing.
    harness.emit(HotkeyEvent::Pressed);
    harness.clock.advance(HOLD);
    harness.emit(HotkeyEvent::Released);
    harness.settle().await;

    harness.transcriber.open();
    assert_eq!(harness.until_idle().await, states(&[Inserting, Idle]));
    let (inserter, transcriber) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["one"]);
    assert_eq!(transcriber.calls(), 1);
}

#[tokio::test]
async fn esc_right_after_release_does_not_cancel_the_transcription() {
    // Cancel applies only while recording (M1.1); once released, Esc is
    // ignored and the text still goes in.
    let mut harness = Harness::start(false);
    harness.transcriber.reply(Ok("kept"));
    harness.dictate();
    harness.emit(HotkeyEvent::CancelPressed);
    assert_eq!(
        harness.until_idle().await,
        states(&[Recording, Transcribing, Inserting, Idle])
    );
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["kept"]);
}

#[tokio::test]
async fn esc_while_transcribing_is_ignored_and_the_text_still_inserts() {
    let mut harness = Harness::start(true);
    harness.transcriber.reply(Ok("kept"));
    harness.dictate();
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Transcribing));
    harness.emit(HotkeyEvent::CancelPressed);
    harness.settle().await;
    harness.transcriber.open();
    assert_eq!(harness.until_idle().await, states(&[Inserting, Idle]));
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["kept"]);
}

#[tokio::test]
async fn a_denied_microphone_on_press_never_records() {
    let permissions = FakePermissions::all_granted();
    permissions.set(Permission::Microphone, PermissionStatus::Denied);
    let mut harness = Harness::with(false, permissions);
    harness.dictate();
    assert_eq!(
        harness.next().await,
        UiEvent::Error(DictationError::PermissionMissing(Permission::Microphone))
    );
    assert!(harness.permissions.requests().is_empty());
    // Granting it makes the next dictation work.
    harness
        .permissions
        .set(Permission::Microphone, PermissionStatus::Granted);
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        states(&[Recording, Transcribing, Inserting, Idle])
    );
    let (inserter, transcriber) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["fixed text"]);
    assert_eq!(transcriber.calls(), 1);
}

#[tokio::test]
async fn a_transcriber_error_then_a_successful_dictation() {
    let mut harness = Harness::start(false);
    harness
        .transcriber
        .reply(Err(TranscribeError::Network("connection reset".into())));
    harness.transcriber.reply(Ok("second try"));
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::Error(DictationError::TranscriptionFailed {
                reason: "network error: connection reset".into()
            }),
            UiEvent::StateChanged(Idle),
        ]
    );
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        states(&[Recording, Transcribing, Inserting, Idle])
    );
    let (inserter, transcriber) = harness.finish().await;
    assert_eq!(inserter.inserted(), ["second try"]);
    assert_eq!(transcriber.calls(), 2);
}

#[tokio::test]
async fn ten_dictations_in_a_row_insert_in_order() {
    let mut harness = Harness::start(false);
    let texts: Vec<String> = (0..10).map(|n| format!("text {n}")).collect();
    for text in &texts {
        harness.transcriber.reply(Ok(text));
    }
    for _ in &texts {
        harness.dictate();
        assert_eq!(
            harness.until_idle().await,
            states(&[Recording, Transcribing, Inserting, Idle])
        );
    }
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), texts);
}

#[tokio::test]
async fn shutdown_while_transcribing_never_inserts() {
    let mut harness = Harness::start(true);
    harness.dictate();
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Transcribing));
    let transcriber = harness.transcriber.clone();
    let (inserter, _) = harness.finish().await;
    // The abandoned transcription answers after shutdown.
    transcriber.open();
    tokio::task::yield_now().await;
    assert!(inserter.inserted().is_empty());
}
