//! Q2 criterion 2: the cleanup pass in the pipeline, with fakes. A cleaned
//! reply is pasted; a timeout, an error and a reply the guard refuses each
//! paste the raw transcript; a timed-out cleanup is cancelled and doesn't
//! hold up the next dictation.

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

use dictation_core::coordinator::Phase::Idle;
use dictation_pipeline::pipeline::{CleanupStep, Config, Extras, Pipeline, Ports, UiEvent};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, FileAudioSource,
};
use dictation_ports::{
    Audio, Cleaner, CleanupError, CleanupOutcome, HotkeyEvent, TranscribeError, Transcriber,
};

const RAW: &str = "Um, so, send the report to Priya by 5.";
const CLEANED: &str = "Send the report to Priya by 5.";
const WAIT: Duration = Duration::from_secs(5);

/// Always transcribes to [`RAW`].
#[derive(Clone)]
struct Says;

impl Transcriber for Says {
    fn transcribe(
        &self,
        _audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
        async { Ok(RAW.to_owned()) }
    }
}

/// What the fake cleaner does with its next call.
enum Script {
    Reply(&'static str),
    Fail(CleanupError),
    /// Never answers; counts itself dropped when the pipeline gives up.
    Hang,
}

#[derive(Clone, Default)]
struct FakeCleaner {
    scripts: Arc<Mutex<VecDeque<Script>>>,
    seen: Arc<Mutex<Vec<String>>>,
    dropped: Arc<AtomicUsize>,
}

struct CountDrop(Arc<AtomicUsize>);

impl Drop for CountDrop {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl Cleaner for FakeCleaner {
    fn clean(&self, text: &str) -> impl Future<Output = Result<String, CleanupError>> + Send {
        self.seen.lock().unwrap().push(text.to_owned());
        let script = self.scripts.lock().unwrap().pop_front();
        let dropped = Arc::clone(&self.dropped);
        async move {
            match script {
                Some(Script::Reply(text)) => Ok(text.to_owned()),
                Some(Script::Fail(error)) => Err(error),
                Some(Script::Hang) => {
                    let _count = CountDrop(dropped);
                    std::future::pending::<()>().await;
                    unreachable!()
                }
                None => Err(CleanupError::Off),
            }
        }
    }
}

/// A 300 ms tone, deleted on drop: loud and long enough to transcribe.
struct TempWav(PathBuf);

impl TempWav {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dictation-cleanup-{}-{}.wav",
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
        for n in 0..4_800 {
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
    cleaner: FakeCleaner,
    pipeline: Pipeline,
    ui: UiReceiver,
    _wav: TempWav,
}

impl Harness {
    fn start(timeout: Duration, scripts: Vec<Script>) -> Self {
        let wav = TempWav::new();
        let hotkey = FakeHotkey::new();
        let clock = FakeClock::at(Duration::from_secs(100));
        let inserter = FakeInserter::new();
        let cleaner = FakeCleaner::default();
        cleaner.scripts.lock().unwrap().extend(scripts);
        let ports = Ports {
            hotkey: hotkey.clone(),
            audio: FileAudioSource::new(wav.0.clone()),
            transcriber: Says,
            inserter: inserter.clone(),
            permissions: FakePermissions::all_granted(),
            clock: clock.clone(),
        };
        let extras = Extras {
            cursor: None,
            cleanup: Some(CleanupStep::new(cleaner.clone()).with_timeout(timeout)),
        };
        let (pipeline, ui) = Pipeline::spawn_with(ports, extras, Config::default()).unwrap();
        Self {
            hotkey,
            clock,
            inserter,
            cleaner,
            pipeline,
            ui,
            _wav: wav,
        }
    }

    /// One dictation; the UI events up to and including Idle.
    async fn dictate(&mut self) -> Vec<UiEvent> {
        assert!(self.hotkey.emit(HotkeyEvent::Pressed));
        self.clock.advance(Duration::from_millis(500));
        assert!(self.hotkey.emit(HotkeyEvent::Released));
        let mut events = Vec::new();
        loop {
            let event = tokio::time::timeout(WAIT, self.ui.recv())
                .await
                .expect("no UI event in time")
                .expect("pipeline stopped");
            let idle = event == UiEvent::StateChanged(Idle);
            events.push(event);
            if idle {
                return events;
            }
        }
    }

