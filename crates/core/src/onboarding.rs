//! The first-run steps (U8, TOS1): which step a new user is on, when they may
//! move on.
//!
//! The steps come in a fixed order: [`Welcome`](Step::Welcome) (what the app
//! does, and the Terms), [`Speech`](Step::Speech) (the engine),
//! [`Permissions`](Step::Permissions) (microphone and Accessibility) and
//! [`Try`](Step::Try) (a practice dictation; Done ends setup). A step is done
//! when its condition holds ([`Readiness::is_met`]); the user may go to a step
//! only when every step before it is done ([`Readiness::allows`]), and may
//! always go back.
//!
//! The permissions and the key are read by the caller each time.

use serde::{Deserialize, Serialize};

/// One onboarding step, in order.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// What the app does.
    #[default]
    Welcome,
    /// The on-device model or the user's own key.
    Speech,
    /// Allow the microphone and Accessibility.
    Permissions,
    /// The dictation key, a practice dictation in the onboarding window, and
    /// Done, which closes onboarding.
    Try,
}

impl Step {
    /// Every step, in order.
    pub const ALL: [Self; 4] = [Self::Welcome, Self::Speech, Self::Permissions, Self::Try];

    /// The step after this one, if any.
    #[must_use]
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Welcome => Some(Self::Speech),
            Self::Speech => Some(Self::Permissions),
            Self::Permissions => Some(Self::Try),
            Self::Try => None,
        }
    }

    /// The step before this one, if any.
    #[must_use]
    pub fn previous(self) -> Option<Self> {
        match self {
            Self::Welcome => None,
            Self::Speech => Some(Self::Welcome),
            Self::Permissions => Some(Self::Speech),
            Self::Try => Some(Self::Permissions),
        }
    }
}

/// What the app has of what dictation needs.
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent facts, each read from its own place"
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Readiness {
    /// The user has continued past Welcome.
    pub welcome_passed: bool,
    /// An engine is chosen: an on-device model (downloaded or not), or a
    /// provider with a key saved.
    pub speech_chosen: bool,
    /// The microphone permission is granted.
    pub microphone: bool,
    /// The Accessibility permission is granted.
    pub accessibility: bool,
}

impl Readiness {
    /// Everything is in place.
    pub const READY: Self = Self {
        welcome_passed: true,
        speech_chosen: true,
        microphone: true,
        accessibility: true,
    };

    /// Whether `step` is done. [`Step::Try`] is always done: trying is
    /// optional, and it has nothing to wait for.
    #[must_use]
    pub fn is_met(&self, step: Step) -> bool {
        match step {
            Step::Welcome => self.welcome_passed,
            Step::Speech => self.speech_chosen,
            Step::Permissions => self.microphone && self.accessibility,
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

    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "a table of the four facts reads best in the tests"
    )]
    fn readiness(
        welcome_passed: bool,
        speech_chosen: bool,
        microphone: bool,
        accessibility: bool,
    ) -> Readiness {
        Readiness {
            welcome_passed,
            speech_chosen,
            microphone,
            accessibility,
        }
    }

    #[test]
    fn there_are_exactly_four_steps_in_the_owners_order() {
        assert_eq!(
            Step::ALL,
            [Step::Welcome, Step::Speech, Step::Permissions, Step::Try]
        );
    }

    #[test]
    fn next_and_previous_walk_the_steps_in_order() {
        for pair in Step::ALL.windows(2) {
            assert_eq!(pair[0].next(), Some(pair[1]));
            assert_eq!(pair[1].previous(), Some(pair[0]));
        }
        assert_eq!(Step::Welcome.previous(), None);
        assert_eq!(Step::Try.next(), None);
    }

    #[test]
    fn the_first_unmet_step_follows_the_order() {
        assert_eq!(
            readiness(false, true, true, true).first_unmet(),
            Step::Welcome
        );
        assert_eq!(
            readiness(true, false, true, true).first_unmet(),
            Step::Speech
        );
        assert_eq!(
            readiness(true, true, false, true).first_unmet(),
            Step::Permissions
        );
        assert_eq!(
            readiness(true, true, true, false).first_unmet(),
            Step::Permissions
        );
        assert_eq!(Readiness::READY.first_unmet(), Step::Try);
    }

    #[test]
    fn a_step_is_allowed_only_when_the_ones_before_are_done() {
        let fresh = Readiness::default();
        assert!(fresh.allows(Step::Welcome));
        assert!(!fresh.allows(Step::Speech));
        let passed = readiness(true, false, false, false);
        assert!(passed.allows(Step::Speech));
        assert!(!passed.allows(Step::Permissions));
        let chosen = readiness(true, true, false, false);
        assert!(chosen.allows(Step::Permissions));
        assert!(!chosen.allows(Step::Try));
        assert!(Readiness::READY.allows(Step::Try));
    }

    #[test]
    fn needs_onboarding_until_everything_is_in_place() {
        assert!(Readiness::default().needs_onboarding());
        assert!(readiness(true, true, true, false).needs_onboarding());
        assert!(!Readiness::READY.needs_onboarding());
    }

    #[test]
    fn resuming_keeps_the_step_unless_an_earlier_one_came_undone() {
        assert_eq!(Readiness::READY.resume_at(Step::Speech), Step::Speech);
        assert_eq!(Readiness::READY.resume_at(Step::Try), Step::Try);
        assert_eq!(
            readiness(true, true, false, true).resume_at(Step::Try),
            Step::Permissions
        );
    }
}
