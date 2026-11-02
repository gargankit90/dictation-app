//! What the tray shows next to its icon, driven by the pipeline's
//! [`UiEvent`]s: a dictation error until the next recording starts (T2),
//! "Hotkey unavailable" for as long as the hotkey doesn't work (#88), and a
//! Relaunch item in its menu while the microphone is denied (M5.7, #161).
//!
//! [`TrayStatus`] is pure; the shell applies each [`TrayView`] it returns to
//! the tray icon's title, tooltip and menu.

use dictation_core::coordinator::Phase;
use dictation_core::error::{DictationError, Permission};

use crate::pipeline::UiEvent;

/// The tray's title while a dictation error shows: short and the same for
/// every error, so the status item always fits in the menu bar (#151). A
/// long title made macOS hide the item, and with it the way to Settings. The
/// whole message is the tooltip's detail (and the overlay shows it).
pub const DICTATION_FAILED: &str = "\u{26a0} Dictation failed";

/// The tray's title while the hotkey doesn't work; as short as
/// [`DICTATION_FAILED`], for the same reason.
pub const HOTKEY_UNAVAILABLE: &str = "Hotkey unavailable";

/// The longest a tray title may be, in characters (#151).
pub const MAX_TITLE_CHARS: usize = 20;

/// What the tray shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrayView {
    /// The text next to the icon, or none.
    pub title: Option<String>,
    /// The detail for the tooltip, after the app's name, or none.
    pub detail: Option<String>,
    /// Whether the menu offers Relaunch: the microphone is denied, and a
    /// grant made in System Settings reaches the app only after a relaunch
    /// (`docs/ENGINEERING.md`).
    pub relaunch: bool,
}

/// The tray's state.
#[derive(Debug, Default)]
pub struct TrayStatus {
    /// The dictation error showing, if any (not the hotkey's).
    error: Option<String>,
    /// Whether the hotkey doesn't work.
    hotkey_down: bool,
    /// Whether the microphone is denied, as far as this run of the app
    /// knows.
    microphone_denied: bool,
    view: TrayView,
}

