//! [`FileAudioSource`]: "records" a WAV file. A fake for tests, and the
//! source behind the shell's debug-only `--debug-audio-file` test hook.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use dictation_ports::{Audio, AudioError, AudioSource, LevelMeter};

use super::lock;

/// An [`AudioSource`] that "records" a WAV file: [`start`](AudioSource::start)
/// reads the file and [`stop`](AudioSource::stop) returns its audio, mixed
/// down to mono, as `-1.0..=1.0` samples at the file's own sample rate (no
/// resampling). Clones share the recording, like the other fakes.
///
/// A missing file is [`AudioError::NoDevice`]; an unreadable one, or one
/// with an unsupported sample width, is [`AudioError::Device`].
///
/// Its [`LevelMeter`] (U4) plays the file's level in real time: while
/// recording, a thread feeds the meter one 10 ms chunk every 10 ms, the
/// file over and over, as a microphone hearing it would. So the overlay's
/// meter can be checked end to end with a file: a tone keeps it up, and a
/// silent file shows "No sound".
#[derive(Debug, Clone)]
pub struct FileAudioSource {
    path: Arc<PathBuf>,
    captured: Arc<Mutex<Option<Audio>>>,
    meter: LevelMeter,
    /// Bumped by each start, stop and discard; the feeding thread stops
    /// once it changes.
    generation: Arc<AtomicU64>,
}

impl FileAudioSource {
    /// A source that plays back the WAV at `path`. The file is read on each
    /// `start`, not here.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Arc::new(path.into()),
            captured: Arc::new(Mutex::new(None)),
            meter: LevelMeter::new(),
            generation: Arc::default(),
        }
    }

    /// Feeds `audio` to the meter in real time until the next start, stop
    /// or discard. Without a thread the meter just reads silence.
    fn play_levels(&self, audio: &Audio) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.meter.begin(None);
        let meter = self.meter.clone();
        let current = Arc::clone(&self.generation);
        let samples = audio.samples.clone();
        let chunk = usize::try_from(audio.sample_rate / 100)
            .unwrap_or(160)
            .max(1);
        let playing = thread::Builder::new()
            .name("file-levels".into())
            .spawn(move || {
                if samples.is_empty() {
                    return;
                }
                for block in samples.chunks(chunk).cycle() {
                    if current.load(Ordering::Acquire) != generation {
                        return;
                    }
                    meter.record(block);
                    thread::sleep(Duration::from_millis(10));
                }
            });
        if let Err(error) = playing {
            tracing::debug!(%error, "no level thread for the file");
        }
    }

    /// Ends the meter's recording and stops its thread.
    fn stop_levels(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.meter.end();
    }
}

impl AudioSource for FileAudioSource {
    fn start(&mut self) -> Result<(), AudioError> {
        let mut captured = lock(&self.captured);
        if captured.is_some() {
            return Err(AudioError::AlreadyRecording);
        }
        let audio = read_wav(&self.path)?;
        self.play_levels(&audio);
        *captured = Some(audio);
        Ok(())
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        let audio = lock(&self.captured)
            .take()
            .ok_or(AudioError::NotRecording)?;
        self.stop_levels();
        Ok(audio)
    }

    fn discard(&mut self) {
        if lock(&self.captured).take().is_some() {
            self.stop_levels();
        }
    }

    fn is_recording(&self) -> bool {
        lock(&self.captured).is_some()
    }

    fn level_meter(&self) -> Option<LevelMeter> {
        Some(self.meter.clone())
    }
}

fn read_wav(path: &Path) -> Result<Audio, AudioError> {
    let reader = hound::WavReader::open(path).map_err(|error| match error {
        hound::Error::IoError(io) if io.kind() == io::ErrorKind::NotFound => AudioError::NoDevice,
        error => device_error(path, &error),
    })?;
    let spec = reader.spec();
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|error| device_error(path, &error))?,
        hound::SampleFormat::Int => {
            // hound accepts headers it can't decode into i32 (e.g. 64-bit
            // PCM); check before shifting so they error instead of panicking.
            if !(1..=32).contains(&spec.bits_per_sample) {
                return Err(AudioError::Device(format!(
                    "cannot read {}: unsupported {}-bit samples",
                    path.display(),
                    spec.bits_per_sample
                )));
            }
            let full_scale = f64::from(1u32 << (spec.bits_per_sample - 1));
            reader
                .into_samples::<i32>()
                .map(|sample| sample.map(|s| (f64::from(s) / full_scale) as f32))
                .collect::<Result<_, _>>()
                .map_err(|error| device_error(path, &error))?
        }
    };
    let channels = usize::from(spec.channels.max(1));
    let samples = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    Ok(Audio {
        sample_rate: spec.sample_rate,
        samples,
    })
}

