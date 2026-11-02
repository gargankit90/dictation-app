//! Reading what is just before the cursor in the focused app, for the smart
//! leading space (`dictation_core::text::clean`).
//!
//! # Privacy
//!
//! This reads text from another app. An implementation reads at most a few
//! characters before the cursor, keeps nothing, never logs what it read at
//! any level and never sends it anywhere. [`BeforeCursor`]'s `Debug` hides
//! the character, and [`CursorReadError`]'s messages never contain text.

use std::fmt;

use thiserror::Error;

/// What is just before the cursor (or the selection) in the focused text
/// field.
///
/// `Debug` never shows the character.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BeforeCursor {
    /// The cursor is at the start of the field, or the field is empty.
    StartOfField,
    /// The character just before the cursor.
    Char(char),
}

impl BeforeCursor {
    /// The character before the cursor, as `dictation_core::text::clean`
    /// takes it: `None` at the start of a field.
    #[must_use]
    pub fn preceding(self) -> Option<char> {
        match self {
            Self::StartOfField => None,
            Self::Char(c) => Some(c),
        }
    }
}

impl fmt::Debug for BeforeCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StartOfField => f.write_str("StartOfField"),
            Self::Char(_) => f.write_str("Char(..)"),
        }
    }
}

/// Why [`TextBeforeCursor::char_before_cursor`] found nothing. Every case
/// means "unknown": the caller adds no space. The messages never contain
/// text from the app.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CursorReadError {
    /// No text field has focus, or the focused app doesn't expose its text
    /// or cursor.
    #[error("the focused app exposes no text cursor")]
    Unavailable,
    /// A password field has focus (or Secure Input is on): nothing is read.
    #[error("a password field has focus")]
    SecureField,
    /// The OS doesn't let this app read other apps' UI (macOS
    /// Accessibility).
    #[error("accessibility permission is missing")]
    PermissionDenied,
    /// The focused app didn't answer in time.
    #[error("the focused app did not answer in time")]
    TimedOut,
    /// Reading failed for another reason; a short description for the log,
    /// never the text.
    #[error("reading the cursor failed: {0}")]
    Failed(String),
}

/// Reads the character before the cursor in the focused app.
///
/// Threading: callable from any thread. Real implementations ask the
/// focused app over IPC and can block until their own timeout, so the
/// pipeline calls them from a blocking task and bounds the wait.
pub trait TextBeforeCursor: Send + Sync {
    /// The character before the cursor in the focused text field, or
    /// [`BeforeCursor::StartOfField`].
    ///
    /// # Errors
    ///
    /// [`CursorReadError::SecureField`] for a password field,
    /// [`CursorReadError::PermissionDenied`] without Accessibility,
    /// [`CursorReadError::TimedOut`] for an app that doesn't answer,
    /// [`CursorReadError::Unavailable`] if there is no text cursor to read,
    /// [`CursorReadError::Failed`] otherwise.
    ///
    /// Threading: may block (bounded by the implementation's own timeout);
    /// call from a blocking task.
    fn char_before_cursor(&self) -> Result<BeforeCursor, CursorReadError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_character() {
        let shown = format!("{:?}", BeforeCursor::Char('Z'));
        assert_eq!(shown, "Char(..)");
        assert!(!shown.contains('Z'));
        assert_eq!(format!("{:?}", BeforeCursor::StartOfField), "StartOfField");
        assert_eq!(
            format!("{:#?}", Ok::<_, CursorReadError>(BeforeCursor::Char('Z'))),
            "Ok(\n    Char(..),\n)"
        );
    }

    #[test]
    fn preceding_is_none_at_the_start_of_a_field() {
        assert_eq!(BeforeCursor::StartOfField.preceding(), None);
        assert_eq!(BeforeCursor::Char('a').preceding(), Some('a'));
    }

    #[test]
    fn error_messages_are_fixed_text() {
        assert_eq!(
            CursorReadError::SecureField.to_string(),
            "a password field has focus"
        );
        assert_eq!(
            CursorReadError::Failed("AXError -25204".into()).to_string(),
            "reading the cursor failed: AXError -25204"
        );
    }
}
