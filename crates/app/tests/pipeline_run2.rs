//! M1.3 Tester, run 2: black-box checks of the review-round-1 fixes (job
//! panics, shutdown against jobs in flight, Tick timing) that the
//! Developer's tests don't reach.

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
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use std::time::Duration;

use dictation_core::coordinator::Phase::{Idle, Inserting, Recording, Transcribing};
use dictation_core::error::DictationError;
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, FileAudioSource,
};
use dictation_ports::{
    Audio, HotkeyEvent, InsertError, TextInserter, TranscribeError, Transcriber,
};

const HOLD: Duration = Duration::from_millis(500);
const WAIT: Duration = Duration::from_secs(5);

/// What the next transcription (or insert) does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Plan {
    Answer,
    /// Panics in the call itself, before a future is returned.
    PanicInCall,
    /// Panics with text in the message, to check it never reaches the UI.
    PanicWithText,
}

#[derive(Clone, Default)]
struct PlannedTranscriber {
    plans: Arc<Mutex<VecDeque<Plan>>>,
    calls: Arc<AtomicUsize>,
}

impl PlannedTranscriber {
    fn plan(&self, plan: Plan) {
        self.plans.lock().unwrap().push_back(plan);
    }
}

impl Transcriber for PlannedTranscriber {
    fn transcribe(
        &self,
        _audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let plan = self
            .plans
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Plan::Answer);
        assert!(plan != Plan::PanicInCall, "crashed in the call");
        async move {
            assert!(plan != Plan::PanicWithText, "secret words spoken");
            Ok("fixed text".to_owned())
        }
    }
}

/// Reports that a paste started, then waits for the release.
type Block = (std_mpsc::Sender<()>, std_mpsc::Receiver<()>);

/// A `FakeInserter` that can be made to block mid-paste until released,
/// or to panic with text in the message.
#[derive(Clone, Default)]
struct BlockingInserter {
    fake: FakeInserter,
    /// While set, an insert reports `entered` and waits on `release`.
    block: Arc<Mutex<Option<Block>>>,
    panic_next: Arc<Mutex<bool>>,
    calls: Arc<AtomicUsize>,
}

impl TextInserter for BlockingInserter {
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(
            !std::mem::take(&mut *self.panic_next.lock().unwrap()),
            "secret words pasted"
        );
        if let Some((entered, release)) = self.block.lock().unwrap().take() {
            entered.send(()).unwrap();
            release.recv_timeout(WAIT).unwrap();
        }
        self.fake.insert(text)
    }
}

struct TempWav(PathBuf);

impl TempWav {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dictation-pipeline-run2-{}-{}.wav",
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
    transcriber: PlannedTranscriber,
    inserter: BlockingInserter,
    pipeline: Pipeline,
    ui: UiReceiver,
    _wav: TempWav,
}