fn device_error(path: &Path, error: &hound::Error) -> AudioError {
    AudioError::Device(format!("cannot read {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A WAV in the temp dir, deleted on drop.
    struct TempWav(PathBuf);

    impl TempWav {
        fn path() -> PathBuf {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            std::env::temp_dir().join(format!(
                "dictation-platform-{}-{}.wav",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ))
        }

        fn int16(sample_rate: u32, channels: u16, samples: &[i16]) -> Self {
            let path = Self::path();
            let spec = hound::WavSpec {
                channels,
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

        fn float(samples: &[f32]) -> Self {
            let path = Self::path();
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 48_000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut writer = hound::WavWriter::create(&path, spec).unwrap();
            for &sample in samples {
                writer.write_sample(sample).unwrap();
            }
            writer.finalize().unwrap();
            Self(path)
        }
    }

    impl Drop for TempWav {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn start_then_stop_returns_the_file_as_mono_samples() {
        let wav = TempWav::int16(16_000, 1, &[0, 16_384, -16_384, i16::MIN]);
        let mut source = FileAudioSource::new(&wav.0);
        assert!(!source.is_recording());

        source.start().unwrap();
        assert!(source.is_recording());
        let audio = source.stop().unwrap();
        assert!(!source.is_recording());
        assert_eq!(
            audio,
            Audio {
                sample_rate: 16_000,
                samples: vec![0.0, 0.5, -0.5, -1.0],
            }
        );
    }

    /// Waits up to a second for a reading other than the floor.
    fn sound_level(meter: &LevelMeter) -> Option<f32> {
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(10));
            match meter.take() {
                Some(level) if level > dictation_core::level::LEVEL_FLOOR_DBFS => {
                    return Some(level);
                }
                Some(_) => {}
                None => return None,
            }
        }
        None
    }

    /// U4: the fake source plays its level in real time while recording.
    #[test]
    fn criterion_u4_1_the_meter_plays_the_files_level() {
        // A 1 s, 16 kHz sine whose RMS is -20 dBFS (amplitude 0.1414).
        #[allow(clippy::cast_possible_truncation, reason = "test sine")]
        let sine: Vec<i16> = (0..16_000)
            .map(|i| {
                let t = f64::from(i) / 16_000.0;
                (0.141_42 * (t * 440.0 * std::f64::consts::TAU).sin() * 32_767.0) as i16
            })
            .collect();
        let wav = TempWav::int16(16_000, 1, &sine);
        let mut source = FileAudioSource::new(&wav.0);
        let meter = source.level_meter().unwrap();
        assert_eq!(meter.take(), None);
        source.start().unwrap();
        let level = sound_level(&meter).unwrap();
        assert!((level + 20.0).abs() <= 2.0, "{level}");
        source.stop().unwrap();
        assert_eq!(meter.take(), None);
    }

    /// U4: the file plays over and over while recording, like a microphone
    /// that keeps hearing it.
    #[test]
    fn criterion_u4_1_the_meter_loops_the_file() {
        let wav = TempWav::int16(16_000, 1, &[8_000; 320]);
        let mut source = FileAudioSource::new(&wav.0);
        let meter = source.level_meter().unwrap();
        source.start().unwrap();
        // Five times the file's 20 ms.
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = meter.take();
        assert!(sound_level(&meter).is_some());
        source.stop().unwrap();
        assert_eq!(meter.take(), None);
    }

    /// U4: a silent file reads the floor; after a discard nothing does.
    #[test]
    fn criterion_u4_1_a_silent_file_reads_the_floor() {
        let wav = TempWav::int16(16_000, 1, &[0; 1_600]);
        let mut source = FileAudioSource::new(&wav.0);
        let meter = source.level_meter().unwrap();
        source.start().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert_eq!(meter.take(), Some(dictation_core::level::LEVEL_FLOOR_DBFS));
        assert_eq!(sound_level(&meter), None);
        source.discard();
        assert_eq!(meter.take(), None);
    }

    #[test]
    fn stereo_is_mixed_down_to_mono() {
        let wav = TempWav::int16(44_100, 2, &[16_384, 0, -16_384, -16_384]);
        let mut source = FileAudioSource::new(&wav.0);
        source.start().unwrap();
        let audio = source.stop().unwrap();
        assert_eq!(audio.sample_rate, 44_100);
        assert_eq!(audio.samples, [0.25, -0.5]);
    }

    #[test]
    fn float_wavs_are_read_as_is() {
        let wav = TempWav::float(&[0.25, -0.75]);
        let mut source = FileAudioSource::new(&wav.0);
        source.start().unwrap();
        assert_eq!(source.stop().unwrap().samples, [0.25, -0.75]);
    }

    #[test]
    fn each_recording_rereads_the_file() {
        let wav = TempWav::int16(16_000, 1, &[0; 1_600]);
        let mut source = FileAudioSource::new(&wav.0);
        for _ in 0..2 {
            source.start().unwrap();
            let audio = source.stop().unwrap();
            assert_eq!(audio.duration(), std::time::Duration::from_millis(100));
        }
    }

    #[test]
    fn start_twice_is_already_recording() {
        let wav = TempWav::int16(16_000, 1, &[0]);
        let mut source = FileAudioSource::new(&wav.0);
        source.start().unwrap();
        assert_eq!(source.start(), Err(AudioError::AlreadyRecording));
        assert!(source.is_recording());
    }

    #[test]
    fn stop_without_start_is_not_recording() {
        let mut source = FileAudioSource::new("unused.wav");
        assert_eq!(source.stop(), Err(AudioError::NotRecording));
    }

    #[test]
    fn discard_drops_the_audio_and_never_fails() {
        let wav = TempWav::int16(16_000, 1, &[0]);
        let mut source = FileAudioSource::new(&wav.0);
        source.discard();
        source.start().unwrap();
        source.discard();
        assert!(!source.is_recording());
        assert_eq!(source.stop(), Err(AudioError::NotRecording));
    }

    #[test]
    fn missing_file_is_no_device_and_nothing_records() {
        let mut source = FileAudioSource::new(TempWav::path());
        assert_eq!(source.start(), Err(AudioError::NoDevice));
        assert!(!source.is_recording());
    }

    #[test]
    fn unreadable_file_is_a_device_error() {
        let path = TempWav::path();
        fs::write(&path, b"not a wav").unwrap();
        let _cleanup = TempWav(path.clone());
        let mut source = FileAudioSource::new(&path);
        assert!(matches!(source.start(), Err(AudioError::Device(_))));
        assert!(!source.is_recording());
    }

    /// A 16 kHz mono PCM WAV with a hand-written header, so it can claim
    /// sample widths hound won't write.
    fn raw_pcm_wav(bits_per_sample: u16, block_align: u16, data: &[u8]) -> TempWav {
        let mut bytes = Vec::new();
        let riff_len = u32::try_from(36 + data.len()).unwrap();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&riff_len.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&16_000u32.to_le_bytes());
        bytes.extend_from_slice(&(16_000 * u32::from(block_align)).to_le_bytes());
        bytes.extend_from_slice(&block_align.to_le_bytes());
        bytes.extend_from_slice(&bits_per_sample.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(data);
        let wav = TempWav(TempWav::path());
        fs::write(&wav.0, bytes).unwrap();
        wav
    }

    #[test]
    fn samples_wider_than_32_bits_are_a_device_error_not_a_panic() {
        let wav = raw_pcm_wav(64, 8, &[0; 16]);
        let mut source = FileAudioSource::new(&wav.0);
        assert!(matches!(source.start(), Err(AudioError::Device(_))));
        assert!(!source.is_recording());
    }

    #[test]
    fn zero_bit_samples_are_a_device_error_not_a_panic() {
        let wav = raw_pcm_wav(0, 1, &[0; 4]);
        let mut source = FileAudioSource::new(&wav.0);
        assert!(matches!(source.start(), Err(AudioError::Device(_))));
        assert!(!source.is_recording());
    }

    #[test]
    fn a_hand_written_16_bit_header_reads_like_hounds_own() {
        let wav = raw_pcm_wav(16, 2, &[0x00, 0x40, 0x00, 0xC0]);
        let mut source = FileAudioSource::new(&wav.0);
        source.start().unwrap();
        assert_eq!(source.stop().unwrap().samples, [0.5, -0.5]);
    }

    #[test]
    fn clones_share_the_recording() {
        let wav = TempWav::int16(16_000, 1, &[16_384]);
        let mut source = FileAudioSource::new(&wav.0);
        let mut handle = source.clone();

        source.start().unwrap();
        assert!(handle.is_recording());
        assert_eq!(handle.start(), Err(AudioError::AlreadyRecording));
        assert_eq!(handle.stop().unwrap().samples, [0.5]);
        assert!(!source.is_recording());
        assert_eq!(source.stop(), Err(AudioError::NotRecording));

        handle.start().unwrap();
        source.discard();
        assert!(!handle.is_recording());
    }
}
