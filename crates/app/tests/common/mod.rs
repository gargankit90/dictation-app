//! Shared by the cleanup pipeline tests: fakes and a harness that runs
//! one dictation through the real pipeline.

#![allow(dead_code, reason = "each test binary uses part of this")]
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

pub const RAW: &str = "Um, so, send the report to Priya by 5.";
pub const CLEANED: &str = "Send the report to Priya by 5.";
pub const WAIT: Duration = Duration::from_secs(5);

/// Always transcribes to [`RAW`].
#[derive(Clone)]
pub struct Says;

impl Transcriber for Says {
    async fn transcribe(&self, _audio: &Audio) -> Result<String, TranscribeError> {
        Ok(RAW.to_owned())
    }
}

/// What the fake cleaner does with its next call.
pub enum Script {
    Reply(&'static str),
    Fail(CleanupError),
    /// Never answers; counts itself dropped when the pipeline gives up.
    Hang,
}

#[derive(Clone, Default)]
pub struct FakeCleaner {
    pub scripts: Arc<Mutex<VecDeque<Script>>>,
    pub seen: Arc<Mutex<Vec<String>>>,
    pub dropped: Arc<AtomicUsize>,
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
pub struct TempWav(pub PathBuf);

impl TempWav {
    pub fn new() -> Self {
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

pub struct Harness {
    pub hotkey: FakeHotkey,
    pub clock: FakeClock,
    pub inserter: FakeInserter,
    pub cleaner: FakeCleaner,
    pub pipeline: Pipeline,
    pub ui: UiReceiver,
    _wav: TempWav,
}

impl Harness {
    pub fn start(timeout: Duration, scripts: Vec<Script>) -> Self {
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
    pub async fn dictate(&mut self) -> Vec<UiEvent> {
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

    pub fn outcomes(events: &[UiEvent]) -> Vec<CleanupOutcome> {
        events
            .iter()
            .filter_map(|event| match event {
                UiEvent::Cleanup(outcome) => Some(*outcome),
                _ => None,
            })
            .collect()
    }

    pub async fn finish(self) -> (FakeInserter, FakeCleaner) {
        self.pipeline.shutdown().await;
        (self.inserter, self.cleaner)
    }
}
