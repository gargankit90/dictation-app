//! Tester (M2.1 run 1): `wav` and `audio_math` cases the unit tests don't
//! cover: 1-sample and odd-length files, non-finite samples through hound,
//! the silence threshold at and near its boundary, and exact durations.

use std::io::Cursor;
use std::time::Duration;

use dictation_core::audio_math::{
    DEFAULT_SILENCE_THRESHOLD, MIN_DURATION, Verdict, assess, duration, is_long_enough, is_silent,
    rms,
};
use dictation_core::wav::{HEADER_LEN, SAMPLE_RATE, encode_f32, encode_i16};
use proptest::prelude::*;

const RATE: u32 = SAMPLE_RATE;

fn u16_at(wav: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([wav[at], wav[at + 1]])
}

fn u32_at(wav: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([wav[at], wav[at + 1], wav[at + 2], wav[at + 3]])
}

#[allow(clippy::unwrap_used, reason = "test helper; a failure fails the test")]
fn decode(wav: &[u8]) -> Vec<i16> {
    let reader = hound::WavReader::new(Cursor::new(wav)).unwrap();
    assert_eq!(reader.spec().channels, 1);
    assert_eq!(reader.spec().sample_rate, RATE);
    assert_eq!(reader.spec().bits_per_sample, 16);
    reader.into_samples::<i16>().map(Result::unwrap).collect()
}

/// Every header field for a file of `count` samples at 16 kHz.
#[allow(clippy::unwrap_used, reason = "test helper; a failure fails the test")]
fn assert_header(wav: &[u8], count: usize) {
    let data = u32::try_from(count * 2).unwrap();
    assert_eq!(wav.len(), HEADER_LEN + count * 2, "total length");
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(u32_at(wav, 4), 36 + data, "RIFF size");
    assert_eq!(&wav[8..16], b"WAVEfmt ");
    assert_eq!(u32_at(wav, 16), 16, "fmt size");
    assert_eq!(u16_at(wav, 20), 1, "PCM");
    assert_eq!(u16_at(wav, 22), 1, "channels");
    assert_eq!(u32_at(wav, 24), 16_000, "sample rate");
    assert_eq!(u32_at(wav, 28), 32_000, "byte rate");
    assert_eq!(u16_at(wav, 32), 2, "block align");
    assert_eq!(u16_at(wav, 34), 16, "bits");
    assert_eq!(&wav[36..40], b"data");
    assert_eq!(u32_at(wav, 40), data, "data size");
}

#[test]
fn header_is_correct_for_empty_one_sample_and_odd_lengths() {
    for count in [0, 1, 3, 3_199, 3_201] {
        let samples: Vec<i16> = (0..count).map(|n| (n % 7) as i16 - 3).collect();
        let wav = encode_i16(&samples, RATE).unwrap();
        assert_header(&wav, count);
        assert_eq!(decode(&wav), samples, "{count} samples");

        let floats = vec![0.25f32; count];
        assert_header(&encode_f32(&floats, RATE).unwrap(), count);
    }
}

#[test]
fn one_sample_file_has_the_sample_little_endian_after_the_header() {
    let wav = encode_i16(&[-2], RATE).unwrap();
    assert_eq!(wav.len(), 46);
    assert_eq!(&wav[44..], &[0xfe, 0xff]);
}

#[test]
fn non_finite_and_out_of_range_f32_round_trip_clamped() {
    let samples = [
        f32::NAN,
        -f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -2.0,
        1.000_001,
        f32::MAX,
        f32::MIN,
        f32::MIN_POSITIVE,
    ];
    let decoded = decode(&encode_f32(&samples, RATE).unwrap());
    assert_eq!(
        decoded,
        [0, 0, 32_767, -32_767, -32_767, 32_767, 32_767, -32_767, 0]
    );
}

