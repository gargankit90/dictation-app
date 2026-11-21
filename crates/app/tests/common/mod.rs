//! Shared by the pipeline's integration tests: stand-ins for the ports and a
//! temporary WAV file. Each test binary uses only some of it.
#![allow(dead_code, reason = "each test binary uses only some of the helpers")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use dictation_ports::{
    Audio, AudioError, AudioSource, InsertError, Permission, PermissionError, PermissionStatus,
    Permissions, TextInserter, TranscribeError, Transcriber,
};

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
