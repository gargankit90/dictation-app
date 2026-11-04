//! The live input level of the recording in progress (U4), shared between
//! an audio source's capture thread and the pipeline.
//!
//! Privacy: a level is a number only. It is shown in the recording pill and
//! never logged at info level, stored or sent anywhere.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dictation_core::level::{LEVEL_FLOOR_DBFS, rms_to_dbfs};

/// A handle on one audio source's input level. Clones share it.
///
/// The source calls [`begin`](Self::begin) once a recording's stream runs,
/// [`record`](Self::record) with each chunk of captured samples, and
/// [`end`](Self::end) when the recording stops or is discarded. The reader
/// (the pipeline, about 20 times a second) calls [`take`](Self::take).
///
/// The level is the RMS of every sample recorded since the last `take`, in
/// dBFS ([`rms_to_dbfs`]): with ~10 ms chunks and a 50 ms read interval,
/// the RMS of the last ~5 chunks. Outside a recording nothing is kept and
/// `take` returns `None`.
#[derive(Debug, Clone, Default)]
pub struct LevelMeter(Arc<Mutex<Window>>);

#[derive(Debug, Default)]
struct Window {
    recording: bool,
    sum_of_squares: f64,
    samples: u64,
    device: Option<String>,
}

impl LevelMeter {
    /// A meter with no recording.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Window> {
        // Nothing panics while holding it; recover the window if it did.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A recording started on `device` (its name, if known): levels count
    /// from now.
    pub fn begin(&self, device: Option<String>) {
        *self.lock() = Window {
            recording: true,
            device,
            ..Window::default()
        };
    }

    /// Adds a chunk of samples (`-1.0..=1.0`; others are clamped, and
    /// non-finite ones count as silence). Ignored outside a recording.
    pub fn record(&self, samples: &[f32]) {
        let sum: f64 = samples
            .iter()
            .map(|&sample| {
                let sample = if sample.is_finite() {
                    f64::from(sample.clamp(-1.0, 1.0))
                } else {
                    0.0
                };
                sample * sample
            })
            .sum();
        let mut window = self.lock();
        if !window.recording {
            return;
        }
        window.sum_of_squares += sum;
        window.samples += samples.len() as u64;
    }

    /// The recording stopped: nothing more is kept until the next
    /// [`begin`](Self::begin).
    pub fn end(&self) {
        *self.lock() = Window::default();
    }

    /// The level since the last call, in dBFS, and starts a new window.
    /// During a recording with no samples since then (the device delivered
    /// nothing) it is [`LEVEL_FLOOR_DBFS`]: no audio is no sound. `None`
    /// outside a recording.
    #[must_use]
    pub fn take(&self) -> Option<f32> {
        let mut window = self.lock();
        if !window.recording {
            return None;
        }
        let level = if window.samples == 0 {
            LEVEL_FLOOR_DBFS
        } else {
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                reason = "a mean of values in 0..=1, as f32"
            )]
            let rms = (window.sum_of_squares / window.samples as f64).sqrt() as f32;
            rms_to_dbfs(rms)
        };
        window.sum_of_squares = 0.0;
        window.samples = 0;
        Some(level)
    }

    /// The recording's input device name, if the source knows it; `None`
    /// outside a recording.
    #[must_use]
    pub fn device(&self) -> Option<String> {
        self.lock().device.clone()
    }

    /// Whether a recording is being measured.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.lock().recording
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine with the given RMS level in dBFS, 100 ms at 16 kHz.
    fn sine_at(dbfs: f32) -> Vec<f32> {
        let amplitude = 10f32.powf(dbfs / 20.0) * std::f32::consts::SQRT_2;
        (0..1_600)
            .map(|i| amplitude * (i as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin())
            .collect()
    }

    #[test]
    fn nothing_is_kept_outside_a_recording() {
        let meter = LevelMeter::new();
        meter.record(&sine_at(-20.0));
        assert_eq!(meter.take(), None);
        meter.begin(Some("Mic".into()));
        meter.end();
        meter.record(&sine_at(-20.0));
        assert_eq!(meter.take(), None);
        assert_eq!(meter.device(), None);
        assert!(!meter.is_recording());
    }

    #[test]
    fn silence_reads_at_the_floor() {
        let meter = LevelMeter::new();
        meter.begin(None);
        meter.record(&[0.0; 160]);
        assert_eq!(meter.take(), Some(LEVEL_FLOOR_DBFS));
        // No samples at all: still the floor while recording.
        assert_eq!(meter.take(), Some(LEVEL_FLOOR_DBFS));
    }

    #[test]
    fn a_sine_reads_its_level() {
        let meter = LevelMeter::new();
        meter.begin(None);
        for chunk in sine_at(-20.0).chunks(160) {
            meter.record(chunk);
        }
        let level = meter.take().unwrap();
        assert!((level + 20.0).abs() <= 2.0, "{level}");
    }

    #[test]
    fn each_take_starts_a_new_window() {
        let meter = LevelMeter::new();
        meter.begin(Some("MacBook Pro Microphone".into()));
        meter.record(&sine_at(-10.0));
        let loud = meter.take().unwrap();
        meter.record(&sine_at(-40.0));
        let quiet = meter.take().unwrap();
        assert!((loud + 10.0).abs() <= 2.0, "{loud}");
        assert!((quiet + 40.0).abs() <= 2.0, "{quiet}");
        assert_eq!(meter.device().as_deref(), Some("MacBook Pro Microphone"));
    }

    #[test]
    fn clones_share_the_level_and_junk_is_silence() {
        let meter = LevelMeter::new();
        let capture = meter.clone();
        capture.begin(None);
        capture.record(&[f32::NAN, f32::INFINITY]);
        assert_eq!(meter.take(), Some(LEVEL_FLOOR_DBFS));
        capture.record(&[5.0, -5.0]);
        assert_eq!(meter.take(), Some(0.0));
    }
}