impl Harness {
    fn start() -> Self {
        let wav = TempWav::new();
        let hotkey = FakeHotkey::new();
        let clock = FakeClock::at(Duration::from_secs(100));
        let transcriber = PlannedTranscriber::default();
        let inserter = BlockingInserter::default();
        let ports = Ports {
            hotkey: hotkey.clone(),
            audio: FileAudioSource::new(wav.0.clone()),
            transcriber: transcriber.clone(),
            inserter: inserter.clone(),
            permissions: FakePermissions::all_granted(),
            clock: clock.clone(),
        };
        let (pipeline, ui) = Pipeline::spawn(ports, Config::default()).unwrap();
        Self {
            hotkey,
            clock,
            transcriber,
            inserter,
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

    async fn finish(mut self) -> (BlockingInserter, PlannedTranscriber) {
        tokio::time::timeout(WAIT, self.pipeline.shutdown())
            .await
            .expect("shutdown hung");
        assert_eq!(self.ui.recv().await, None, "unexpected extra UI event");
        (self.inserter, self.transcriber)
    }
}

fn states(phases: &[dictation_core::coordinator::Phase]) -> Vec<UiEvent> {
    phases.iter().copied().map(UiEvent::StateChanged).collect()
}

fn transcriber_crashed() -> Vec<UiEvent> {
    vec![
        UiEvent::StateChanged(Recording),
        UiEvent::StateChanged(Transcribing),
        UiEvent::Error(DictationError::TranscriptionFailed {
            reason: "the transcriber crashed".into(),
        }),
        UiEvent::StateChanged(Idle),
    ]
}

#[tokio::test]
async fn a_transcriber_that_panics_in_the_call_itself_is_an_error() {
    let mut harness = Harness::start();
    harness.transcriber.plan(Plan::PanicInCall);
    harness.dictate();
    assert_eq!(harness.until_idle().await, transcriber_crashed());
    harness.dictate();
    assert_eq!(
        harness.until_idle().await,
        states(&[Recording, Transcribing, Inserting, Idle])
    );
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.fake.inserted(), ["fixed text"]);
}

#[tokio::test]
async fn three_transcriber_panics_in_a_row_then_a_dictation_works() {
    let mut harness = Harness::start();
    for plan in [Plan::PanicInCall, Plan::PanicWithText, Plan::PanicInCall] {
        harness.transcriber.plan(plan);
    }
    for _ in 0..3 {
        harness.dictate();
        assert_eq!(harness.until_idle().await, transcriber_crashed());
    }
    harness.dictate();
    harness.until_idle().await;
    let (inserter, transcriber) = harness.finish().await;
    assert_eq!(inserter.fake.inserted(), ["fixed text"]);
    assert_eq!(transcriber.calls.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn a_panic_message_never_reaches_the_ui() {
    let mut harness = Harness::start();
    harness.transcriber.plan(Plan::PanicWithText);
    harness.dictate();
    let mut events = harness.until_idle().await;
    *harness.inserter.panic_next.lock().unwrap() = true;
    harness.dictate();
    events.extend(harness.until_idle().await);
    assert!(
        !format!("{events:?}").contains("secret"),
        "panic text leaked: {events:?}"
    );
    assert!(
        events.contains(&UiEvent::Error(DictationError::InsertFailed {
            reason: "the inserter crashed".into()
        }))
    );
    let (inserter, _) = harness.finish().await;
    assert!(inserter.fake.inserted().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_while_an_insert_is_pasting_returns_and_starts_no_other_insert() {
    let mut harness = Harness::start();
    let (entered_tx, entered) = std_mpsc::channel();
    let (release, release_rx) = std_mpsc::channel();
    *harness.inserter.block.lock().unwrap() = Some((entered_tx, release_rx));
    harness.dictate();
    assert_eq!(
        [
            harness.next().await,
            harness.next().await,
            harness.next().await
        ],
        [
            UiEvent::StateChanged(Recording),
            UiEvent::StateChanged(Transcribing),
            UiEvent::StateChanged(Inserting),
        ]
    );
    entered.recv_timeout(WAIT).unwrap();
    // A second dictation queued behind the paste, then shutdown.
    harness.dictate();
    let hotkey = harness.hotkey.clone();
    let inserter = harness.inserter.clone();
    // shutdown() must not wait for the blocked paste; finish() times out
    // if it does, and checks no UI event (e.g. Idle) came after it.
    let (inserter_after, _) = harness.finish().await;
    assert!(!hotkey.emit(HotkeyEvent::Pressed), "hotkey still running");
    release.send(()).unwrap();
    // Let the paste finish; its result has nowhere to go.
    for _ in 0..50 {
        if inserter.fake.inserted().len() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(inserter_after.fake.inserted(), ["fixed text"]);
    assert_eq!(inserter.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn a_second_recording_after_a_long_idle_gets_no_early_tick() {
    let mut harness = Harness::start();
    harness.dictate();
    harness.until_idle().await;
    // Ten idle minutes: the tick interval isn't polled, so a Tick is long
    // overdue when the next recording starts.
    tokio::time::advance(Duration::from_secs(10 * 60)).await;
    harness.emit(HotkeyEvent::Pressed);
    // The press is stamped now; by the time the task gets to it the clock
    // is past the limit, so any Tick would stop the recording.
    harness.clock.advance(Duration::from_secs(6 * 60));
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    tokio::time::advance(Duration::from_millis(200)).await;
    harness.emit(HotkeyEvent::CancelPressed);
    assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
    let (inserter, transcriber) = harness.finish().await;
    assert_eq!(transcriber.calls.load(Ordering::SeqCst), 1);
    assert_eq!(inserter.fake.inserted(), ["fixed text"]);
}

#[tokio::test(start_paused = true)]
async fn a_real_five_minute_recording_still_stops_by_itself() {
    let mut harness = Harness::start();
    harness.emit(HotkeyEvent::Pressed);
    assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
    // Just under the limit: still recording.
    harness.clock.advance(Duration::from_secs(5 * 60 - 1));
    tokio::time::advance(Duration::from_secs(5 * 60 - 1)).await;
    tokio::task::yield_now().await;
    assert!(harness.ui.try_recv().is_err(), "stopped before 5 minutes");
    harness.clock.advance(Duration::from_secs(2));
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(
        harness.until_idle().await,
        states(&[Transcribing, Inserting, Idle])
    );
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.fake.inserted(), ["fixed text"]);
}
