//! Debug mode only: [`SavingAudio`] saves every recording as a WAV file so
//! it can be played back. Outside debug mode audio never touches the disk
//! (docs/ENGINEERING.md); the shell wraps its source in this only in a
//! debug build launched with `--debug-save-audio`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use dictation_core::wav;
use dictation_ports::{Audio, AudioError, AudioSource};

/// An [`AudioSource`] that writes each finished recording to
/// `<dir>/recording-<unix ms>.wav` (16-bit PCM at the recording's rate)
/// before returning it. A file that can't be written is logged and skipped;
/// the dictation goes on. Discarded recordings aren't saved.
pub struct SavingAudio<A> {
    inner: A,
    dir: PathBuf,
}

impl<A> SavingAudio<A> {
    /// Saves `inner`'s recordings into `dir`, created when first needed.
    pub fn new(inner: A, dir: impl Into<PathBuf>) -> Self {
        Self {
            inner,
            dir: dir.into(),
        }
    }
}

impl<A: AudioSource> AudioSource for SavingAudio<A> {
    fn start(&mut self) -> Result<(), AudioError> {
        self.inner.start()
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        let audio = self.inner.stop()?;
        match save(&self.dir, &audio) {
            Ok(path) => tracing::info!(path = %path.display(), "debug mode: saved the recording"),
            Err(error) => tracing::warn!(%error, "debug mode: could not save the recording"),
        }
        Ok(audio)
    }

    fn discard(&mut self) {
        self.inner.discard();
    }

    fn is_recording(&self) -> bool {
        self.inner.is_recording()
    }

    fn level_meter(&self) -> Option<dictation_ports::LevelMeter> {
        self.inner.level_meter()
    }
}

fn save(dir: &Path, audio: &Audio) -> Result<PathBuf, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis());
    save_at(dir, audio, millis)
}