impl TrayStatus {
    /// The tray at launch: `microphone_denied` if the OS already reports the
    /// microphone refused, so Relaunch is offered before the first press.
    #[must_use]
    pub fn new(microphone_denied: bool) -> Self {
        Self {
            microphone_denied,
            view: TrayView {
                relaunch: microphone_denied,
                ..TrayView::default()
            },
            ..Self::default()
        }
    }

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
    ///
    /// Relaunch is offered from the microphone's denial error until a
    /// recording starts (the microphone works after all); the error going
    /// away doesn't hide it, since only a relaunch picks up a grant.
    pub fn on_event(&mut self, event: &UiEvent) -> Option<TrayView> {
        match event {
            UiEvent::Error(DictationError::HotkeyFailed) => {
                self.hotkey_down = true;
                self.error = None;
            }
            UiEvent::Error(error) => {
                if *error == DictationError::PermissionMissing(Permission::Microphone) {
                    self.microphone_denied = true;
                }
                self.error = Some(error.to_string());
            }
            UiEvent::HotkeyRestored => self.hotkey_down = false,
            UiEvent::StateChanged(Phase::Recording) => {
                self.error = None;
                self.microphone_denied = false;
            }
            UiEvent::StateChanged(_) => return None,
        }
        let relaunch = self.microphone_denied;
        let view = match (&self.error, self.hotkey_down) {
            (Some(error), _) => TrayView {
                title: Some(DICTATION_FAILED.to_owned()),
                detail: Some(error.clone()),
                relaunch,
            },
            (None, true) => TrayView {
                title: Some(HOTKEY_UNAVAILABLE.to_owned()),
                detail: Some(DictationError::HotkeyFailed.to_string()),
                relaunch,
            },
            (None, false) => TrayView {
                relaunch,
                ..TrayView::default()
            },
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
    fn both_titles_are_short() {
        for title in [DICTATION_FAILED, HOTKEY_UNAVAILABLE] {
            assert!(title.chars().count() <= MAX_TITLE_CHARS, "{title}");
        }
    }

    #[test]
    fn starts_with_nothing() {
        assert_eq!(TrayStatus::default().view(), &TrayView::default());
        assert_eq!(TrayStatus::new(false).view(), &TrayView::default());
    }

    fn microphone_denied() -> UiEvent {
        UiEvent::Error(DictationError::PermissionMissing(Permission::Microphone))
    }

    fn relaunch(view: Option<TrayView>) -> Option<bool> {
        view.map(|view| view.relaunch)
    }

    // M5.7 criterion 2: Relaunch shows only while the microphone is denied.
    #[test]
    fn m5_7_criterion_2_relaunch_is_offered_only_while_the_microphone_is_denied() {
        let mut tray = TrayStatus::default();
        // Other errors and phases don't offer it.
        assert_eq!(relaunch(tray.on_event(&no_key())), Some(false));
        assert_eq!(
            relaunch(
                tray.on_event(&UiEvent::Error(DictationError::PermissionMissing(
                    Permission::Accessibility
                )))
            ),
            Some(false)
        );
        assert_eq!(
            relaunch(tray.on_event(&UiEvent::Error(DictationError::HotkeyFailed))),
            Some(false)
        );
        assert!(!tray.view().relaunch);

        // The microphone's denial does, with the error.
        let shown = tray.on_event(&microphone_denied()).unwrap();
        assert!(shown.relaunch);
        assert_eq!(shown.title.as_deref(), Some(DICTATION_FAILED));

        // It stays when the error goes (Idle, a newer error, the hotkey
        // back): only a relaunch picks up a grant.
        assert_eq!(tray.on_event(&UiEvent::StateChanged(Phase::Idle)), None);
        assert_eq!(relaunch(tray.on_event(&no_key())), Some(true));
        assert_eq!(tray.on_event(&UiEvent::HotkeyRestored), None);
        assert!(tray.view().relaunch);

        // A recording means the microphone works: gone.
        assert_eq!(
            tray.on_event(&UiEvent::StateChanged(Phase::Recording)),
            Some(TrayView::default())
        );
        // Denied again (the start failed for want of it): back.
        assert_eq!(relaunch(tray.on_event(&microphone_denied())), Some(true));
    }

    #[test]
    fn m5_7_criterion_2_a_microphone_denied_at_launch_offers_relaunch_at_once() {
        let mut tray = TrayStatus::new(true);
        assert_eq!(
            tray.view(),
            &TrayView {
                relaunch: true,
                ..TrayView::default()
            }
        );
        // An error keeps it.
        assert_eq!(relaunch(tray.on_event(&no_key())), Some(true));
        assert_eq!(
            tray.on_event(&UiEvent::StateChanged(Phase::Recording)),
            Some(TrayView::default())
        );
    }

    #[test]
    fn a_dictation_error_shows_until_the_next_recording() {
        let mut tray = TrayStatus::default();
        let shown = tray.on_event(&no_key()).unwrap();
        assert_eq!(shown.title.as_deref(), Some(DICTATION_FAILED));
        assert_eq!(shown.detail, Some(no_key_text()));
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
        assert_eq!(
            tray.on_event(&no_key()),
            Some(TrayView {
                title: Some(DICTATION_FAILED.to_owned()),
                detail: Some(no_key_text()),
                relaunch: false,
            })
        );
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
        assert_eq!(tray.view().detail, Some(no_key_text()));
        // The hotkey failing again replaces it: that's what matters now.
        assert_eq!(
            title(tray.on_event(&UiEvent::Error(DictationError::HotkeyFailed))),
            Some(Some(HOTKEY_UNAVAILABLE.to_owned()))
        );
    }
}
