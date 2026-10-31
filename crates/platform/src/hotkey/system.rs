//! [`SystemHotkey`]: this OS's hotkey adapter behind one name, so callers
//! (the Tauri shell) need no `cfg(target_os)` of their own.

use dictation_core::hotkey_spec::HotkeySpec;
use dictation_ports::{HotkeyError, HotkeySink, HotkeySource};

/// The global hotkey on this OS: [`MacHotkey`](super::MacHotkey) on macOS.
/// Where there's no adapter yet (Windows until M-W.1, Linux),
/// [`start`](HotkeySource::start) fails with
/// [`HotkeyError::Unavailable`].
pub struct SystemHotkey {
    #[cfg(target_os = "macos")]
    inner: super::MacHotkey,
}

impl SystemHotkey {
    /// The hotkey for `spec`; it listens only once started.
    #[must_use]
    pub fn new(spec: HotkeySpec) -> Self {
        #[cfg(target_os = "macos")]
        {
            Self {
                inner: super::MacHotkey::new(spec),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = spec;
            Self {}
        }
    }
}

impl HotkeySource for SystemHotkey {
    fn start(&mut self, sink: HotkeySink) -> Result<(), HotkeyError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.start(sink)
        }
        #[cfg(not(target_os = "macos"))]
        {
            drop(sink);
            Err(HotkeyError::Unavailable(
                "no hotkey adapter for this OS yet".into(),
            ))
        }
    }

    fn stop(&mut self) {
        #[cfg(target_os = "macos")]
        self.inner.stop();
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn unavailable_without_an_adapter() {
        let mut hotkey = SystemHotkey::new(HotkeySpec::default());
        assert!(matches!(
            hotkey.start(Box::new(|_| {})),
            Err(HotkeyError::Unavailable(_))
        ));
        hotkey.stop();
    }
}
