use std::sync::{Arc, Mutex};

use dictation_ports::{Clipboard, ClipboardError, ClipboardMarkers, SessionMarker};

use super::lock;

/// A [`Clipboard`] held in memory. It records the markers and session
/// marker of what's on it, so tests can check a paste flow marks its writes
/// and restores only its own.
#[derive(Debug, Clone, Default)]
pub struct MemClipboard {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    text: Option<String>,
    markers: ClipboardMarkers,
    session: Option<SessionMarker>,
    sessions_made: u64,
    unavailable: bool,
}

impl State {
    fn set(
        &mut self,
        text: Option<String>,
        markers: ClipboardMarkers,
        session: Option<SessionMarker>,
    ) {
        self.text = text;
        self.markers = markers;
        self.session = session;
    }
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
        clipboard.user_copies(text);
        clipboard
    }

    /// The user copies `text` (in another app): no markers, no session.
    /// Works even while unavailable, like another app holding the clipboard.
    pub fn user_copies(&self, text: impl Into<String>) {
        lock(&self.state).set(Some(text.into()), ClipboardMarkers::NONE, None);
    }

    /// The markers on what's on the clipboard now.
    #[must_use]
    pub fn markers(&self) -> ClipboardMarkers {
        lock(&self.state).markers
    }

    /// The session marker on the clipboard now, if a dictation write is
    /// still there.
    #[must_use]
    pub fn session(&self) -> Option<SessionMarker> {
        lock(&self.state).session.clone()
    }

    /// Makes every read and write fail with [`ClipboardError::Unavailable`]
    /// (another app holding the clipboard) until set back to `false`.
    pub fn set_unavailable(&self, unavailable: bool) {
        lock(&self.state).unavailable = unavailable;
    }

    fn with_access<T>(&self, f: impl FnOnce(&mut State) -> T) -> Result<T, ClipboardError> {
        let mut state = lock(&self.state);
        if state.unavailable {
            return Err(ClipboardError::Unavailable("held by another app".into()));
        }
        Ok(f(&mut state))
    }
}

impl Clipboard for MemClipboard {
    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        self.with_access(|state| state.text.clone())
    }

    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        self.with_access(|state| state.set(Some(text.to_owned()), ClipboardMarkers::NONE, None))
    }

    fn write_dictation(&self, text: &str) -> Result<SessionMarker, ClipboardError> {
        self.with_access(|state| {
            state.sessions_made += 1;
            let session = SessionMarker::new(format!("mem-session-{}", state.sessions_made));
            state.set(
                Some(text.to_owned()),
                ClipboardMarkers::DICTATION,
                Some(session.clone()),
            );
            session
        })
    }

    fn holds_dictation(&self, session: &SessionMarker) -> bool {
        lock(&self.state).session.as_ref() == Some(session)
    }

    fn restore_if_unchanged(
        &self,
        session: &SessionMarker,
        previous: Option<&str>,
    ) -> Result<bool, ClipboardError> {
        self.with_access(|state| {
            if state.session.as_ref() != Some(session) {
                return false;
            }
            state.set(previous.map(str::to_owned), ClipboardMarkers::NONE, None);
            true
        })
    }
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
        assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
        assert_eq!(clipboard.session(), None);
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
    fn dictation_writes_carry_all_three_markers_and_a_new_session() {
        let clipboard = MemClipboard::with_text("user's text");
        assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);

        let first = clipboard.write_dictation("hello").unwrap();
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("hello"));
        assert_eq!(clipboard.markers(), ClipboardMarkers::DICTATION);
        assert_eq!(clipboard.session(), Some(first.clone()));

        let second = clipboard.write_dictation("hello").unwrap();
        assert_ne!(first, second, "each write gets its own session");
        assert_eq!(clipboard.session(), Some(second));
    }

    #[test]
    fn restore_puts_back_the_users_text_without_markers() {
        let clipboard = MemClipboard::with_text("user's text");
        let session = clipboard.write_dictation("hello").unwrap();
        assert_eq!(
            clipboard.restore_if_unchanged(&session, Some("user's text")),
            Ok(true)
        );
        assert_eq!(
            clipboard.read_text().unwrap().as_deref(),
            Some("user's text")
        );
        assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
        assert_eq!(clipboard.session(), None);
    }

    #[test]
    fn restore_of_an_empty_clipboard_clears_it() {
        let clipboard = MemClipboard::new();
        let session = clipboard.write_dictation("hello").unwrap();
        assert_eq!(clipboard.restore_if_unchanged(&session, None), Ok(true));
        assert_eq!(clipboard.read_text(), Ok(None));
        assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
    }

    #[test]
    fn restore_is_skipped_when_the_user_copied_in_between() {
        let clipboard = MemClipboard::with_text("old");
        let session = clipboard.write_dictation("hello").unwrap();
        // Even identical text: only the session marker counts.
        clipboard.user_copies("hello");
        assert_eq!(clipboard.session(), None);
        assert_eq!(
            clipboard.restore_if_unchanged(&session, Some("old")),
            Ok(false)
        );
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("hello"));
    }

    #[test]
    fn restore_is_skipped_after_a_newer_dictation() {
        let clipboard = MemClipboard::with_text("old");
        let first = clipboard.write_dictation("one").unwrap();
        clipboard.write_dictation("two").unwrap();
        assert_eq!(
            clipboard.restore_if_unchanged(&first, Some("old")),
            Ok(false)
        );
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("two"));
    }

    #[test]
    fn unavailable_fails_and_leaves_the_contents_unchanged() {
        let clipboard = MemClipboard::with_text("kept");
        let session = clipboard.write_dictation("hello").unwrap();
        clipboard.set_unavailable(true);
        assert!(matches!(
            clipboard.write_text("lost"),
            Err(ClipboardError::Unavailable(_))
        ));
        assert!(clipboard.write_dictation("lost").is_err());
        assert!(
            clipboard
                .restore_if_unchanged(&session, Some("kept"))
                .is_err()
        );
        assert!(clipboard.read_text().is_err());

        clipboard.set_unavailable(false);
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("hello"));
        assert_eq!(clipboard.session(), Some(session));
    }
}
