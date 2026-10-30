//! The system clipboard, used to paste text and restore what was there.

use thiserror::Error;

/// Why a [`Clipboard`] call failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ClipboardError {
    /// The clipboard couldn't be opened (another app holds it, on Windows).
    #[error("clipboard is unavailable: {0}")]
    Unavailable(String),
}

/// Plain-text access to the system clipboard.
///
/// Threading: `&self` methods, callable from any thread. Real
/// implementations may block briefly (Windows retries `OpenClipboard`), so
/// the pipeline calls them from a blocking task, not the async executor.
pub trait Clipboard: Send + Sync {
    /// The clipboard's text, or `None` if it holds no text (empty, or an
    /// image or files).
    ///
    /// # Errors
    ///
    /// [`ClipboardError::Unavailable`] if the clipboard can't be read.
    ///
    /// Threading: may block briefly; call from a blocking task.
    fn read_text(&self) -> Result<Option<String>, ClipboardError>;

    /// Replaces the clipboard's contents with `text`. Real implementations
    /// mark the write transient so clipboard managers skip it
    /// (`docs/ENGINEERING.md`).
    ///
    /// # Errors
    ///
    /// [`ClipboardError::Unavailable`] if the clipboard can't be written;
    /// its previous contents are then unchanged.
    ///
    /// Threading: may block briefly; call from a blocking task.
    fn write_text(&self, text: &str) -> Result<(), ClipboardError>;
}
