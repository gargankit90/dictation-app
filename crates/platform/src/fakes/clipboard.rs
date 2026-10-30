use std::sync::{Arc, Mutex};

use dictation_ports::{Clipboard, ClipboardError};

use super::lock;

/// A [`Clipboard`] held in memory.
#[derive(Debug, Clone, Default)]
pub struct MemClipboard {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    text: Option<String>,
    unavailable: bool,
}

impl MemClipboard {
    /// An empty clipboard.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A clipboard holding `text`, as if the user had copied it.
    #[must_use]
    pub fn with_text(text: impl Into<String>) -> Self {
        let clipboard = Self::new();
        lock(&clipboard.state).text = Some(text.into());
        clipboard
    }

    /// Makes every read and write fail with [`ClipboardError::Unavailable`]
    /// (another app holding the clipboard) until set back to `false`.
    pub fn set_unavailable(&self, unavailable: bool) {
        lock(&self.state).unavailable = unavailable;
    }
}

impl Clipboard for MemClipboard {
    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        let state = lock(&self.state);
        if state.unavailable {
            return Err(unavailable());
        }
        Ok(state.text.clone())
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        let mut state = lock(&self.state);
        if state.unavailable {
            return Err(unavailable());
        }
        state.text = Some(text.to_owned());
        Ok(())
    }
}

fn unavailable() -> ClipboardError {
    ClipboardError::Unavailable("held by another app".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let clipboard = MemClipboard::new();
        assert_eq!(clipboard.read_text().unwrap(), None);
        clipboard.write_text("dictated").unwrap();
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("dictated"));
    }

    #[test]
    fn write_replaces_what_the_user_copied_and_clones_share_it() {
        let clipboard = MemClipboard::with_text("user's text");
        let handle = clipboard.clone();
        assert_eq!(handle.read_text().unwrap().as_deref(), Some("user's text"));
        clipboard.write_text("dictated").unwrap();
        assert_eq!(handle.read_text().unwrap().as_deref(), Some("dictated"));
    }

    #[test]
    fn unavailable_fails_and_leaves_the_contents_unchanged() {
        let clipboard = MemClipboard::with_text("kept");
        clipboard.set_unavailable(true);
        assert!(matches!(
            clipboard.write_text("lost"),
            Err(ClipboardError::Unavailable(_))
        ));
        assert!(clipboard.read_text().is_err());

        clipboard.set_unavailable(false);
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("kept"));
    }
}
