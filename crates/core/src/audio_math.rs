//! Measuring captured audio: loudness, length, and whether a clip is worth
//! sending to a transcriber.
//!
//! Samples are mono `f32` in `-1.0..=1.0`, as the audio port delivers them.
//! Nothing here panics: empty buffers measure as zero, a zero sample rate as
//! zero length, and non-finite samples (`NaN`, `±inf`) count as silence.

use std::time::Duration;

use crate::coordinator::MIN_HOLD;

/// RMS below this is silence: 0.01 is −40 dBFS.
///
/// Laptop and headset microphones in a quiet room sit around −60 to
/// −50 dBFS, while even soft speech close to the mic reaches −35 to
/// −20 dBFS, so −40 dBFS leaves room on both sides. Sending silence costs
/// an API call and invites the transcriber to hallucinate words.
pub const DEFAULT_SILENCE_THRESHOLD: f32 = 0.01;

/// Clips shorter than this are not transcribed.
///
/// The same 200 ms as [`MIN_HOLD`], the coordinator's shortest hotkey hold:
/// anything shorter is an accidental tap, too short to hold a word, and is
/// where transcribers make up text. The check is repeated on the audio
/// because the microphone starts a little after the key goes down, so a
/// hold just over [`MIN_HOLD`] can yield less audio than that.
pub const MIN_DURATION: Duration = MIN_HOLD;

/// Root mean square of the samples: 0.0 for silence, 1.0 for a full-scale
/// square wave.
///
/// Each sample is clamped to `-1.0..=1.0` first, so the result is always in
/// `0.0..=1.0`. Non-finite samples count as 0.0. Empty input gives 0.0.
#[must_use]
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_of_squares: f64 = samples
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
    // usize -> f64 loses precision only past 2^53 samples (years of audio),
    // and the mean of values in 0..=1 always fits an f32.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let rms = (sum_of_squares / samples.len() as f64).sqrt() as f32;
    rms
}

/// Whether the samples are quieter than `threshold` (RMS, `0.0..=1.0`).
///
/// Empty input is silent. A `NaN` threshold matches nothing, so a broken
/// setting never throws speech away.
#[must_use]
pub fn is_silent(samples: &[f32], threshold: f32) -> bool {
    rms(samples) < threshold
}

/// How long `sample_count` samples last at `sample_rate` Hz, to the
/// nanosecond (rounded down). Zero when `sample_rate` is zero.
#[must_use]
pub fn duration(sample_count: usize, sample_rate: u32) -> Duration {
    if sample_rate == 0 {
        return Duration::ZERO;
    }
    let rate = u64::from(sample_rate);
    // usize is at most 64 bits on every target we build for.
    let count = sample_count as u64;
    let whole_seconds = count / rate;
    // remainder < rate <= u32::MAX, so this fits a u64 and the result is
    // below 10^9.
    let nanos = (count % rate) * 1_000_000_000 / rate;
    #[allow(clippy::cast_possible_truncation)]
    Duration::new(whole_seconds, nanos as u32)
}

/// Whether `sample_count` samples at `sample_rate` Hz last at least
/// [`MIN_DURATION`]. Empty audio and a zero sample rate are too short.
#[must_use]
pub fn is_long_enough(sample_count: usize, sample_rate: u32) -> bool {
    duration(sample_count, sample_rate) >= MIN_DURATION
}

/// What to do with a finished recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Worth transcribing.
    Transcribe,
    /// No samples at all (or no sample rate).
    Empty,
    /// Shorter than [`MIN_DURATION`].
    TooShort,
    /// Long enough, but quieter than the threshold throughout.
    Silent,
}