/// [`save`] with the time given, so a test can make names collide.
fn save_at(dir: &Path, audio: &Audio, millis: u128) -> Result<PathBuf, String> {
    let bytes = wav::encode_f32(&audio.samples, audio.sample_rate).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // Two stops in the same millisecond get -1, -2, …
    let mut path = dir.join(format!("recording-{millis}.wav"));
    let mut n = 0;
    while path.exists() {
        n += 1;
        path = dir.join(format!("recording-{millis}-{n}.wav"));
    }
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A path in the temp dir, removed (file or directory) on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            Self(std::env::temp_dir().join(format!(
                "dictation-debug-audio-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }

        fn files(&self, sub: &str) -> Vec<PathBuf> {
            let mut files: Vec<_> = std::fs::read_dir(self.0.join(sub))
                .map(|dir| dir.map(|entry| entry.unwrap().path()).collect())
                .unwrap_or_default();
            files.sort();
            files
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// Records a fixed clip, or fails to stop.
    struct Clip {
        recording: bool,
        fail: bool,
    }

    impl AudioSource for Clip {
        fn start(&mut self) -> Result<(), AudioError> {
            self.recording = true;
            Ok(())
        }

        fn stop(&mut self) -> Result<Audio, AudioError> {
            self.recording = false;
            if self.fail {
                return Err(AudioError::NoDevice);
            }
            Ok(Audio {
                sample_rate: 16_000,
                samples: vec![0.0, 0.5, -0.5],
            })
        }

        fn discard(&mut self) {
            self.recording = false;
        }

        fn is_recording(&self) -> bool {
            self.recording
        }
    }

    fn source(dir: &TempDir, fail: bool) -> SavingAudio<Clip> {
        SavingAudio::new(
            Clip {
                recording: false,
                fail,
            },
            dir.0.join("recordings"),
        )
    }

    /// A [`Clip`] with a level meter.
    struct Metered(Clip, dictation_ports::LevelMeter);

    impl AudioSource for Metered {
        fn start(&mut self) -> Result<(), AudioError> {
            self.0.start()
        }

        fn stop(&mut self) -> Result<Audio, AudioError> {
            self.0.stop()
        }

        fn discard(&mut self) {
            self.0.discard();
        }

        fn is_recording(&self) -> bool {
            self.0.is_recording()
        }

        fn level_meter(&self) -> Option<dictation_ports::LevelMeter> {
            Some(self.1.clone())
        }
    }

    /// U4: saving the recordings keeps the microphone's level meter.
    #[test]
    fn the_level_meter_passes_through() {
        let dir = TempDir::new();
        assert!(source(&dir, false).level_meter().is_none());
        let meter = dictation_ports::LevelMeter::new();
        let saving = SavingAudio::new(
            Metered(
                Clip {
                    recording: false,
                    fail: false,
                },
                meter.clone(),
            ),
            dir.0.join("recordings"),
        );
        meter.begin(Some("Mic".into()));
        assert_eq!(
            saving.level_meter().unwrap().device().as_deref(),
            Some("Mic")
        );
    }

    #[test]
    fn criterion_3_each_recording_is_saved_as_a_playable_wav() {
        let dir = TempDir::new();
        let mut audio = source(&dir, false);
        for _ in 0..2 {
            audio.start().unwrap();
            assert!(audio.is_recording());
            assert_eq!(audio.stop().unwrap().samples, [0.0, 0.5, -0.5]);
        }
        let files = dir.files("recordings");
        assert_eq!(files.len(), 2, "{files:?}");
        for file in files {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                name.starts_with("recording-") && name.ends_with(".wav"),
                "{name}"
            );
            let reader = hound::WavReader::open(&file).unwrap();
            assert_eq!(reader.spec().sample_rate, 16_000);
            let samples: Vec<i16> = reader.into_samples().map(Result::unwrap).collect();
            assert_eq!(samples, [0, 16_384, -16_384]);
        }
    }

    #[test]
    fn discarded_and_failed_recordings_are_not_saved() {
        let dir = TempDir::new();
        let mut audio = source(&dir, false);
        audio.start().unwrap();
        audio.discard();
        assert!(!audio.is_recording());

        let mut failing = source(&dir, true);
        failing.start().unwrap();
        assert_eq!(failing.stop(), Err(AudioError::NoDevice));
        assert!(dir.files("recordings").is_empty());
    }

    #[test]
    fn an_unwritable_directory_still_returns_the_audio() {
        let dir = TempDir::new();
        // A file where the directory's parent should be.
        std::fs::write(&dir.0, b"a file, not a directory").unwrap();
        let mut audio = source(&dir, false);
        audio.start().unwrap();
        assert_eq!(audio.stop().unwrap().samples.len(), 3);
    }

    #[test]
    fn audio_that_cannot_be_encoded_is_skipped() {
        let dir = TempDir::new();
        let audio = Audio {
            sample_rate: 0,
            samples: vec![],
        };
        assert!(save(&dir.0, &audio).is_err());
        assert!(!dir.0.exists());
    }

    /// P2-03: at one timestamp, with the base name and the first suffixes
    /// already taken, the next free suffix is used, nothing is overwritten
    /// and every file decodes. It does not depend on how fast the disk is.
    #[test]
    fn same_timestamp_occupied_suffixes_produce_unique_recordings() {
        let dir = TempDir::new();
        std::fs::create_dir_all(&dir.0).unwrap();
        let taken = [
            "recording-1234.wav",
            "recording-1234-1.wav",
            "recording-1234-2.wav",
        ];
        for name in taken {
            std::fs::write(dir.0.join(name), format!("old {name}")).unwrap();
        }
        let audio = Audio {
            sample_rate: 16_000,
            samples: vec![0.0, 0.5],
        };

        // In a thread with a bound: a loop that never finds a free name
        // fails the test instead of hanging it.
        let (tx, rx) = std::sync::mpsc::channel();
        let path_dir = dir.0.clone();
        std::thread::spawn(move || {
            let paths: Vec<_> = (0..3)
                .map(|_| save_at(&path_dir, &audio, 1234).unwrap())
                .collect();
            let _ = tx.send(paths);
        });
        let paths = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("save_at never found a free name");
        let names: Vec<_> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "recording-1234-3.wav",
                "recording-1234-4.wav",
                "recording-1234-5.wav"
            ]
        );
        for name in taken {
            assert_eq!(
                std::fs::read_to_string(dir.0.join(name)).unwrap(),
                format!("old {name}"),
                "{name} was overwritten"
            );
        }
        for path in &paths {
            let reader = hound::WavReader::open(path).unwrap();
            assert_eq!(reader.spec().sample_rate, 16_000);
            assert_eq!(reader.len(), 2);
        }
    }

    #[test]
    fn names_never_collide() {
        let dir = TempDir::new();
        let audio = Audio {
            sample_rate: 16_000,
            samples: vec![0.0],
        };
        let paths: Vec<_> = (0..5).map(|_| save(&dir.0, &audio).unwrap()).collect();
        assert_eq!(dir.files("").len(), 5, "{paths:?}");
    }
}
