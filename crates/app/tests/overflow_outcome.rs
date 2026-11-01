//! T1 (P1-04, #91): what a hotkey queue overflow leaves behind, checked as an
//! outcome. After keys are dropped the pipeline must end Idle, with the
//! microphone not recording and nothing sent to the transcriber, whatever
//! Pressed, Released and Cancel keys were queued, and without any key press
//! to settle it.
//!
//! The runtime is current-thread, so the pipeline task can't run while the
//! test's loop injects keys: the queue fills and overflows at the same key
//! every time. The completion fence is the pipeline's own `Idle`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail the test on purpose"
)]

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use std::time::Duration;

use dictation_core::coordinator::{MIN_HOLD, Phase};
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{DiscardingInserter, GrantedPermissions};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{FakeClock, FakeHotkey};
use dictation_ports::{Audio, AudioError, AudioSource, HotkeyEvent, TranscribeError, Transcriber};
use tokio::sync::Notify;

const WAIT: Duration = Duration::from_secs(10);

/// More keys than the queue holds (64), so some are dropped.
const BURST: usize = 200;

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

/// The first `start` blocks on a gate (the device opening), later ones don't.
#[derive(Clone)]
struct Mic {
    gate: Arc<Mutex<Option<std_mpsc::Receiver<()>>>>,
    entered: Arc<Notify>,
    /// Signalled when `discard` has run: the fence for the audio worker.
    discarded: Arc<Notify>,
    recording: Arc<Mutex<bool>>,
    starts: Arc<AtomicUsize>,
}

impl Mic {
    fn new() -> (Self, std_mpsc::Sender<()>) {
        let (release, gate) = std_mpsc::channel();
        let mic = Self {
            gate: Arc::new(Mutex::new(Some(gate))),
            entered: Arc::new(Notify::new()),
            discarded: Arc::new(Notify::new()),
            recording: Arc::default(),
            starts: Arc::default(),
        };
        (mic, release)
    }
}

impl AudioSource for Mic {
    fn start(&mut self) -> Result<(), AudioError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            let _ = gate.recv();
        }
        *self.recording.lock().unwrap() = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        *self.recording.lock().unwrap() = false;
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

async fn next(ui: &mut UiReceiver) -> UiEvent {
    tokio::time::timeout(WAIT, ui.recv())
        .await
        .expect("no UI event in time")
        .unwrap()
}

/// What the burst sends: key `n` of the burst.
type Pattern = Box<dyn Fn(usize) -> HotkeyEvent>;

fn patterns() -> Vec<(String, Pattern)> {
    use HotkeyEvent::{CancelPressed, Pressed, Released};
    let mut patterns: Vec<(String, Pattern)> = vec![
        ("all presses".into(), Box::new(|_| Pressed)),
        ("all releases".into(), Box::new(|_| Released)),
        ("all cancels".into(), Box::new(|_| CancelPressed)),
        // Each Released after a Pressed would be a dictation, if it ran.
        (
            "press, release, press, release...".into(),
            Box::new(|n| if n % 2 == 0 { Pressed } else { Released }),
        ),
        // The queue holds 64 repeats; the Released that would stop the
        // recording is the one dropped.
        (
            "64 presses, then the release (dropped)".into(),
            Box::new(|n| if n < 64 { Pressed } else { Released }),
        ),
        (
            "the queued keys end with a release".into(),
            Box::new(|n| if n < 63 { Pressed } else { Released }),
        ),
        (
            "the queued keys end with a press".into(),
            Box::new(|n| if n < 63 { Released } else { Pressed }),
        ),
        (
            "cancels and presses".into(),
            Box::new(|n| if n % 3 == 0 { CancelPressed } else { Pressed }),
        ),
        (
            "the dropped keys end with a cancel".into(),
            Box::new(|n| match n {
                0..=63 => Released,
                _ if n == BURST - 1 => CancelPressed,
                _ => Pressed,
            }),
        ),
    ];
    for seed in 1..=12_u64 {
        patterns.push((
            format!("random keys, seed {seed}"),
            Box::new(move |n| {
                let mixed = (n as u64 + 1)
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    .wrapping_add(seed.wrapping_mul(0xD1B5_4A32_D192_ED03));
                match (mixed >> 33) % 3 {
                    0 => Pressed,
                    1 => Released,
                    _ => CancelPressed,
                }
            }),
        ));
    }
    patterns
}

#[tokio::test(flavor = "current_thread")]
async fn overflow_fence_discards_all_untrusted_queued_keys() {
    for (name, pattern) in patterns() {
        let (mic, release_gate) = Mic::new();
        let (recording, entered, discarded, starts) = (
            Arc::clone(&mic.recording),
            Arc::clone(&mic.entered),
            Arc::clone(&mic.discarded),
            Arc::clone(&mic.starts),
        );
        let transcriber = Counting::default();
        let transcribed = Arc::clone(&transcriber.0);
        let clock = FakeClock::new();
        let hotkey = FakeHotkey::new();
        let keys = hotkey.clone();
        let (pipeline, mut ui) = Pipeline::spawn(
            Ports {
                hotkey,
                audio: mic,
                transcriber,
                inserter: DiscardingInserter,
                permissions: GrantedPermissions,
                clock: clock.clone(),
            },
            Config::default(),
        )
        .unwrap();

        // A recording is open, its device still opening.
        assert!(keys.emit(HotkeyEvent::Pressed));
        assert_eq!(next(&mut ui).await, UiEvent::StateChanged(Phase::Recording));
        tokio::time::timeout(WAIT, entered.notified())
            .await
            .unwrap();

        // The burst, with the pipeline unable to run: 64 keys queue, the
        // rest are dropped. Time moves on so a Released is a real dictation.
        for n in 0..BURST {
            clock.advance(MIN_HOLD * 2);
            assert!(keys.emit(pattern(n)), "{name}");
        }

        // Fence: the pipeline, running at last, cancels by itself.
        assert_eq!(
            next(&mut ui).await,
            UiEvent::StateChanged(Phase::Idle),
            "{name}: ends Idle with no key to settle it"
        );
        release_gate.send(()).unwrap();
        // Fence for the audio worker: the overflow's cancellation reached the
        // microphone, by itself. Shutdown would discard too and hide a
        // microphone left open, so nothing checks it after that.
        tokio::time::timeout(WAIT, discarded.notified())
            .await
            .unwrap_or_else(|_| panic!("{name}: overflow never discarded the recording"));
        assert!(
            !*recording.lock().unwrap(),
            "{name}: overflow left the microphone open"
        );
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert!(ui.try_recv().is_err(), "{name}: nothing after Idle");
        assert_eq!(
            transcribed.load(Ordering::SeqCst),
            0,
            "{name}: no transcription"
        );
        assert_eq!(starts.load(Ordering::SeqCst), 1, "{name}: no other start");

        pipeline.shutdown().await;
        assert!(!*recording.lock().unwrap(), "{name}: no open recording");
        assert_eq!(transcribed.load(Ordering::SeqCst), 0, "{name}");
    }
}
