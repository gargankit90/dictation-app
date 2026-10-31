//! [`SystemMicrophone`]: this OS's microphone behind one name, so callers
//! (the Tauri shell) need no `cfg(target_os)` of their own.

use dictation_ports::{Audio, AudioError, AudioSource};

/// The microphone on this OS: [`MacMicrophone`](super::MacMicrophone) on
/// macOS. Where there's no adapter yet (Windows, Linux),
/// [`start`](AudioSource::start) fails with [`AudioError::NoDevice`].
#[derive(Default)]
pub struct SystemMicrophone {
    #[cfg(target_os = "macos")]
    inner: super::MacMicrophone,
}

impl SystemMicrophone {
    /// The default input device; nothing opens until started.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl AudioSource for SystemMicrophone {
    fn start(&mut self) -> Result<(), AudioError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.start()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(AudioError::NoDevice)
        }
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.stop()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(AudioError::NotRecording)
        }
    }

    fn discard(&mut self) {
        #[cfg(target_os = "macos")]
        self.inner.discard();
    }

    fn is_recording(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.inner.is_recording()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn no_device_without_an_adapter() {
        let mut mic = SystemMicrophone::new();
        assert_eq!(mic.start(), Err(AudioError::NoDevice));
        assert_eq!(mic.stop(), Err(AudioError::NotRecording));
        mic.discard();
        assert!(!mic.is_recording());
    }
}