#[test]
fn rms_and_assess_do_not_panic_on_non_finite_input() {
    let samples = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::MIN,
    ];
    let value = rms(&samples);
    assert!(value.is_finite() && (0.0..=1.0).contains(&value));
    // NaN/inf count as 0, MAX/MIN clamp to ±1: sqrt(2/5).
    assert!((value - 0.4f32.sqrt()).abs() < 1e-6);
    let _ = assess(&samples, RATE, f32::INFINITY);
    let _ = assess(&samples, RATE, f32::NEG_INFINITY);
}

#[test]
fn silence_threshold_at_and_near_the_default_boundary() {
    let at = vec![DEFAULT_SILENCE_THRESHOLD; 16_000];
    assert!(
        !is_silent(&at, DEFAULT_SILENCE_THRESHOLD),
        "exactly at: speech"
    );
    let below = vec![0.009_9f32; 16_000];
    assert!(is_silent(&below, DEFAULT_SILENCE_THRESHOLD), "just below");
    let above = vec![-0.010_1f32; 16_000];
    assert!(!is_silent(&above, DEFAULT_SILENCE_THRESHOLD), "just above");
}

#[test]
fn degenerate_thresholds() {
    let quiet = [0.0f32; 10];
    assert!(!is_silent(&quiet, 0.0), "zero threshold keeps everything");
    assert!(
        !is_silent(&quiet, -1.0),
        "negative threshold keeps everything"
    );
    assert!(
        is_silent(&[1.0; 10], f32::INFINITY),
        "inf threshold drops all"
    );
}

#[test]
fn duration_is_integer_exact_at_16_khz() {
    // One sample at 16 kHz is exactly 62_500 ns.
    for count in [0usize, 1, 2, 3_199, 3_200, 3_201, 16_000, 9_600_000] {
        assert_eq!(
            duration(count, RATE),
            Duration::from_nanos(count as u64 * 62_500),
            "{count}"
        );
    }
    assert_eq!(duration(3_200, RATE), MIN_DURATION);
}

#[test]
fn minimum_duration_boundary_through_assess() {
    let t = DEFAULT_SILENCE_THRESHOLD;
    assert!(is_long_enough(3_200, RATE));
    assert!(!is_long_enough(3_199, RATE));
    assert_eq!(assess(&vec![0.5; 3_199], RATE, t), Verdict::TooShort);
    assert_eq!(assess(&vec![0.5; 3_200], RATE, t), Verdict::Transcribe);
    assert_eq!(assess(&vec![0.0; 3_200], RATE, t), Verdict::Silent);
    assert_eq!(assess(&[0.5], RATE, t), Verdict::TooShort);
    assert_eq!(assess(&[], RATE, t), Verdict::Empty);
    assert_eq!(assess(&[], 0, t), Verdict::Empty);
}

#[test]
fn ten_minutes_of_audio_encodes_and_measures() {
    let samples = vec![0.5f32; 10 * 60 * 16_000];
    let wav = encode_f32(&samples, RATE).unwrap();
    assert_header(&wav, samples.len());
    assert!((rms(&samples) - 0.5).abs() < 1e-6);
    assert_eq!(duration(samples.len(), RATE), Duration::from_secs(600));
    assert_eq!(
        assess(&samples, RATE, DEFAULT_SILENCE_THRESHOLD),
        Verdict::Transcribe
    );
}

proptest! {
    #[test]
    fn header_sizes_match_any_length(count in 0usize..5_000) {
        let wav = encode_i16(&vec![1; count], RATE).unwrap();
        prop_assert_eq!(wav.len(), HEADER_LEN + 2 * count);
        prop_assert_eq!(u32_at(&wav, 4) as usize, wav.len() - 8);
        prop_assert_eq!(u32_at(&wav, 40) as usize, 2 * count);
    }

    #[test]
    fn duration_at_16_khz_is_exact(count in 0usize..1_000_000_000) {
        prop_assert_eq!(duration(count, RATE), Duration::from_nanos(count as u64 * 62_500));
    }
}
