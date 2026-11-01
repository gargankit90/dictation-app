//! M2.3 Tester (run 1): black-box checks of #81 through the public
//! `Pipeline::spawn` API, with a source whose `start` is gated.
//!
//! - a `PermissionDenied` start that ends after the release reports the
//!   permission error once and never calls `stop` (criterion 8);
//! - cancelled recordings behind a blocked `start` never start, and
//!   shutdown does not replay them (criterion 6);
//! - a synchronous key flood while a `start` blocks queues bounded audio
//!   work and the pipeline then dictates normally (criterion 7).

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use std::time::{Duration, Instant};

use dictation_core::coordinator::{MIN_HOLD, Phase};
use dictation_core::error::DictationError;
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{DiscardingInserter, GrantedPermissions};
use dictation_pipeline::test_hooks::{Injector, SyntheticHotkey, hook_events};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{FakeClock, FakeHotkey};
use dictation_ports::{
    Audio, AudioError, AudioSource, HotkeyError, HotkeyEvent, Permission, TranscribeError,
    Transcriber,
};
use tokio::sync::Notify;

const WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Default)]
struct Counting(Arc<AtomicUsize>);

impl Transcriber for Counting {
    fn transcribe(
        &self,
        _audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
        self.0.fetch_add(1, Ordering::SeqCst);
        async { Ok("text".to_owned()) }
    }
}

/// The first `start` blocks on a gate, then fails with `first_error` (or
/// succeeds if `None`); later starts succeed.
#[derive(Clone)]
struct GatedStart {
    gate: Arc<Mutex<Option<std_mpsc::Receiver<()>>>>,
    first_error: Arc<Mutex<Option<AudioError>>>,
    recording: Arc<Mutex<bool>>,
    starts: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
    /// Signalled when `discard` has run: the fence for the audio worker.
    discarded: Arc<Notify>,
}

impl GatedStart {
    fn new(first_error: Option<AudioError>) -> (Self, std_mpsc::Sender<()>) {
        let (release, gate) = std_mpsc::channel();
        let source = Self {
            gate: Arc::new(Mutex::new(Some(gate))),
            first_error: Arc::new(Mutex::new(first_error)),
            recording: Arc::default(),
            starts: Arc::default(),
            stops: Arc::default(),
            discarded: Arc::new(Notify::new()),
        };
        (source, release)
    }
}

impl AudioSource for GatedStart {
    fn start(&mut self) -> Result<(), AudioError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            let _ = gate.recv();
            if let Some(error) = self.first_error.lock().unwrap().take() {
                return Err(error);
            }
        }
        *self.recording.lock().unwrap() = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        let mut recording = self.recording.lock().unwrap();
        if !*recording {
            return Err(AudioError::NotRecording);
        }
        *recording = false;
        Ok(Audio {
            sample_rate: 16_000,
            samples: vec![0.1; 16_000],
        })
    }

    fn discard(&mut self) {
        *self.recording.lock().unwrap() = false;
        self.discarded.notify_one();
    }

    fn is_recording(&self) -> bool {
        *self.recording.lock().unwrap()
    }
}

struct Run {
    pipeline: Pipeline,
    ui: UiReceiver,
    injector: Injector,
    clock: FakeClock,
    transcribed: Arc<AtomicUsize>,
}

async fn next(ui: &mut UiReceiver) -> UiEvent {
    tokio::time::timeout(WAIT, ui.recv())
        .await
        .unwrap()
        .unwrap()
}

fn spawn(audio: GatedStart) -> Run {
    let real = FakeHotkey::new();
    real.fail_next_start(HotkeyError::PermissionDenied);
    let (hotkey, injector) = SyntheticHotkey::new(real);
    let clock = FakeClock::new();
    let transcriber = Counting::default();
    let transcribed = Arc::clone(&transcriber.0);
    let ports = Ports {
        hotkey,
        audio,
        transcriber,
        inserter: DiscardingInserter,
        permissions: GrantedPermissions,
        clock: clock.clone(),
    };
    let (pipeline, ui) = Pipeline::spawn(ports, Config::default()).unwrap();
    Run {
        pipeline,
        ui,
        injector,
        clock,
        transcribed,
    }
}

impl Run {
    fn hook(&self, flag: &str) {
        for event in hook_events(["dictation-app", flag]) {
            assert!(self.injector.inject(event));
        }
    }

    fn release(&self) {
        self.clock.advance(MIN_HOLD * 2);
        self.hook("--release");
    }

    /// Events up to and including the next `Idle`.
    async fn until_idle(&mut self) -> Vec<UiEvent> {
        let mut seen = Vec::new();
        loop {
            let event = next(&mut self.ui).await;
            let idle = event == UiEvent::StateChanged(Phase::Idle);
            seen.push(event);
            if idle {
                return seen;
            }
        }
    }

    async fn dictate_once(&mut self) -> Vec<UiEvent> {
        self.hook("--press");
        assert_eq!(
            next(&mut self.ui).await,
            UiEvent::StateChanged(Phase::Recording)
        );
        self.release();
        self.until_idle().await
    }
}

async fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn criterion_8_a_denied_start_after_the_release_reports_once_without_stop() {
    let (audio, release_gate) = GatedStart::new(Some(AudioError::PermissionDenied));
    let (starts, stops) = (Arc::clone(&audio.starts), Arc::clone(&audio.stops));
    let mut run = spawn(audio);

    run.hook("--press");
    assert_eq!(
        next(&mut run.ui).await,
        UiEvent::StateChanged(Phase::Recording)
    );
    wait_until("the start to begin", || starts.load(Ordering::SeqCst) == 1).await;
    // Release while the start is still blocked, then let it fail.
    run.release();
    assert_eq!(
        next(&mut run.ui).await,
        UiEvent::StateChanged(Phase::Transcribing)
    );
    release_gate.send(()).unwrap();
    let events = run.until_idle().await;
    let errors: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, UiEvent::Error(_)))
        .collect();
    assert_eq!(
        errors,
        [&UiEvent::Error(DictationError::PermissionMissing(
            Permission::Microphone
        ))],
        "events: {events:?}"
    );
    assert_eq!(
        stops.load(Ordering::SeqCst),
        0,
        "stop ran on a failed start"
    );
    assert_eq!(run.transcribed.load(Ordering::SeqCst), 0);

    // No stale error: the next dictation is clean and transcribes.
    let events = run.dictate_once().await;
    assert!(
        events.iter().all(|e| !matches!(e, UiEvent::Error(_))),
        "events: {events:?}"
    );
    assert_eq!(run.transcribed.load(Ordering::SeqCst), 1);
    run.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn criterion_6_cancelled_recordings_never_start_and_shutdown_does_not_replay_them() {
    let (audio, release_gate) = GatedStart::new(None);
    let starts = Arc::clone(&audio.starts);
    let mut run = spawn(audio);

    run.hook("--press");
    assert_eq!(
        next(&mut run.ui).await,
        UiEvent::StateChanged(Phase::Recording)
    );
    wait_until("the start to begin", || starts.load(Ordering::SeqCst) == 1).await;
    for _ in 0..6 {
        assert!(run.injector.inject(HotkeyEvent::CancelPressed));
        assert_eq!(next(&mut run.ui).await, UiEvent::StateChanged(Phase::Idle));
        run.hook("--press");
        assert_eq!(
            next(&mut run.ui).await,
            UiEvent::StateChanged(Phase::Recording)
        );
    }
    assert!(run.injector.inject(HotkeyEvent::CancelPressed));
    assert_eq!(next(&mut run.ui).await, UiEvent::StateChanged(Phase::Idle));

    // Shutdown while the first start is still blocked: free it from another
    // thread, as the device finally opening.
    let opener = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        release_gate.send(()).unwrap();
    });
    run.pipeline.shutdown().await;
    opener.join().unwrap();
    assert_eq!(
        starts.load(Ordering::SeqCst),
        1,
        "abandoned starts must not run"
    );
    assert_eq!(run.transcribed.load(Ordering::SeqCst), 0);
}

/// A current-thread runtime: the pipeline can't run while the test's own
/// loop injects keys, so the flood fills the queue, deterministically. Then
/// the pipeline must end idle with no recording, and nothing is sent to wake
/// it up (#91: it used to need an Esc, and the test waited 500 ms).
#[tokio::test(flavor = "current_thread")]
async fn criterion_7_a_key_flood_behind_a_blocked_start_is_bounded_and_recovers() {
    let (audio, release_gate) = GatedStart::new(None);
    let starts = Arc::clone(&audio.starts);
    let recording = Arc::clone(&audio.recording);
    let discarded = Arc::clone(&audio.discarded);
    let mut run = spawn(audio);

    run.hook("--press");
    assert_eq!(
        next(&mut run.ui).await,
        UiEvent::StateChanged(Phase::Recording)
    );
    wait_until("the start to begin", || starts.load(Ordering::SeqCst) == 1).await;
    for _ in 0..15_000 {
        assert!(run.injector.inject(HotkeyEvent::CancelPressed));
        run.hook("--press");
    }
    assert!(run.injector.inject(HotkeyEvent::CancelPressed));
    // The fence: the pipeline throws away what was queued and cancels the
    // recording it had, by itself.
    assert_eq!(next(&mut run.ui).await, UiEvent::StateChanged(Phase::Idle));
    release_gate.send(()).unwrap();
    // The overflow's cancellation reached the microphone by itself, before
    // anything else (shutdown also discards and would hide an open one).
    tokio::time::timeout(WAIT, discarded.notified())
        .await
        .expect("overflow never discarded the recording");
    assert!(
        !*recording.lock().unwrap(),
        "overflow left the microphone open"
    );
    // Nothing queued ran: no more starts, no recording, no transcription.
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert!(run.ui.try_recv().is_err(), "nothing after the Idle");
    assert_eq!(starts.load(Ordering::SeqCst), 1, "no cancelled start ran");
    assert_eq!(run.transcribed.load(Ordering::SeqCst), 0);

    let events = run.dictate_once().await;
    assert_eq!(events.last(), Some(&UiEvent::StateChanged(Phase::Idle)));
    assert_eq!(run.transcribed.load(Ordering::SeqCst), 1);
    run.pipeline.shutdown().await;
    assert!(
        !*recording.lock().unwrap(),
        "the microphone is not left recording"
    );
}
