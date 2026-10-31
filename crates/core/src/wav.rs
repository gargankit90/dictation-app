//! Encoding captured audio as an in-memory WAV file for upload.
//!
//! The output is the canonical 44-byte-header RIFF/WAVE layout: one `fmt `
//! chunk (PCM, mono, 16-bit) and one `data` chunk, little-endian, nothing
//! else. Transcribers want 16 kHz ([`SAMPLE_RATE`]); other rates are
//! encoded as given so fakes and tests can use their own.

use thiserror::Error;

/// The rate the transcribers want, in samples per second.
pub const SAMPLE_RATE: u32 = 16_000;

/// Length of the RIFF, `fmt ` and `data` headers together, in bytes.
pub const HEADER_LEN: usize = 44;

const CHANNELS: u16 = 1;
const BITS_PER_SAMPLE: u16 = 16;
const BYTES_PER_SAMPLE: u16 = BITS_PER_SAMPLE / 8;
/// `WAVE_FORMAT_PCM`.
const FORMAT_PCM: u16 = 1;
/// Size of the PCM `fmt ` chunk body.
const FMT_CHUNK_LEN: u32 = 16;

/// Why audio could not be encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WavError {
    /// The sample rate is zero, or so high the byte rate overflows `u32`.
    #[error("invalid sample rate: {0} Hz")]
    InvalidSampleRate(u32),
    /// The samples don't fit in a WAV file's 4 GiB size fields.
    #[error("audio too long for a WAV file: {0} samples")]
    TooLong(usize),
}

/// Converts one `-1.0..=1.0` sample to 16-bit PCM.
///
/// Values outside the range are clamped, so a clipping microphone saturates
/// instead of wrapping around. `NaN` becomes 0 (silence). The scale is
/// symmetric (`1.0` → `32767`, `-1.0` → `-32767`), so `i16::MIN` is never
/// produced.
#[must_use]
pub fn f32_to_i16(sample: f32) -> i16 {
    if sample.is_nan() {
        return 0;
    }
    let scaled = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round();
    // Clamped to ±32767 above, so the cast neither truncates nor wraps.
    #[allow(clippy::cast_possible_truncation)]
    let pcm = scaled as i16;
    pcm
}

