//! The first-run steps (U8): which step a new user is on, when they may move
//! on, and what is kept across launches.
//!
//! The steps come in a fixed order: [`Welcome`](Step::Welcome) (what the app
//! does), [`Speech`](Step::Speech) (language and
//! engine), [`Permissions`](Step::Permissions) (microphone and
//! Accessibility), [`Try`](Step::Try) (a practice dictation) and
//! [`Ready`](Step::Ready) (a summary, and the events switch). A step is done when its condition
//! holds ([`Readiness::is_met`]); the user may go to a step only when every
//! step before it is done ([`Readiness::allows`]), and may always go back.
//!
//! The permissions and the key are read by the caller each time. What only
//! the user's walk through the steps can tell is [`SetupProgress`], which the
//! app keeps in a file so that quitting resumes at the same step.

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
    /// The language, and the on-device model or the user's own key.
    Speech,
    /// Allow the microphone and Accessibility.
    Permissions,
    /// The dictation key, and a practice dictation in the onboarding window.
    Try,
    /// A summary of the real states, and the usage and error events switch;
    /// closes onboarding.
    Ready,
}

impl Step {
    /// Every step, in order.
    pub const ALL: [Self; 5] = [
        Self::Welcome,
        Self::Speech,
        Self::Permissions,
        Self::Try,
        Self::Ready,
    ];

    /// The step after this one, if any.
    #[must_use]
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Welcome => Some(Self::Speech),
            Self::Speech => Some(Self::Permissions),
            Self::Permissions => Some(Self::Try),
            Self::Try => Some(Self::Ready),
            Self::Ready => None,
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
            Self::Ready => Some(Self::Try),
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

    /// Whether `step` is done. [`Step::Try`] and [`Step::Ready`] are always
    /// done: trying is optional, and neither has anything to wait for.
    #[must_use]
    pub fn is_met(&self, step: Step) -> bool {
        match step {
            Step::Welcome => self.welcome_passed,
            Step::Speech => self.speech_chosen,
            Step::Permissions => self.microphone && self.accessibility,
            Step::Try | Step::Ready => true,
        }
    }

    /// The first step that isn't done, or [`Step::Ready`] once all are.
    #[must_use]
    pub fn first_unmet(&self) -> Step {
        Step::ALL
            .into_iter()
            .find(|step| !self.is_met(*step))
            .unwrap_or(Step::Ready)
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

/// How the practice dictation in [`Step::Try`] went.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum TryOutcome {
    /// Not tried yet.
    #[default]
    NotTried,
    /// A dictation landed in the practice box.
    Passed,
    /// The user pressed "Skip for now".
    Skipped,
}

/// The engine the user selected on [`Step::Speech`], before they continue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum SpeechChoice {
    /// The recommended on-device model.
    Local,
    /// The user's own key. The key itself is never kept here.
    OwnKey,
}

/// What survives a quit: where the user is and what they chose that only the
/// steps know. Holds no audio, text or key.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent facts kept in a file, each with its own meaning"
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetupProgress {
    /// The step the user was on.
    pub step: Step,
    /// The user has continued past Welcome.
    pub welcome_passed: bool,
    /// "Start dictating" was pressed.
    pub completed: bool,
    /// How the practice dictation went.
    pub try_outcome: TryOutcome,
    /// The user cancelled the model download, so a relaunch doesn't start it
    /// again by itself.
    pub download_paused: bool,
    /// The engine selected on the Speech step, if the user selected one.
    pub speech_choice: Option<SpeechChoice>,
    /// Sharing was off at some point of this walk through setup, so Ready
    /// can't say it has been on since setup began (U10).
    pub sharing_was_off: bool,
}

impl SetupProgress {
    /// Someone who set the app up before these steps existed: nothing to
    /// resume, and they already had the chance to turn sharing off.
    pub const ALREADY_SET_UP: Self = Self {
        step: Step::Ready,
        welcome_passed: true,
        completed: true,
        try_outcome: TryOutcome::NotTried,
        download_paused: false,
        speech_choice: None,
        sharing_was_off: false,
    };

    /// Reads the file's text. `None` if it can't be understood.
    #[must_use]
    pub fn from_json(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }

    /// The file's text.
    #[must_use]
    pub fn to_json(&self) -> String {
        // Plain data with string keys: serializing it can't fail.
        serde_json::to_string(self).unwrap_or_default()
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
    fn there_are_exactly_five_steps_in_the_owners_order() {
        assert_eq!(
            Step::ALL,
            [
                Step::Welcome,
                Step::Speech,
                Step::Permissions,
                Step::Try,
                Step::Ready
            ]
        );
    }

    #[test]
    fn next_and_previous_walk_the_steps_in_order() {
        for pair in Step::ALL.windows(2) {
            assert_eq!(pair[0].next(), Some(pair[1]));
            assert_eq!(pair[1].previous(), Some(pair[0]));
        }
        assert_eq!(Step::Welcome.previous(), None);
        assert_eq!(Step::Ready.next(), None);
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
        assert_eq!(Readiness::READY.first_unmet(), Step::Ready);
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
        assert!(Readiness::READY.allows(Step::Ready));
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
        assert_eq!(Readiness::READY.resume_at(Step::Ready), Step::Ready);
        assert_eq!(
            readiness(true, true, false, true).resume_at(Step::Try),
            Step::Permissions
        );
    }

    #[test]
    fn the_progress_round_trips_and_a_bad_file_is_refused() {
        let progress = SetupProgress {
            step: Step::Permissions,
            welcome_passed: true,
            completed: false,
            try_outcome: TryOutcome::Skipped,
            download_paused: true,
            speech_choice: Some(SpeechChoice::OwnKey),
            sharing_was_off: true,
        };
        assert_eq!(
            SetupProgress::from_json(&progress.to_json()),
            Some(progress)
        );
        assert_eq!(SetupProgress::from_json("not json"), None);
    }

    #[test]
    fn a_partial_progress_file_takes_defaults_for_the_rest() {
        assert_eq!(
            SetupProgress::from_json(r#"{"step":"try"}"#),
            Some(SetupProgress {
                step: Step::Try,
                ..SetupProgress::default()
            })
        );
    }
}