/// Decides whether a recording goes to the transcriber, checking emptiness,
/// then length, then loudness against `silence_threshold` (normally
/// [`DEFAULT_SILENCE_THRESHOLD`]).
#[must_use]
pub fn assess(samples: &[f32], sample_rate: u32, silence_threshold: f32) -> Verdict {
    if samples.is_empty() || sample_rate == 0 {
        Verdict::Empty
    } else if !is_long_enough(samples.len(), sample_rate) {
        Verdict::TooShort
    } else if is_silent(samples, silence_threshold) {
        Verdict::Silent
    } else {
        Verdict::Transcribe
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const RATE: u32 = 16_000;

    /// A sine wave of `amplitude` at 440 Hz, `millis` long.
    fn tone(amplitude: f32, millis: u32) -> Vec<f32> {
        let count = RATE * millis / 1_000;
        (0..count)
            .map(|n| {
                #[allow(clippy::cast_precision_loss)]
                let t = n as f32 / RATE as f32;
                amplitude * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn rms_of_known_signals() {
        assert!(close(rms(&[0.0; 100]), 0.0));
        assert!(close(rms(&[0.5, -0.5, 0.5, -0.5]), 0.5));
        assert!(close(rms(&[1.0, -1.0]), 1.0));
        // A sine's RMS is its amplitude over sqrt(2).
        assert!(close(rms(&tone(0.5, 1_000)), 0.5 / 2f32.sqrt()));
    }

    #[test]
    fn criterion_3_rms_of_an_empty_buffer_is_zero() {
        assert!(close(rms(&[]), 0.0));
    }

    #[test]
    fn rms_clamps_out_of_range_samples_and_ignores_non_finite_ones() {
        assert!(close(rms(&[2.0, -5.0]), 1.0));
        assert!(close(
            rms(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY]),
            0.0
        ));
        assert!(close(rms(&[f32::NAN, 1.0]), 0.5f32.sqrt()));
    }

    #[test]
    fn criterion_2_silence_below_the_threshold_is_detected() {
        assert!(is_silent(&vec![0.0; 16_000], DEFAULT_SILENCE_THRESHOLD));
        // Room noise around -55 dBFS.
        assert!(is_silent(&tone(0.0025, 1_000), DEFAULT_SILENCE_THRESHOLD));
        assert!(is_silent(&[], DEFAULT_SILENCE_THRESHOLD));
    }

    #[test]
    fn criterion_2_speech_above_the_threshold_is_not_silent() {
        // Soft speech around -30 dBFS, and a loud voice.
        assert!(!is_silent(&tone(0.045, 1_000), DEFAULT_SILENCE_THRESHOLD));
        assert!(!is_silent(&tone(0.8, 1_000), DEFAULT_SILENCE_THRESHOLD));
    }

    #[test]
    fn criterion_2_the_threshold_is_exclusive_and_configurable() {
        let samples = [0.2, -0.2];
        assert!(!is_silent(&samples, 0.2), "exactly at the threshold");
        assert!(is_silent(&samples, 0.21));
        assert!(!is_silent(&samples, 0.1));
    }

    #[test]
    fn criterion_2_a_nan_threshold_never_discards_audio() {
        assert!(!is_silent(&[0.0; 10], f32::NAN));
    }

    #[test]
    fn criterion_2_a_short_burst_in_silence_is_not_silent() {
        let mut samples = vec![0.0; 16_000];
        samples.extend(tone(0.5, 200));
        assert!(!is_silent(&samples, DEFAULT_SILENCE_THRESHOLD));
    }

    #[test]
    fn duration_from_sample_count_and_rate() {
        assert_eq!(duration(16_000, RATE), Duration::from_secs(1));
        assert_eq!(duration(8_000, RATE), Duration::from_millis(500));
        assert_eq!(
            duration(1, RATE),
            Duration::from_micros(62) + Duration::from_nanos(500)
        );
        assert_eq!(
            duration(44_100 * 3 + 441, 44_100),
            Duration::from_millis(3_010)
        );
        assert_eq!(duration(4_800_000, RATE), Duration::from_mins(5));
    }

    #[test]
    fn criterion_3_duration_of_empty_audio_or_a_zero_rate_is_zero() {
        assert_eq!(duration(0, RATE), Duration::ZERO);
        assert_eq!(duration(16_000, 0), Duration::ZERO);
    }

    #[test]
    fn duration_handles_extreme_inputs() {
        assert_eq!(
            duration(usize::MAX, 1),
            Duration::from_secs(usize::MAX as u64)
        );
        assert_eq!(
            duration(usize::MAX, u32::MAX).as_secs(),
            usize::MAX as u64 / u64::from(u32::MAX)
        );
        assert_eq!(duration(1, u32::MAX), Duration::ZERO);
    }

    #[test]
    fn criterion_3_minimum_length_is_200_ms() {
        assert_eq!(MIN_DURATION, Duration::from_millis(200));
        assert_eq!(MIN_DURATION, MIN_HOLD);
        assert!(!is_long_enough(3_199, RATE));
        assert!(is_long_enough(3_200, RATE));
        assert!(!is_long_enough(0, RATE));
        assert!(!is_long_enough(3_200, 0));
    }

    #[test]
    fn criterion_3_assess_checks_empty_then_length_then_silence() {
        let t = DEFAULT_SILENCE_THRESHOLD;
        assert_eq!(assess(&[], RATE, t), Verdict::Empty);
        assert_eq!(assess(&vec![0.5; 16_000], 0, t), Verdict::Empty);
        assert_eq!(assess(&tone(0.5, 150), RATE, t), Verdict::TooShort);
        assert_eq!(assess(&[0.0; 100], RATE, t), Verdict::TooShort);
        assert_eq!(assess(&vec![0.0; 16_000], RATE, t), Verdict::Silent);
        assert_eq!(assess(&tone(0.5, 200), RATE, t), Verdict::Transcribe);
        assert_eq!(assess(&vec![f32::NAN; 16_000], RATE, t), Verdict::Silent);
    }

    proptest! {
        #[test]
        fn rms_is_within_bounds(samples in prop::collection::vec(any::<f32>(), 0..2_000)) {
            let value = rms(&samples);
            prop_assert!((0.0..=1.0).contains(&value), "rms {value}");
        }

        #[test]
        fn rms_is_at_most_the_peak(samples in prop::collection::vec(-1.0f32..=1.0, 1..2_000)) {
            let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
            prop_assert!(rms(&samples) <= peak + 1e-6);
        }

        #[test]
        fn constant_signal_rms_is_its_magnitude(level in -1.0f32..=1.0, len in 1usize..2_000) {
            prop_assert!((rms(&vec![level; len]) - level.abs()).abs() < 1e-5);
        }

        #[test]
        fn duration_matches_float_maths(count in 0usize..100_000_000, rate in 1u32..200_000) {
            #[allow(clippy::cast_precision_loss)]
            let expected = count as f64 / f64::from(rate);
            prop_assert!((duration(count, rate).as_secs_f64() - expected).abs() < 1e-6);
        }

        #[test]
        fn assess_never_panics(
            samples in prop::collection::vec(any::<f32>(), 0..5_000),
            rate in any::<u32>(),
            threshold in any::<f32>(),
        ) {
            let _ = assess(&samples, rate, threshold);
        }
    }
}
