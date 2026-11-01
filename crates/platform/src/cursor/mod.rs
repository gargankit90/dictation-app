//! The character before the cursor in the focused app (the
//! [`TextBeforeCursor`] port), for the smart leading space (#93).
//!
//! On macOS, [`SystemTextBeforeCursor`] asks the focused element over the
//! Accessibility API (see the `macos` module for exactly what it reads).
//! Elsewhere (until the Windows milestone) it always answers
//! [`CursorReadError::Unavailable`], so no space is added.
//!
//! Privacy (see the port's docs): at most two UTF-16 units before the cursor
//! are copied out, nothing is kept or logged, and a password field is never
//! read.

#[cfg(target_os = "macos")]
mod macos;

use dictation_ports::{BeforeCursor, CursorReadError, TextBeforeCursor};

/// This OS's reader of the text before the cursor.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemTextBeforeCursor;

impl SystemTextBeforeCursor {
    /// The reader.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl TextBeforeCursor for SystemTextBeforeCursor {
    fn char_before_cursor(&self) -> Result<BeforeCursor, CursorReadError> {
        #[cfg(target_os = "macos")]
        {
            macos::char_before_cursor()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(CursorReadError::Unavailable)
        }
    }
}

// No test calls the real reader: it would read whatever text field has
// focus on the developer's machine, and its timing depends on that app. The
// macos module tests its pure parts; the VM scenario e2e_smart_space tests
// the real read.
