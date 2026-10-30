//! The global dictation hotkey and the cancel key.

use thiserror::Error;

/// A key event from a [`HotkeySource`]. Untimed: the pipeline stamps it with
/// its [`Clock`](crate::Clock) before handing it to the coordinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    /// The dictation hotkey went down. Key repeats aren't reported.
    Pressed,
    /// The dictation hotkey came up.
    Released,
    /// The cancel key (Esc) was pressed.
    CancelPressed,
}

/// Receives [`HotkeyEvent`]s. Called on the source's own thread (an OS event
/// tap or hook), so it must return quickly: send the event on a channel and
/// do the work elsewhere.
pub type HotkeySink = Box<dyn FnMut(HotkeyEvent) + Send>;

/// Why a [`HotkeySource`] call failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HotkeyError {
    /// The OS refused the keyboard hook (macOS: Accessibility or Input
    /// Monitoring not granted).
    #[error("keyboard access is not permitted")]
    PermissionDenied,
    /// [`HotkeySource::start`] was called while already listening.
    #[error("already listening for the hotkey")]
    AlreadyStarted,
    /// The hook couldn't be installed for another reason.
    #[error("hotkey unavailable: {0}")]
    Unavailable(String),
}

/// Global keyboard listening for the dictation hotkey and Esc.
///
/// Threading: one owner calls `start` and `stop` (`&mut self`). Events go to
/// the sink on the source's own thread, in the order they happened, from
/// `start` until `stop` returns.
pub trait HotkeySource: Send {
    /// Starts listening and sends every event to `sink`.
    ///
    /// # Errors
    ///
    /// [`HotkeyError::PermissionDenied`] if the OS refuses the hook,
    /// [`HotkeyError::AlreadyStarted`] if already listening, and
    /// [`HotkeyError::Unavailable`] otherwise. On error the sink is dropped
    /// and never called.
    ///
    /// Threading: returns once the hook is installed; the sink is then
    /// called on the source's own thread, never on the caller's.
    fn start(&mut self, sink: HotkeySink) -> Result<(), HotkeyError>;

    /// Stops listening and drops the sink; no events arrive after it
    /// returns.
    ///
    /// Errors: none; does nothing if not listening. Threading: may wait for
    /// an in-flight sink call to finish, so don't call it from inside the
    /// sink.
    fn stop(&mut self);
}
