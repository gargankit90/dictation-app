//! Black-box checks of `FileAudioSource` (the debug audio file the app really
//! uses) and of the clipboard markers, through the public API only. Self-tests
//! of the other fakes were removed in SIMP1: their callers exercise them. Integration tests see the crate without `cfg(test)`, so
//! they need the `testing` feature:
//! `cargo nextest run -p dictation-platform --features testing`.
#![cfg(feature = "testing")]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use dictation_platform::fakes::{FileAudioSource, MemClipboard};
use dictation_ports::{AudioError, AudioSource, Clipboard, ClipboardMarkers};

// Helpers sit in a `cfg(test)` module so clippy's allow-unwrap-in-tests
// covers them.
#[cfg(test)]
mod support {
    use super::*;

    /// A file in the temp dir, deleted on drop.
    pub struct TempFile(pub PathBuf);

    impl TempFile {
        pub fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            Self(std::env::temp_dir().join(format!(
                "dictation-platform-it-{}-{}.wav",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }

        pub fn wav<S: hound::Sample + Copy>(spec: hound::WavSpec, samples: &[S]) -> Self {
            let file = Self::new();
            let mut writer = hound::WavWriter::create(&file.0, spec).unwrap();
            for &sample in samples {
                writer.write_sample(sample).unwrap();
            }
            writer.finalize().unwrap();
            file
        }

        pub fn bytes(bytes: &[u8]) -> Self {
            let file = Self::new();
            fs::write(&file.0, bytes).unwrap();
            file
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    pub fn int_spec(channels: u16, bits_per_sample: u16) -> hound::WavSpec {
        hound::WavSpec {
            channels,
            sample_rate: 16_000,
            bits_per_sample,
            sample_format: hound::SampleFormat::Int,
        }
    }

    pub fn record(path: &PathBuf) -> Result<dictation_ports::Audio, AudioError> {
        let mut source = FileAudioSource::new(path);
        source.start()?;
        source.stop()
    }
}

use support::{TempFile, int_spec, record};

// --- FileAudioSource: WAV decoding edge cases -------------------------------

#[test]
fn empty_wav_records_zero_samples() {
    let wav = TempFile::wav::<i16>(int_spec(1, 16), &[]);
    let audio = record(&wav.0).unwrap();
    assert_eq!(audio.sample_rate, 16_000);
    assert!(audio.samples.is_empty());
    assert_eq!(audio.duration(), Duration::ZERO);
}

#[test]
fn zero_byte_file_is_a_device_error_and_nothing_records() {
    let file = TempFile::bytes(&[]);
    let mut source = FileAudioSource::new(&file.0);
    assert!(matches!(source.start(), Err(AudioError::Device(_))));
    assert!(!source.is_recording());
}

#[test]
fn truncated_wav_is_a_device_error_not_a_panic() {
    let wav = TempFile::wav::<i16>(int_spec(1, 16), &[1_000; 100]);
    let bytes = fs::read(&wav.0).unwrap();
    // Cut the data chunk short; the header still claims 100 samples.
    let truncated = TempFile::bytes(&bytes[..bytes.len() - 51]);
    let mut source = FileAudioSource::new(&truncated.0);
    assert!(matches!(source.start(), Err(AudioError::Device(_))));
    assert!(!source.is_recording());
}

#[test]
fn header_only_riff_is_a_device_error() {
    let file = TempFile::bytes(b"RIFF\x24\x00\x00\x00WAVE");
    assert!(matches!(record(&file.0), Err(AudioError::Device(_))));
}

#[test]
fn eight_bit_wav_is_scaled_to_full_range() {
    let wav = TempFile::wav::<i8>(int_spec(1, 8), &[0, 64, -64, i8::MIN]);
    assert_eq!(record(&wav.0).unwrap().samples, [0.0, 0.5, -0.5, -1.0]);
}

#[test]
fn twenty_four_bit_wav_is_scaled_to_full_range() {
    let full = 1 << 23;
    let wav = TempFile::wav::<i32>(int_spec(1, 24), &[0, full / 2, -full / 2, -full]);
    assert_eq!(record(&wav.0).unwrap().samples, [0.0, 0.5, -0.5, -1.0]);
}

#[test]
fn thirty_two_bit_int_wav_is_scaled_to_full_range() {
    let wav = TempFile::wav::<i32>(int_spec(1, 32), &[0, 1 << 30, i32::MIN]);
    assert_eq!(record(&wav.0).unwrap().samples, [0.0, 0.5, -1.0]);
}

#[test]
fn samples_stay_within_minus_one_to_one() {
    let wav = TempFile::wav::<i16>(int_spec(2, 16), &[i16::MAX, i16::MAX, i16::MIN, i16::MIN]);
    let audio = record(&wav.0).unwrap();
    assert_eq!(audio.samples.len(), 2);
    assert!(audio.samples.iter().all(|s| (-1.0..=1.0).contains(s)));
}

#[test]
fn file_replaced_between_recordings_is_reread() {
    let wav = TempFile::wav::<i16>(int_spec(1, 16), &[0; 160]);
    let mut source = FileAudioSource::new(&wav.0);
    source.start().unwrap();
    assert_eq!(source.stop().unwrap().samples.len(), 160);

    let other = TempFile::wav::<i16>(int_spec(1, 16), &[0; 320]);
    fs::copy(&other.0, &wav.0).unwrap();
    source.start().unwrap();
    assert_eq!(source.stop().unwrap().samples.len(), 320);
}

#[test]
fn file_deleted_after_start_still_stops_with_the_audio() {
    let wav = TempFile::wav::<i16>(int_spec(1, 16), &[0; 16]);
    let mut source = FileAudioSource::new(&wav.0);
    source.start().unwrap();
    fs::remove_file(&wav.0).unwrap();
    assert_eq!(source.stop().unwrap().samples.len(), 16);
    assert_eq!(source.start(), Err(AudioError::NoDevice));
}

// --- FakeHotkey: ordering and threading --------------------------------------

// --- Round trips and shared state ---------------------------------------------

// --- The fakes satisfy the ports' Send/Sync bounds as trait objects -----------

// --- Run 2: review round 1 fixes ----------------------------------------------

/// A 16 kHz mono PCM WAV with a hand-written header claiming
/// `bits_per_sample`, which hound won't write itself.
#[cfg(test)]
fn pcm_header_wav(bits_per_sample: u16) -> TempFile {
    let block_align = bits_per_sample.div_ceil(8).max(1);
    let data = vec![0u8; usize::from(block_align) * 2];
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&u32::try_from(36 + data.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&16_000u32.to_le_bytes());
    bytes.extend_from_slice(&(16_000 * u32::from(block_align)).to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(&data);
    TempFile::bytes(&bytes)
}

#[test]
fn unsupported_sample_widths_are_an_error_not_a_panic() {
    for bits in [0, 33, 64] {
        let wav = pcm_header_wav(bits);
        let mut source = FileAudioSource::new(&wav.0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| source.start()));
        let result = result.unwrap_or_else(|_| panic!("{bits}-bit header panicked"));
        assert!(result.is_err(), "{bits}-bit header was accepted");
        assert!(
            !source.is_recording(),
            "{bits}-bit header left it recording"
        );
    }
}

#[test]
fn audio_clones_share_state_across_threads() {
    let wav = TempFile::wav(int_spec(1, 16), &[16_384i16]);
    let mut source = FileAudioSource::new(&wav.0);
    let mut handle = source.clone();
    thread::spawn(move || handle.start().unwrap())
        .join()
        .unwrap();
    assert!(source.is_recording());
    assert_eq!(source.stop().unwrap().samples, [0.5]);
    assert!(!source.is_recording());
}

/// The paste flow through the port trait only.
#[cfg(test)]
fn dictate(
    clipboard: &dyn Clipboard,
    text: &str,
) -> (Option<String>, dictation_ports::SessionMarker) {
    let previous = clipboard.read_text().unwrap();
    (previous, clipboard.write_dictation(text).unwrap())
}

#[test]
fn dictation_write_is_marked_and_restored_when_untouched() {
    let clipboard = MemClipboard::with_text("user's text");
    let handle = clipboard.clone();
    let (previous, session) = dictate(&handle, "hello");
    assert_eq!(clipboard.markers(), ClipboardMarkers::DICTATION);
    assert_eq!(clipboard.session().as_ref(), Some(&session));
    assert_eq!(clipboard.read_text(), Ok(Some("hello".into())));

    let restored =
        thread::spawn(move || handle.restore_if_unchanged(&session, previous.as_deref()))
            .join()
            .unwrap();
    assert_eq!(restored, Ok(true));
    assert_eq!(clipboard.read_text(), Ok(Some("user's text".into())));
    assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
    assert_eq!(clipboard.session(), None);
}

#[test]
fn restore_skipped_when_user_copied_in_between() {
    let clipboard = MemClipboard::with_text("old");
    let (previous, session) = dictate(&clipboard, "hello");
    clipboard.user_copies("new copy");
    assert_eq!(
        clipboard.restore_if_unchanged(&session, previous.as_deref()),
        Ok(false)
    );
    assert_eq!(clipboard.read_text(), Ok(Some("new copy".into())));
    assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
}

#[test]
fn restore_skipped_after_a_plain_write_text() {
    let clipboard = MemClipboard::with_text("old");
    let (previous, session) = dictate(&clipboard, "hello");
    clipboard.write_text("hello").unwrap();
    assert_eq!(clipboard.session(), None);
    assert_eq!(
        clipboard.restore_if_unchanged(&session, previous.as_deref()),
        Ok(false)
    );
    assert_eq!(clipboard.read_text(), Ok(Some("hello".into())));
}

#[test]
fn a_session_restores_only_once() {
    let clipboard = MemClipboard::with_text("old");
    let (previous, session) = dictate(&clipboard, "hello");
    assert_eq!(
        clipboard.restore_if_unchanged(&session, previous.as_deref()),
        Ok(true)
    );
    clipboard.user_copies("later");
    assert_eq!(
        clipboard.restore_if_unchanged(&session, previous.as_deref()),
        Ok(false)
    );
    assert_eq!(clipboard.read_text(), Ok(Some("later".into())));
}