/// Encodes `-1.0..=1.0` mono samples as a 16-bit PCM WAV file.
///
/// Samples are converted with [`f32_to_i16`].
///
/// # Errors
///
/// [`WavError::InvalidSampleRate`] for a zero or overflowing rate and
/// [`WavError::TooLong`] if the data doesn't fit the WAV size fields.
pub fn encode_f32(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, WavError> {
    let mut wav = header(samples.len(), sample_rate)?;
    for &sample in samples {
        wav.extend_from_slice(&f32_to_i16(sample).to_le_bytes());
    }
    Ok(wav)
}

/// Encodes 16-bit PCM mono samples as a WAV file.
///
/// # Errors
///
/// As for [`encode_f32`].
pub fn encode_i16(samples: &[i16], sample_rate: u32) -> Result<Vec<u8>, WavError> {
    let mut wav = header(samples.len(), sample_rate)?;
    for &sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(wav)
}

/// The 44 header bytes, in a buffer with room for the samples.
fn header(sample_count: usize, sample_rate: u32) -> Result<Vec<u8>, WavError> {
    let block_align = CHANNELS * BYTES_PER_SAMPLE;
    let byte_rate = sample_rate
        .checked_mul(u32::from(block_align))
        .filter(|_| sample_rate > 0)
        .ok_or(WavError::InvalidSampleRate(sample_rate))?;
    let data_len = sample_count
        .checked_mul(usize::from(BYTES_PER_SAMPLE))
        .and_then(|len| u32::try_from(len).ok())
        // The RIFF size field counts everything after its own 8 bytes.
        .filter(|len| len.checked_add(36).is_some())
        .ok_or(WavError::TooLong(sample_count))?;
    let riff_len = data_len + 36;

    let mut wav = Vec::with_capacity(HEADER_LEN + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_len.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&FMT_CHUNK_LEN.to_le_bytes());
    wav.extend_from_slice(&FORMAT_PCM.to_le_bytes());
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    Ok(wav)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use proptest::prelude::*;

    use super::*;

    fn u16_at(wav: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([wav[at], wav[at + 1]])
    }

    fn u32_at(wav: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([wav[at], wav[at + 1], wav[at + 2], wav[at + 3]])
    }

    fn decode(wav: &[u8]) -> (hound::WavSpec, Vec<i16>) {
        let reader = hound::WavReader::new(Cursor::new(wav)).unwrap();
        let spec = reader.spec();
        let samples = reader.into_samples::<i16>().map(Result::unwrap).collect();
        (spec, samples)
    }

    #[test]
    fn criterion_1_header_fields_and_length_are_correct() {
        let wav = encode_i16(&[0, 1, -1, i16::MAX, i16::MIN], SAMPLE_RATE).unwrap();

        assert_eq!(wav.len(), HEADER_LEN + 5 * 2);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(u32_at(&wav, 4), u32::try_from(wav.len() - 8).unwrap());
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u32_at(&wav, 16), 16, "fmt chunk size");
        assert_eq!(u16_at(&wav, 20), 1, "PCM");
        assert_eq!(u16_at(&wav, 22), 1, "mono");
        assert_eq!(u32_at(&wav, 24), 16_000, "sample rate");
        assert_eq!(u32_at(&wav, 28), 32_000, "byte rate");
        assert_eq!(u16_at(&wav, 32), 2, "block align");
        assert_eq!(u16_at(&wav, 34), 16, "bits per sample");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(u32_at(&wav, 40), 10, "data size");
        assert_eq!(&wav[44..46], &[0, 0]);
        assert_eq!(&wav[50..52], &[0xff, 0x7f], "i16::MAX little-endian");
    }

    #[test]
    fn criterion_1_round_trips_through_hound() {
        let samples = [0, 1, -1, 1_000, -1_000, i16::MAX, i16::MIN];
        let (spec, decoded) = decode(&encode_i16(&samples, SAMPLE_RATE).unwrap());

        assert_eq!(
            spec,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            }
        );
        assert_eq!(decoded, samples);
    }

    #[test]
    fn criterion_1_empty_audio_is_a_valid_header_only_file() {
        let wav = encode_f32(&[], SAMPLE_RATE).unwrap();

        assert_eq!(wav.len(), HEADER_LEN);
        assert_eq!(u32_at(&wav, 4), 36);
        assert_eq!(u32_at(&wav, 40), 0);
        assert_eq!(decode(&wav).1, Vec::<i16>::new());
    }

    #[test]
    fn criterion_1_five_minutes_at_16_khz_is_the_expected_size() {
        let samples = vec![0.0; 5 * 60 * 16_000];
        let wav = encode_f32(&samples, SAMPLE_RATE).unwrap();

        assert_eq!(wav.len(), HEADER_LEN + 9_600_000);
        assert_eq!(u32_at(&wav, 40), 9_600_000);
    }

    #[test]
    fn other_sample_rates_are_encoded_as_given() {
        let wav = encode_i16(&[0], 44_100).unwrap();

        assert_eq!(u32_at(&wav, 24), 44_100);
        assert_eq!(u32_at(&wav, 28), 88_200);
        assert_eq!(decode(&wav).0.sample_rate, 44_100);
    }

    #[test]
    fn rejects_a_zero_or_overflowing_sample_rate() {
        assert_eq!(encode_i16(&[], 0), Err(WavError::InvalidSampleRate(0)));
        assert_eq!(
            encode_f32(&[], u32::MAX),
            Err(WavError::InvalidSampleRate(u32::MAX))
        );
        assert!(encode_i16(&[], u32::MAX / 2).is_ok());
    }

    #[test]
    fn rejects_audio_too_long_for_the_size_fields() {
        // Only the length is checked before allocating, so no 4 GiB buffer.
        let too_many = (u32::MAX as usize - 36) / 2 + 1;
        assert_eq!(
            header(too_many, SAMPLE_RATE),
            Err(WavError::TooLong(too_many))
        );
        assert_eq!(
            header(usize::MAX, SAMPLE_RATE),
            Err(WavError::TooLong(usize::MAX))
        );
        assert!(header(too_many - 1, SAMPLE_RATE).is_ok());
    }

    #[test]
    fn errors_have_readable_messages() {
        assert_eq!(
            WavError::InvalidSampleRate(0).to_string(),
            "invalid sample rate: 0 Hz"
        );
        assert_eq!(
            WavError::TooLong(3).to_string(),
            "audio too long for a WAV file: 3 samples"
        );
    }

    #[test]
    fn f32_conversion_scales_and_clamps() {
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(-0.0), 0);
        assert_eq!(f32_to_i16(1.0), i16::MAX);
        assert_eq!(f32_to_i16(-1.0), -i16::MAX);
        assert_eq!(f32_to_i16(0.5), 16_384);
        assert_eq!(f32_to_i16(-0.5), -16_384);
        assert_eq!(f32_to_i16(1.5), i16::MAX);
        assert_eq!(f32_to_i16(-7.0), -i16::MAX);
        assert_eq!(f32_to_i16(f32::INFINITY), i16::MAX);
        assert_eq!(f32_to_i16(f32::NEG_INFINITY), -i16::MAX);
        assert_eq!(f32_to_i16(f32::NAN), 0);
    }

    #[test]
    fn f32_samples_round_trip_through_hound() {
        let samples = [0.0, 0.5, -0.5, 1.0, -1.0, 2.0, f32::NAN];
        let (_, decoded) = decode(&encode_f32(&samples, SAMPLE_RATE).unwrap());

        assert_eq!(decoded, [0, 16_384, -16_384, 32_767, -32_767, 32_767, 0]);
    }

    proptest! {
        #[test]
        fn i16_samples_round_trip_exactly(samples in prop::collection::vec(any::<i16>(), 0..2_000)) {
            let wav = encode_i16(&samples, SAMPLE_RATE).unwrap();
            prop_assert_eq!(wav.len(), HEADER_LEN + 2 * samples.len());
            prop_assert_eq!(decode(&wav).1, samples);
        }

        #[test]
        fn f32_samples_round_trip_within_one_step(samples in prop::collection::vec(-1.0f32..=1.0, 0..2_000)) {
            let (_, decoded) = decode(&encode_f32(&samples, SAMPLE_RATE).unwrap());
            prop_assert_eq!(decoded.len(), samples.len());
            for (&pcm, &sample) in decoded.iter().zip(&samples) {
                let back = f32::from(pcm) / f32::from(i16::MAX);
                prop_assert!((back - sample).abs() <= 1.0 / f32::from(i16::MAX));
            }
        }

        #[test]
        fn f32_conversion_never_panics_and_stays_symmetric(sample in any::<f32>()) {
            let pcm = f32_to_i16(sample);
            prop_assert!(pcm > i16::MIN);
            if !sample.is_nan() {
                prop_assert_eq!(f32_to_i16(-sample), -pcm);
            }
        }
    }
}
