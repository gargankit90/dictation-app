//! Secure Input: while a password field (or an app like Terminal with
//! "Secure Keyboard Entry") has it on, macOS hides key presses from event
//! taps. Modifier-only hotkeys keep working, hotkeys with a key stop
//! silently, so the app warns while it's on (docs/ENGINEERING.md).
//!
//! [`SecureInputWatcher`] polls a probe, since macOS sends no notification
//! when Secure Input turns on or off. On macOS the probe is
//! [`is_secure_input_enabled`].

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// A change in Secure Input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecureInputEvent {
    /// Secure Input turned on: show the warning.
    On,
    /// Secure Input turned off: clear the warning.
    Off,
}

/// How often the macOS watcher polls; a user won't notice a warning that
/// shows half a second late.
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Calls a callback whenever Secure Input changes, from its own thread,
/// until dropped. The first poll reports [`SecureInputEvent::On`] if it's
/// already on; "off" at start is not reported.
pub struct SecureInputWatcher {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl SecureInputWatcher {
    /// Polls `probe` every `interval` and sends each change to `on_change`.
    ///
    /// # Errors
    ///
    /// If the polling thread can't be spawned.
    pub fn start(
        probe: impl Fn() -> bool + Send + 'static,
        interval: Duration,
        mut on_change: impl FnMut(SecureInputEvent) + Send + 'static,
    ) -> std::io::Result<Self> {
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = thread::Builder::new()
            .name("secure-input".into())
            .spawn(move || {
                let mut on = false;
                loop {
                    let now = probe();
                    if now != on {
                        on = now;
                        on_change(if on {
                            SecureInputEvent::On
                        } else {
                            SecureInputEvent::Off
                        });
                    }
                    match stopped.recv_timeout(interval) {
                        Err(RecvTimeoutError::Timeout) => {}
                        Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })?;
        Ok(Self {
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// Watches the real Secure Input state every [`POLL_INTERVAL`].
    ///
    /// # Errors
    ///
    /// If the polling thread can't be spawned.
    #[cfg(target_os = "macos")]
    pub fn start_macos(
        on_change: impl FnMut(SecureInputEvent) + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::start(is_secure_input_enabled, POLL_INTERVAL, on_change)
    }
}

impl Drop for SecureInputWatcher {
    fn drop(&mut self) {
        // Dropping the sender wakes the thread at once.
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            // A panic in the callback already ended the thread.
            let _ = thread.join();
        }
    }
}

// SAFETY: these declarations match the C signatures in Carbon
// (HIToolbox); each call site's `unsafe` block upholds its contract.
#[cfg(target_os = "macos")]
#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}

/// Whether any process has Secure Input on right now.
#[cfg(target_os = "macos")]
#[must_use]
pub fn is_secure_input_enabled() -> bool {
    // SAFETY: IsSecureEventInputEnabled takes no arguments, only reads
    // system-wide state and may be called from any thread.
    unsafe { IsSecureEventInputEnabled() != 0 }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    const WAIT: Duration = Duration::from_secs(5);

    fn watch(on: &Arc<AtomicBool>) -> (SecureInputWatcher, mpsc::Receiver<SecureInputEvent>) {
        let (tx, rx) = mpsc::channel();
        let probe = Arc::clone(on);
        let watcher = SecureInputWatcher::start(
            move || probe.load(Ordering::SeqCst),
            Duration::from_millis(1),
            move |event| {
                let _ = tx.send(event);
            },
        )
        .unwrap();
        (watcher, rx)
    }

    #[test]
    fn reports_each_change_once() {
        let on = Arc::new(AtomicBool::new(false));
        let (_watcher, rx) = watch(&on);

        on.store(true, Ordering::SeqCst);
        assert_eq!(rx.recv_timeout(WAIT), Ok(SecureInputEvent::On));
        on.store(false, Ordering::SeqCst);
        assert_eq!(rx.recv_timeout(WAIT), Ok(SecureInputEvent::Off));
        on.store(true, Ordering::SeqCst);
        assert_eq!(rx.recv_timeout(WAIT), Ok(SecureInputEvent::On));
    }

    #[test]
    fn already_on_is_reported_at_start() {
        let on = Arc::new(AtomicBool::new(true));
        let (_watcher, rx) = watch(&on);
        assert_eq!(rx.recv_timeout(WAIT), Ok(SecureInputEvent::On));
    }

    #[test]
    fn nothing_after_drop() {
        let on = Arc::new(AtomicBool::new(false));
        let (watcher, rx) = watch(&on);
        drop(watcher);
        on.store(true, Ordering::SeqCst);
        // The callback (and its sender) went with the thread.
        assert_eq!(
            rx.recv_timeout(WAIT),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_probe_answers() {
        // Can't control the real state from a test; just make sure the call
        // links and returns.
        let _ = is_secure_input_enabled();
    }
}
