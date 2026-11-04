//! Capturing audio from the microphone.

use thiserror::Error;

/// Captured mono audio: samples in `-1.0..=1.0` at `sample_rate` Hz.
///
/// Real sources deliver 16 kHz (what the transcribers want); fakes deliver
/// whatever rate their input has.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    /// Samples per second.
    pub sample_rate: u32,
    /// One sample per frame, mono.
    pub samples: Vec<f32>,
}

impl Audio {
    /// How long the audio lasts. Zero when `sample_rate` is zero.
    #[must_use]
    pub fn duration(&self) -> std::time::Duration {
        dictation_core::audio_math::duration(self.samples.len(), self.sample_rate)
    }
}

/// Why an [`AudioSource`] call failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AudioError {
    /// The OS hasn't granted microphone access.
    #[error("microphone permission is missing")]
    PermissionDenied,
    /// There is no input device (or the configured one is gone).
    #[error("no microphone found")]
    NoDevice,
    /// [`AudioSource::start`] was called while already recording.
    #[error("already recording")]
    AlreadyRecording,
    /// [`AudioSource::stop`] was called while not recording.
    #[error("not recording")]
    NotRecording,
    /// The device or input failed; a short description for the log.
    #[error("audio device error: {0}")]
    Device(String),
}

/// A source of microphone audio, driven by the coordinator's
/// `StartRecording`, `StopRecording` and `DiscardRecording` commands.
///
/// Threading: methods take `&mut self`, so one owner (the pipeline) drives
/// the source. Implementations capture on their own thread or OS callback and
/// must never block that callback. `start` and `stop` may block briefly
/// (tens of milliseconds: opening the device, or reading a file in the
/// file-backed fake), so callers on an async runtime run them on a
/// blocking thread or accept that short stall. Anything slower, such as a
/// Bluetooth route switch, implementations do on their own thread.
pub trait AudioSource: Send {
    /// Starts capturing.
    ///
    /// # Errors
    ///
    /// [`AudioError::PermissionDenied`] without microphone access,
    /// [`AudioError::NoDevice`] without an input device,
    /// [`AudioError::AlreadyRecording`] if a capture is running, and
    /// [`AudioError::Device`] if the device fails to start. On error nothing
    /// is recording.
    ///
    /// Threading: returns once the stream is running, without waiting for
    /// audio; may block briefly (see the trait docs).
    fn start(&mut self) -> Result<(), AudioError>;

    /// Stops capturing and returns everything captured since
    /// [`start`](Self::start).
    ///
    /// # Errors
    ///
    /// [`AudioError::NotRecording`] if no capture is running, and
    /// [`AudioError::Device`] if the device failed mid-capture. Either way,
    /// nothing is recording afterwards.
    ///
    /// Threading: drains what the capture thread buffered and returns; may
    /// block briefly (see the trait docs).
    fn stop(&mut self) -> Result<Audio, AudioError>;

    /// Stops capturing, if running, and drops the audio.
    ///
    /// Errors: none, so a cancel can't leave the microphone on. Threading:
    /// returns quickly.
    fn discard(&mut self);

    /// Whether a capture is running.
    ///
    /// Errors: none. Threading: never blocks.
    fn is_recording(&self) -> bool;

    /// The source's live input level (U4), if it measures one: a handle the
    /// source feeds while recording and the pipeline reads. `None` (the
    /// default) for sources that don't, such as file playback.
    ///
    /// Errors: none. Threading: never blocks; called once, before the
    /// source moves to its worker.
    fn level_meter(&self) -> Option<crate::LevelMeter> {
        None
    }
}

/// A boxed source is a source, so a caller can pick one at run time (the
/// shell: the microphone, or a file for a test hook).
impl<A: AudioSource + ?Sized> AudioSource for Box<A> {
    fn start(&mut self) -> Result<(), AudioError> {
        (**self).start()
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        (**self).stop()
    }

    fn discard(&mut self) {
        (**self).discard();
    }

    fn is_recording(&self) -> bool {
        (**self).is_recording()
    }

    fn level_meter(&self) -> Option<crate::LevelMeter> {
        (**self).level_meter()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn duration_follows_sample_rate() {
        let audio = Audio {
            sample_rate: 16_000,
            samples: vec![0.0; 8_000],
        };
        assert_eq!(audio.duration(), Duration::from_millis(500));
    }

    #[test]
    fn duration_is_zero_without_a_sample_rate() {
        let audio = Audio {
            sample_rate: 0,
            samples: vec![0.0; 10],
        };
        assert_eq!(audio.duration(), Duration::ZERO);
    }

    /// Records a fixed clip.
    struct OneClip(bool);

    impl AudioSource for OneClip {
        fn start(&mut self) -> Result<(), AudioError> {
            self.0 = true;
            Ok(())
        }

        fn stop(&mut self) -> Result<Audio, AudioError> {
            self.0 = false;
            Ok(Audio {
                sample_rate: 16_000,
                samples: vec![0.5],
            })
        }

        fn discard(&mut self) {
            self.0 = false;
        }

        fn is_recording(&self) -> bool {
            self.0
        }
    }

    #[test]
    fn a_boxed_source_forwards_every_call() {
        let mut audio: Box<dyn AudioSource> = Box::new(OneClip(false));
        audio.start().unwrap();
        assert!(audio.is_recording());
        assert_eq!(audio.stop().unwrap().samples, [0.5]);
        audio.start().unwrap();
        audio.discard();
        assert!(!audio.is_recording());
        assert!(audio.level_meter().is_none());
    }
}
