//! What the tray shows next to its icon, driven by the pipeline's
//! [`UiEvent`]s: a dictation error until the next recording starts (T2), and
//! "Hotkey unavailable" for as long as the hotkey doesn't work (#88).
//!
//! [`TrayStatus`] is pure; the shell applies each [`TrayView`] it returns to
//! the tray icon's title and tooltip.

use dictation_core::coordinator::Phase;
use dictation_core::error::DictationError;

use crate::pipeline::UiEvent;

/// The tray's title while the hotkey doesn't work.
pub const HOTKEY_UNAVAILABLE: &str = "Hotkey unavailable";

/// What the tray shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrayView {
    /// The text next to the icon, or none.
    pub title: Option<String>,
    /// The detail for the tooltip, after the app's name, or none.
    pub detail: Option<String>,
}

/// The tray's state.
#[derive(Debug, Default)]
pub struct TrayStatus {
    /// The dictation error showing, if any (not the hotkey's).
    error: Option<String>,
    /// Whether the hotkey doesn't work.
    hotkey_down: bool,
    view: TrayView,
}

impl TrayStatus {
    /// What shows now.
    #[must_use]
    pub fn view(&self) -> &TrayView {
        &self.view
    }

    /// Applies a pipeline event; `None` if what shows doesn't change.
    ///
    /// A dictation error shows until the next recording starts. While the
    /// hotkey doesn't work ([`DictationError::HotkeyFailed`] until
    /// [`UiEvent::HotkeyRestored`]) the title says
    /// [`HOTKEY_UNAVAILABLE`], unless a newer dictation error shows; once
    /// that clears, it says so again.
    pub fn on_event(&mut self, event: &UiEvent) -> Option<TrayView> {
        match event {
            UiEvent::Error(DictationError::HotkeyFailed) => {
                self.hotkey_down = true;
                self.error = None;
            }
            UiEvent::Error(error) => self.error = Some(error.to_string()),
            UiEvent::HotkeyRestored => self.hotkey_down = false,
            UiEvent::StateChanged(Phase::Recording) => self.error = None,
            UiEvent::StateChanged(_) => return None,
        }
        let view = match (&self.error, self.hotkey_down) {
            (Some(error), _) => TrayView {
                title: Some(error.clone()),
                detail: Some(error.clone()),
            },
            (None, true) => TrayView {
                title: Some(HOTKEY_UNAVAILABLE.to_owned()),
                detail: Some(DictationError::HotkeyFailed.to_string()),
            },
            (None, false) => TrayView::default(),
        };
        if view == self.view {
            return None;
        }
        self.view = view.clone();
        Some(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_key() -> UiEvent {
        UiEvent::Error(DictationError::TranscriptionFailed {
            reason: "Add your API key in Settings.".into(),
        })
    }

    fn title(view: Option<TrayView>) -> Option<Option<String>> {
        view.map(|view| view.title)
    }

    #[test]
    fn starts_with_nothing() {
        assert_eq!(TrayStatus::default().view(), &TrayView::default());
    }

    #[test]
    fn a_dictation_error_shows_until_the_next_recording() {
        let mut tray = TrayStatus::default();
        let shown = tray.on_event(&no_key()).unwrap();
        assert_eq!(shown.title.as_deref(), Some(no_key_text().as_str()));
        assert_eq!(shown.detail, shown.title);
        assert_eq!(tray.on_event(&UiEvent::StateChanged(Phase::Idle)), None);
        assert_eq!(
            tray.on_event(&UiEvent::StateChanged(Phase::Recording)),
            Some(TrayView::default())
        );
        assert_eq!(
            tray.on_event(&UiEvent::StateChanged(Phase::Recording)),
            None
        );
    }

    fn no_key_text() -> String {
        match no_key() {
            UiEvent::Error(error) => error.to_string(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn hotkey_unavailable_stays_until_it_is_restored() {
        let mut tray = TrayStatus::default();
        let shown = tray
            .on_event(&UiEvent::Error(DictationError::HotkeyFailed))
            .unwrap();
        assert_eq!(shown.title.as_deref(), Some(HOTKEY_UNAVAILABLE));
        assert_eq!(shown.detail, Some(DictationError::HotkeyFailed.to_string()));
        // A recording (the test hooks still work) doesn't clear it.
        assert_eq!(
            tray.on_event(&UiEvent::StateChanged(Phase::Recording)),
            None
        );
        // Shown twice is no change.
        assert_eq!(
            tray.on_event(&UiEvent::Error(DictationError::HotkeyFailed)),
            None
        );
        assert_eq!(
            tray.on_event(&UiEvent::HotkeyRestored),
            Some(TrayView::default())
        );
        assert_eq!(tray.on_event(&UiEvent::HotkeyRestored), None);
    }

    #[test]
    fn a_newer_error_shows_over_it_and_it_comes_back_after() {
        let mut tray = TrayStatus::default();
        tray.on_event(&UiEvent::Error(DictationError::HotkeyFailed));
        assert_eq!(title(tray.on_event(&no_key())), Some(Some(no_key_text())));
        assert_eq!(
            title(tray.on_event(&UiEvent::StateChanged(Phase::Recording))),
            Some(Some(HOTKEY_UNAVAILABLE.to_owned()))
        );
    }

    #[test]
    fn restored_keeps_a_dictation_error_that_shows() {
        let mut tray = TrayStatus::default();
        tray.on_event(&UiEvent::Error(DictationError::HotkeyFailed));
        tray.on_event(&no_key());
        assert_eq!(tray.on_event(&UiEvent::HotkeyRestored), None);
        assert_eq!(tray.view().title, Some(no_key_text()));
        // The hotkey failing again replaces it: that's what matters now.
        assert_eq!(
            title(tray.on_event(&UiEvent::Error(DictationError::HotkeyFailed))),
            Some(Some(HOTKEY_UNAVAILABLE.to_owned()))
        );
    }
}
