//! M4.3 Tester run 1: silence assessment at the levels measured in #72,
//! using noise-like (not sine) signals and a mid-clip word.

use dictation_core::audio_math::{DEFAULT_SILENCE_THRESHOLD, Verdict, assess};

const RATE: u32 = 16_000;

/// Deterministic pseudo-noise (xorshift) scaled so its RMS is `dbfs`.
fn noise(dbfs: f32, millis: u32) -> Vec<f32> {
    let rms = 10f32.powf(dbfs / 20.0);
    let mut state: u32 = 0x1234_5678;
    (0..RATE * millis / 1000)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            // uniform in -1..1 has RMS 1/sqrt(3)
            #[allow(clippy::cast_precision_loss)]
            let unit = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            unit * rms * 3f32.sqrt()
        })
        .collect()
}

#[test]
fn issue_72_levels_with_noise_like_speech_are_transcribed() {
    for dbfs in [-36.9, -40.3, -45.0] {
        assert_eq!(
            assess(&noise(dbfs, 2_000), RATE, DEFAULT_SILENCE_THRESHOLD),
            Verdict::Transcribe,
            "{dbfs} dBFS"
        );
    }
}

#[test]
fn quiet_room_noise_is_silent() {
    for dbfs in [-55.0, -60.0, -70.0, -90.0] {
        assert_eq!(
            assess(&noise(dbfs, 2_000), RATE, DEFAULT_SILENCE_THRESHOLD),
            Verdict::Silent,
            "{dbfs} dBFS"
        );
    }
}

#[test]
fn a_word_in_the_middle_of_a_long_pause_is_transcribed() {
    let mut clip = vec![0.0; (RATE * 3) as usize];
    let word = noise(-38.0, 200);
    let at = RATE as usize;
    clip[at..at + word.len()].copy_from_slice(&word);
    assert_eq!(
        assess(&clip, RATE, DEFAULT_SILENCE_THRESHOLD),
        Verdict::Transcribe
    );
}

#[test]
fn a_199_ms_clip_is_too_short_even_when_loud() {
    assert_eq!(
        assess(&noise(-20.0, 199), RATE, DEFAULT_SILENCE_THRESHOLD),
        Verdict::TooShort
    );
}