    fn outcomes(events: &[UiEvent]) -> Vec<CleanupOutcome> {
        events
            .iter()
            .filter_map(|event| match event {
                UiEvent::Cleanup(outcome) => Some(*outcome),
                _ => None,
            })
            .collect()
    }

    async fn finish(self) -> (FakeInserter, FakeCleaner) {
        self.pipeline.shutdown().await;
        (self.inserter, self.cleaner)
    }
}

#[tokio::test]
async fn q2_criterion_2_the_cleaned_text_is_pasted() {
    let mut harness = Harness::start(WAIT, vec![Script::Reply(CLEANED)]);
    let events = harness.dictate().await;
    assert_eq!(Harness::outcomes(&events), [CleanupOutcome::Applied]);
    let (inserter, cleaner) = harness.finish().await;
    assert_eq!(inserter.inserted(), [CLEANED]);
    assert_eq!(*cleaner.seen.lock().unwrap(), [RAW], "the raw transcript");
}

#[tokio::test]
async fn q2_criterion_2_an_error_pastes_the_raw_text() {
    for error in [
        CleanupError::InvalidKey,
        CleanupError::OutOfCredits,
        CleanupError::RateLimited,
        CleanupError::NoRoute,
        CleanupError::Provider(503),
        CleanupError::Network,
        CleanupError::BadReply,
    ] {
        let mut harness = Harness::start(WAIT, vec![Script::Fail(error.clone())]);
        let events = harness.dictate().await;
        assert_eq!(
            Harness::outcomes(&events),
            [CleanupOutcome::FallbackError],
            "{error}"
        );
        assert!(
            !events.iter().any(|e| matches!(e, UiEvent::Error(_))),
            "a cleanup error is never shown: {events:?}"
        );
        let (inserter, _) = harness.finish().await;
        assert_eq!(inserter.inserted(), [RAW], "{error}");
    }
}

#[tokio::test]
async fn q2_criterion_2_a_reply_the_guard_refuses_pastes_the_raw_text() {
    // Drops the name and the number.
    let mut harness = Harness::start(WAIT, vec![Script::Reply("Send the report to her.")]);
    let events = harness.dictate().await;
    assert_eq!(Harness::outcomes(&events), [CleanupOutcome::FallbackGuard]);
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), [RAW]);
}

#[tokio::test]
async fn q2_criterion_2_a_timeout_pastes_the_raw_text_cancels_and_frees_the_next_dictation() {
    let timeout = Duration::from_millis(150);
    let mut harness = Harness::start(
        timeout,
        vec![
            Script::Hang,
            Script::Fail(CleanupError::Timeout),
            Script::Reply(CLEANED),
        ],
    );
    let started = tokio::time::Instant::now();
    let events = harness.dictate().await;
    assert_eq!(
        Harness::outcomes(&events),
        [CleanupOutcome::FallbackTimeout]
    );
    // Cancelled: the hung request's future was dropped at the deadline.
    assert_eq!(harness.cleaner.dropped.load(Ordering::SeqCst), 1);
    // The client's own timeout counts as a timeout too.
    let events = harness.dictate().await;
    assert_eq!(
        Harness::outcomes(&events),
        [CleanupOutcome::FallbackTimeout]
    );
    // The next dictation is cleaned, with no wait left over.
    let events = harness.dictate().await;
    assert_eq!(Harness::outcomes(&events), [CleanupOutcome::Applied]);
    assert!(started.elapsed() < WAIT, "took {:?}", started.elapsed());
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), [RAW, RAW, CLEANED]);
}

#[tokio::test]
async fn q2_cleanup_off_pastes_the_raw_text_and_counts_nothing() {
    let mut harness = Harness::start(WAIT, vec![]);
    let events = harness.dictate().await;
    assert!(Harness::outcomes(&events).is_empty(), "{events:?}");
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), [RAW]);
}
