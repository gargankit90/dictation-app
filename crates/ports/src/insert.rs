//! Putting the transcript into the app that has focus.

use thiserror::Error;

/// Why a [`TextInserter::insert`] failed. The messages never contain the
/// text.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InsertError {
    /// The OS doesn't allow synthetic key presses (macOS Accessibility).
    #[error("accessibility permission is missing")]
    PermissionDenied,
    /// A password field has focus (Secure Input), so pasting is skipped.
    #[error("a password field has focus")]
    SecureInput,
    /// Pasting failed for another reason; a short description for the log.
    #[error("insert failed: {0}")]
    Failed(String),
}

/// Inserts text into the focused app, typically by writing the clipboard
/// and simulating Cmd+V / Ctrl+V.
///
/// Threading: callable from any thread. Real implementations block for a
/// few hundred milliseconds (paste, wait, restore the clipboard), so the
/// pipeline calls them from a blocking task, never the async executor.
pub trait TextInserter: Send + Sync {
    /// Inserts `text` at the cursor of the focused app. Leaves the user's
    /// clipboard as it was.
    ///
    /// # Errors
    ///
    /// [`InsertError::PermissionDenied`] without Accessibility,
    /// [`InsertError::SecureInput`] while a password field has focus, and
    /// [`InsertError::Failed`] otherwise. On error nothing was inserted, as
    /// far as the implementation can tell.
    ///
    /// Threading: blocks for a few hundred milliseconds; call from a
    /// blocking task.
    fn insert(&self, text: &str) -> Result<(), InsertError>;

    /// Finishes clipboard work still owed from earlier inserts before the
    /// app exits (#241: the user's clipboard goes back a short delay after
    /// a paste, on a timer the exit would otherwise kill), then refuses
    /// further inserts. Called once, when the pipeline shuts down. The
    /// default has nothing to finish.
    ///
    /// Threading: may block up to the implementation's restore delay; call
    /// from a blocking task.
    fn finish(&self) {}
}

impl TextInserter for Box<dyn TextInserter> {
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        (**self).insert(text)
    }

    fn finish(&self) {
        (**self).finish();
    }
}
