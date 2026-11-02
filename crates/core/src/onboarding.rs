//! The first-run steps (M5.2): which step a new user is on, and when they
//! may move on.
//!
//! The steps come in a fixed order: [`Microphone`](Step::Microphone),
//! [`Accessibility`](Step::Accessibility), [`Key`](Step::Key) (provider and
//! API key, tested) and [`Try`](Step::Try) (a test dictation). A step is done
//! when its condition holds ([`Readiness::is_met`]); the user may go to a
//! step only when every step before it is done ([`Readiness::allows`]), and
//! may always go back. Nothing here is stored: the conditions are the OS's
//! permissions and the keychain, read by the caller each time.

use serde::{Deserialize, Serialize};

/// One onboarding step, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// Allow the microphone.
    Microphone,
    /// Allow Accessibility (the hotkey and the paste need it).
    Accessibility,
    /// Pick the provider and add a working API key.
    Key,
    /// Try a dictation into a text field in the onboarding window.
    Try,
}

impl Step {
    /// Every step, in order.
    pub const ALL: [Self; 4] = [Self::Microphone, Self::Accessibility, Self::Key, Self::Try];

    /// The step after this one, if any.
    #[must_use]
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Microphone => Some(Self::Accessibility),
            Self::Accessibility => Some(Self::Key),
            Self::Key => Some(Self::Try),
            Self::Try => None,
        }
    }

    /// The step before this one, if any.
    #[must_use]
    pub fn previous(self) -> Option<Self> {
        match self {
            Self::Microphone => None,
            Self::Accessibility => Some(Self::Microphone),
            Self::Key => Some(Self::Accessibility),
            Self::Try => Some(Self::Key),
        }
    }
}

/// What the app has of what dictation needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Readiness {
    /// The microphone permission is granted.
    pub microphone: bool,
    /// The Accessibility permission is granted.
    pub accessibility: bool,
    /// The selected provider has an API key saved.
    pub key_saved: bool,
}

impl Readiness {
    /// Everything is in place.
    pub const READY: Self = Self {
        microphone: true,
        accessibility: true,
        key_saved: true,
    };

    /// Whether `step` is done. [`Step::Try`] is always done: trying is
    /// optional, and it has nothing to wait for.
    #[must_use]
    pub fn is_met(&self, step: Step) -> bool {
        match step {
            Step::Microphone => self.microphone,
            Step::Accessibility => self.accessibility,
            Step::Key => self.key_saved,
            Step::Try => true,
        }
    }

    /// The first step that isn't done, or [`Step::Try`] once all are.
    #[must_use]
    pub fn first_unmet(&self) -> Step {
        Step::ALL
            .into_iter()
            .find(|step| !self.is_met(*step))
            .unwrap_or(Step::Try)
    }

    /// Whether the user may be on `step`: every step before it is done.
    #[must_use]
    pub fn allows(&self, step: Step) -> bool {
        step <= self.first_unmet()
    }

    /// Whether dictation can't work yet, so onboarding shows at launch.
    #[must_use]
    pub fn needs_onboarding(&self) -> bool {
        *self != Self::READY
    }

    /// The step to show for a user who was last on `step`: that step, or the
    /// first one that is no longer done if that comes earlier (a permission
    /// was turned off since).
    #[must_use]
    pub fn resume_at(&self, step: Step) -> Step {
        step.min(self.first_unmet())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readiness(microphone: bool, accessibility: bool, key_saved: bool) -> Readiness {
        Readiness {
            microphone,
            accessibility,
            key_saved,
        }
    }

    #[test]
    fn next_and_previous_walk_the_steps_in_order() {
        for pair in Step::ALL.windows(2) {
            assert_eq!(pair[0].next(), Some(pair[1]));
            assert_eq!(pair[1].previous(), Some(pair[0]));
        }
        assert_eq!(Step::Microphone.previous(), None);
        assert_eq!(Step::Try.next(), None);
    }

    #[test]
    fn the_first_unmet_step_follows_the_order() {
        assert_eq!(
            readiness(false, false, false).first_unmet(),
            Step::Microphone
        );
        assert_eq!(readiness(false, true, true).first_unmet(), Step::Microphone);
        assert_eq!(
            readiness(true, false, false).first_unmet(),
            Step::Accessibility
        );
        assert_eq!(
            readiness(true, false, true).first_unmet(),
            Step::Accessibility
        );
        assert_eq!(readiness(true, true, false).first_unmet(), Step::Key);
        assert_eq!(Readiness::READY.first_unmet(), Step::Try);
    }

    #[test]
    fn a_step_is_allowed_only_once_every_earlier_one_is_done() {
        let none = Readiness::default();
        assert!(none.allows(Step::Microphone));
        assert!(!none.allows(Step::Accessibility));
        assert!(!none.allows(Step::Try));

        let permissions = readiness(true, true, false);
        assert!(permissions.allows(Step::Microphone));
        assert!(permissions.allows(Step::Accessibility));
        assert!(permissions.allows(Step::Key));
        assert!(!permissions.allows(Step::Try));

        for step in Step::ALL {
            assert!(Readiness::READY.allows(step));
        }
    }

    #[test]
    fn onboarding_is_needed_until_everything_is_in_place() {
        assert!(!Readiness::READY.needs_onboarding());
        for missing in [
            readiness(false, true, true),
            readiness(true, false, true),
            readiness(true, true, false),
        ] {
            assert!(missing.needs_onboarding(), "{missing:?}");
        }
    }

    #[test]
    fn resuming_goes_back_to_a_step_that_came_undone() {
        // Accessibility was turned off while the user was on the key step.
        assert_eq!(
            readiness(true, false, true).resume_at(Step::Key),
            Step::Accessibility
        );
        // Otherwise the user is where they left off, even on a done step
        // they went back to.
        assert_eq!(
            Readiness::READY.resume_at(Step::Microphone),
            Step::Microphone
        );
        assert_eq!(readiness(true, true, false).resume_at(Step::Key), Step::Key);
    }

    #[test]
    fn steps_serialize_in_snake_case() {
        let names: Vec<String> = Step::ALL
            .iter()
            .map(|step| serde_json::to_string(step).unwrap())
            .collect();
        assert_eq!(
            names,
            ["\"microphone\"", "\"accessibility\"", "\"key\"", "\"try\""]
        );
    }
}
