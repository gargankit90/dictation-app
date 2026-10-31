//! [`PermissionTolerantHotkey`]: the hotkey a release build runs, where a
//! missing Accessibility grant must not stop the app from starting.

use dictation_core::error::DictationError;
use dictation_ports::{HotkeyError, HotkeySink, HotkeySource, Permission};

/// Wraps a [`HotkeySource`] so that [`HotkeyError::PermissionDenied`] on
/// [`start`](HotkeySource::start) is logged as `PermissionMissing` and the
/// app keeps running without a hotkey (the user can grant Accessibility and
/// restart). Every other error, [`HotkeyError::Unavailable`] included, is
/// returned unchanged so the caller reports it.
pub struct PermissionTolerantHotkey<H> {
    inner: H,
    missing: bool,
}

impl<H: HotkeySource> PermissionTolerantHotkey<H> {
    /// Wraps `inner`.
    pub fn new(inner: H) -> Self {
        Self {
            inner,
            missing: false,
        }
    }

    /// Whether the last `start` found Accessibility missing.
    #[must_use]
    pub fn permission_missing(&self) -> bool {
        self.missing
    }
}

impl<H: HotkeySource> HotkeySource for PermissionTolerantHotkey<H> {
    fn start(&mut self, sink: HotkeySink) -> Result<(), HotkeyError> {
        self.missing = false;
        match self.inner.start(sink) {
            Err(HotkeyError::PermissionDenied) => {
                self.missing = true;
                tracing::warn!(
                    error = %DictationError::PermissionMissing(Permission::Accessibility),
                    "PermissionMissing: the hotkey is off until Accessibility is granted and the app restarts"
                );
                Ok(())
            }
            other => other,
        }
    }

    fn stop(&mut self) {
        self.inner.stop();
    }
}

#[cfg(test)]
mod tests {
    use dictation_platform::fakes::FakeHotkey;
    use dictation_ports::HotkeyEvent;

    use super::*;

    #[test]
    fn missing_permission_is_not_fatal() {
        let fake = FakeHotkey::new();
        fake.fail_next_start(HotkeyError::PermissionDenied);
        let mut hotkey = PermissionTolerantHotkey::new(fake.clone());
        assert_eq!(hotkey.start(Box::new(|_| {})), Ok(()));
        assert!(hotkey.permission_missing());
        assert!(!fake.is_listening());
    }

    #[test]
    fn other_errors_are_reported() {
        for error in [
            HotkeyError::Unavailable("no adapter".into()),
            HotkeyError::AlreadyStarted,
        ] {
            let fake = FakeHotkey::new();
            fake.fail_next_start(error.clone());
            let mut hotkey = PermissionTolerantHotkey::new(fake);
            assert_eq!(hotkey.start(Box::new(|_| {})), Err(error));
            assert!(!hotkey.permission_missing());
        }
    }

    #[test]
    fn a_working_source_passes_events_through() {
        let fake = FakeHotkey::new();
        let mut hotkey = PermissionTolerantHotkey::new(fake.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        hotkey
            .start(Box::new(move |event| {
                let _ = tx.send(event);
            }))
            .unwrap();
        assert!(fake.emit(HotkeyEvent::Pressed));
        hotkey.stop();
        assert!(!fake.is_listening());
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [HotkeyEvent::Pressed]);
    }
}
