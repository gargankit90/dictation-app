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
use dictation_platform::fakes::{FakeClock, FakeHotkey};
use dictation_ports::{
    Audio, AudioError, AudioSource, HotkeyError, HotkeyEvent, Permission, TranscribeError,
    Transcriber,
};
use tokio::sync::mpsc::UnboundedReceiver;

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
    }

    fn is_recording(&self) -> bool {
        *self.recording.lock().unwrap()
    }
}

struct Run {
    pipeline: Pipeline,
    ui: UnboundedReceiver<UiEvent>,
    injector: Injector,
    clock: FakeClock,
    transcribed: Arc<AtomicUsize>,
}

async fn next(ui: &mut UnboundedReceiver<UiEvent>) -> UiEvent {
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

#[tokio::test(flavor = "multi_thread")]
async fn criterion_7_a_key_flood_behind_a_blocked_start_is_bounded_and_recovers() {
    let (audio, release_gate) = GatedStart::new(None);
    let starts = Arc::clone(&audio.starts);
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
    release_gate.send(()).unwrap();
    // Let the flood drain, then the pipeline must be idle and usable.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut tail = Vec::new();
    while let Ok(e) = run.ui.try_recv() {
        tail.push(e);
    }
    // Known gap (tech-debt issue): a dropped key's cancel is applied after
    // the keys still queued, so the flood can end in Recording even when
    // its last key was a cancel. One Esc settles it either way.
    let mut idle = matches!(tail.last(), Some(UiEvent::StateChanged(Phase::Idle)) | None);
    if !idle {
        assert!(run.injector.inject(HotkeyEvent::CancelPressed));
        let deadline = Instant::now() + WAIT;
        while !idle {
            assert!(Instant::now() < deadline, "no Idle after an Esc");
            idle = next(&mut run.ui).await == UiEvent::StateChanged(Phase::Idle);
        }
    }
    let started_by_flood = starts.load(Ordering::SeqCst);
    assert!(
        started_by_flood <= 1 + 64,
        "{started_by_flood} starts ran for a flood of cancelled recordings"
    );
    let events = run.dictate_once().await;
    assert_eq!(events.last(), Some(&UiEvent::StateChanged(Phase::Idle)));
    assert_eq!(run.transcribed.load(Ordering::SeqCst), 1);
    run.pipeline.shutdown().await;
}
