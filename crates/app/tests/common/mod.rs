//! Shared by the pipeline's integration tests: stand-ins for the ports, a
//! temporary WAV file, and a harness that runs one dictation through the
//! real pipeline. Each test binary uses only some of it.
#![allow(dead_code, reason = "each test binary uses only some of the helpers")]
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
use std::time::{Duration, Instant};

use dictation_core::coordinator::Phase::Idle;
use dictation_pipeline::pipeline::{CleanupStep, Config, Extras, Pipeline, Ports, UiEvent};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, FileAudioSource,
};
use dictation_ports::{
    Audio, AudioError, AudioSource, Cleaner, CleanupError, CleanupOutcome, HotkeyEvent,
    InsertError, Permission, PermissionError, PermissionStatus, Permissions, TextInserter,
    TranscribeError, Transcriber,
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

/// The sample rate of [`ToneAudio`]'s recordings.
const SAMPLE_RATE: u32 = 16_000;

/// The text [`FixedTranscriber`] returns.
pub const FIXED_TRANSCRIPT: &str = "This is a test dictation.";

/// Records a tone: [`stop`](AudioSource::stop) returns one second of a
/// 440 Hz tone at 16 kHz, however long the recording lasted. It is loud and
/// long enough for the pipeline to send it on to the transcriber; real
/// silence never gets that far ([`dictation_core::audio_math::assess`]).
#[derive(Debug, Default)]
pub struct ToneAudio {
    started: Option<Instant>,
}

impl AudioSource for ToneAudio {
    fn start(&mut self) -> Result<(), AudioError> {
        if self.started.is_some() {
            return Err(AudioError::AlreadyRecording);
        }
        self.started = Some(Instant::now());
        Ok(())
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        self.started.take().ok_or(AudioError::NotRecording)?;
        let samples = (0..SAMPLE_RATE)
            .map(|n| {
                #[allow(clippy::cast_precision_loss, reason = "n < 16_000")]
                let t = n as f32 / SAMPLE_RATE as f32;
                0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();
        Ok(Audio {
            sample_rate: SAMPLE_RATE,
            samples,
        })
    }

    fn discard(&mut self) {
        self.started = None;
    }

    fn is_recording(&self) -> bool {
        self.started.is_some()
    }
}

/// Answers every transcription with [`FIXED_TRANSCRIPT`].
#[derive(Debug, Default, Clone, Copy)]
pub struct FixedTranscriber;

impl Transcriber for FixedTranscriber {
    async fn transcribe(&self, _audio: &Audio) -> Result<String, TranscribeError> {
        Ok(FIXED_TRANSCRIPT.to_owned())
    }
}

/// Pastes nothing; logs only the text's length (at debug).
#[derive(Debug, Default, Clone, Copy)]
pub struct DiscardingInserter;

impl TextInserter for DiscardingInserter {
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        tracing::debug!(
            chars = text.chars().count(),
            "stand-in inserter: not pasting"
        );
        Ok(())
    }
}

/// Reports every permission granted: the stand-ins above need none. The
/// real hotkey still needs Accessibility and says so when it can't start.
#[derive(Debug, Default, Clone, Copy)]
pub struct GrantedPermissions;

impl Permissions for GrantedPermissions {
    fn check(&self, _permission: Permission) -> PermissionStatus {
        PermissionStatus::Granted
    }

    fn request(&self, _permission: Permission) -> Result<(), PermissionError> {
        Ok(())
    }
}

/// A 16-bit mono WAV file in the temp folder, removed when dropped.
pub struct TempWav(pub PathBuf);

impl TempWav {
    /// `samples` at 16 kHz.
    pub fn new(samples: &[i16]) -> Self {
        Self::with_rate(16_000, samples)
    }

    /// `samples` at `sample_rate`.
    pub fn with_rate(sample_rate: u32, samples: &[i16]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dictation-test-{}-{}.wav",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
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

    /// `millis` of a 440 Hz tone at 16 kHz, loud enough to be transcribed.
    pub fn tone(millis: u32) -> Self {
        let samples: Vec<i16> = (0..millis * 16)
            .map(|n| {
                let t = f64::from(n) / 16_000.0;
                #[allow(clippy::cast_possible_truncation, reason = "within i16")]
                let sample = (16_384.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i16;
                sample
            })
            .collect();
        Self::new(&samples)
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
        let wav = TempWav::tone(300);
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
