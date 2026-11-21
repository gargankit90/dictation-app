//! Onboarding (M5.2): the first-run steps in the settings window.
//!
//! The `main` window shows one of two views, the settings form or the
//! onboarding steps ([`View`]); the app decides which (at launch, from the
//! tray) and the window asks with `get_onboarding` whenever it is shown, so
//! no window needs to listen to events and the window's capability stays
//! "the app's commands and nothing else".
//!
//! The steps and their order are `dictation_core::onboarding`. The app keeps
//! where the user is ([`Onboarding`]) in memory, so going back and reopening
//! from the tray resume at the same step, unless a permission has been turned
//! off since (then that step comes back). Only finishing is kept across a quit,
//! as an empty marker file ([`COMPLETED_MARKER`]); an unfinished setup starts
//! again at the first step that isn't done. The conditions are read each time: the permissions from
//! the OS (cheap, never blocks), the key from the keychain only when the user
//! moves past the Speech step (bounded like the other key reads).
//!
//! Accessibility (#50): macOS reports a grant made in System Settings to the
//! running app (`AXIsProcessTrusted` turns true; the hotkey supervisor from
//! M5.5b starts the event tap by itself). In case the grant doesn't take
//! ([`OnboardingStatus::needs_relaunch`]), the step offers Relaunch:
//! `relaunch_app` leaves a marker in the config directory and restarts the
//! app, and the new launch shows onboarding again ([`take_resume_marker`]).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use dictation_core::onboarding::{Readiness, Step};
use dictation_pipeline::config::{Config, SETTINGS_FILE};
use dictation_ports::{Permission, PermissionStatus, Permissions, SecretStore, SetupProblem};
use serde::{Deserialize, Serialize};

use crate::engine::LiveEngine;

/// The file whose presence makes the next launch show onboarding: left by
/// Relaunch, removed by the launch that reads it.
const RESUME_MARKER: &str = "onboarding-resume";

/// The empty file whose presence says setup was finished.
pub const COMPLETED_MARKER: &str = "onboarding-completed";

/// What the settings window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum View {
    /// The settings form.
    Settings,
    /// The onboarding steps.
    Onboarding,
}

/// A permission the onboarding steps ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum AppPermission {
    /// The microphone.
    Microphone,
    /// Accessibility.
    Accessibility,
}

impl From<AppPermission> for Permission {
    fn from(permission: AppPermission) -> Self {
        match permission {
            AppPermission::Microphone => Self::Microphone,
            AppPermission::Accessibility => Self::Accessibility,
        }
    }
}

/// What the settings window needs to show onboarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub struct OnboardingStatus {
    /// Which view to show.
    pub view: View,
    /// The step the user is on.
    pub step: Step,
    /// The microphone permission.
    pub microphone: PermissionStatus,
    /// The Accessibility permission.
    pub accessibility: PermissionStatus,
    /// macOS says Accessibility is allowed but the hotkey can't use it: a
    /// relaunch makes it take.
    pub needs_relaunch: bool,
    /// Sharing has been on for the whole of this walk through setup.
    pub sharing_on_throughout: bool,
    /// Setup was finished, then a permission went missing: by a person's hand,
    /// or through an update signed differently (N1, #206). Until setup is
    /// finished again the window says so, and Done closes it.
    pub asked_again: bool,
}

/// The OS's permissions, for the onboarding commands.
pub struct OsPermissions(pub Arc<dyn Permissions>);

/// Where the user is in onboarding. Managed state, in memory; only finishing
/// is kept across launches ([`COMPLETED_MARKER`]).
#[derive(Debug)]
pub struct Onboarding {
    state: Mutex<State>,
    /// `None`: nothing is kept (tests).
    marker: Option<PathBuf>,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "independent facts about this walk through setup"
)]
#[derive(Debug)]
struct State {
    view: View,
    /// The step the user is on.
    step: Step,
    /// The user has continued past Welcome.
    welcome_passed: bool,
    /// "Start dictating" was pressed, here or in an earlier launch.
    completed: bool,
    /// Sharing was off at some point of this walk through setup, so Ready
    /// can't say it has been on since setup began (U10).
    sharing_was_off: bool,
    /// Setup was finished, then a permission went missing (N1, #206): the
    /// window says so and closes when both are back.
    asked_again: bool,
    /// Onboarding was opened in this launch. A scripted test launch that never
    /// opens it isn't setting anything up.
    opened: bool,
    /// Problems already recorded this launch, for those read over and over.
    told: Vec<SetupProblem>,
    /// Accessibility was asked for this launch.
    accessibility_asked: bool,
    /// Leaving was recorded, and setup hasn't been opened again since.
    left_told: bool,
    /// How many reads in a row found a permission missing after setup was
    /// finished. One read is no proof it has gone.
    lost_reads: u8,
}

impl Default for Onboarding {
    fn default() -> Self {
        Self::with(false, false, None)
    }
}

/// Why the user can't go to a step yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotYet(pub Step);

impl std::fmt::Display for NotYet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = match self.0 {
            Step::Welcome => "Continue past the first screen first.",
            Step::Speech => "Choose how speech becomes text first.",
            Step::Permissions => "Allow the microphone and Accessibility first.",
            Step::Try | Step::Ready => "Finish the steps before this one first.",
        };
        f.write_str(what)
    }
}

impl Onboarding {
    /// The settings view, onboarding not started, nothing kept.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A new setup starts at Welcome. Past it (`welcome_passed`), setup opens
    /// at the first step that isn't done: the summary once it was finished.
    fn with(completed: bool, welcome_passed: bool, marker: Option<PathBuf>) -> Self {
        let welcome_passed = completed || welcome_passed;
        Self {
            state: Mutex::new(State {
                view: View::Settings,
                step: if welcome_passed {
                    Step::Ready
                } else {
                    Step::Welcome
                },
                welcome_passed,
                completed,
                sharing_was_off: false,
                asked_again: false,
                opened: false,
                told: Vec::new(),
                accessibility_asked: false,
                left_told: false,
                lost_reads: 0,
            }),
            marker,
        }
    }

    /// Whether setup was finished, as the marker in `config_dir` says.
    /// Welcome counts as passed once the app has settings (a fresh install has
    /// none) or after Relaunch, so setup comes back at the step that isn't done.
    #[must_use]
    pub fn load(config_dir: &Path) -> Self {
        let marker = config_dir.join(COMPLETED_MARKER);
        let welcome_passed =
            config_dir.join(SETTINGS_FILE).exists() || resume_marker(config_dir).exists();
        Self::with(marker.exists(), welcome_passed, Some(marker))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Switches the window to onboarding, at the step the user was on, and
    /// returns that step.
    pub fn open(&self, readiness: Readiness) -> Step {
        let mut state = self.lock();
        state.step = readiness.resume_at(state.step);
        if state.completed && !(readiness.microphone && readiness.accessibility) {
            // This read counts, so the window that opens because of it says why
            // from its first status, not a poll later.
            state.lost_reads = state.lost_reads.max(1);
        }
        state.view = View::Onboarding;
        state.opened = true;
        state.left_told = false;
        state.step
    }

    /// "Run setup again": setup that was finished goes through the steps from
    /// Welcome. It stays finished, so the tray doesn't start offering "Resume
    /// setup…", and Welcome stays passed.
    pub fn start_over_if_finished(&self) {
        let mut state = self.lock();
        if !state.completed {
            return;
        }
        state.step = Step::Welcome;
        state.asked_again = false;
        // A new walk: whether sharing is off now is noted when it opens.
        state.sharing_was_off = false;
    }

    /// Switches the window back to the settings view; the step is kept for
    /// the next [`open`](Self::open).
    pub fn close(&self) {
        self.lock().view = View::Settings;
    }

    /// "Start dictating": setup is done, and the window goes back to the
    /// settings view. The marker is written; a failure only costs showing
    /// setup again at the next launch, so it is logged and the app goes on.
    pub fn finish(&self) {
        let mut state = self.lock();
        state.view = View::Settings;
        state.completed = true;
        state.asked_again = false;
        drop(state);
        let Some(marker) = &self.marker else { return };
        let written = marker
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(marker, b""));
        if let Err(error) = written {
            tracing::warn!(%error, "couldn't note that setup is finished");
        }
    }

    /// The view.
    #[must_use]
    pub fn view(&self) -> View {
        self.lock().view
    }

    /// Whether the user has continued past Welcome.
    #[must_use]
    pub fn welcome_passed(&self) -> bool {
        self.lock().welcome_passed
    }

    /// Whether setup was finished.
    #[must_use]
    pub fn completed(&self) -> bool {
        self.lock().completed
    }

    /// Whether setup isn't finished: the tray then offers "Resume setup…".
    #[must_use]
    pub fn in_progress(&self) -> bool {
        !self.lock().completed
    }

    /// The user continued past Welcome. True the first time.
    pub fn pass_welcome(&self) -> bool {
        let mut state = self.lock();
        !std::mem::replace(&mut state.welcome_passed, true)
    }

    /// Whether `problem` hasn't been recorded yet this launch; it now has.
    pub fn first_time(&self, problem: SetupProblem) -> bool {
        let mut state = self.lock();
        if state.told.contains(&problem) {
            return false;
        }
        state.told.push(problem);
        true
    }

    /// Sharing is off, or was just turned off: Ready can't say it has been on
    /// since setup began. Nothing to note for someone who has never opened
    /// setup: opening it notes the choice then.
    pub fn note_sharing_off(&self) {
        let mut state = self.lock();
        if state.opened {
            state.sharing_was_off = true;
        }
    }

    /// The user asked for Accessibility.
    pub fn note_accessibility_asked(&self) {
        self.lock().accessibility_asked = true;
    }

    /// If setup is on the window and leaving it wasn't recorded yet: the step
    /// the user stopped on, and whether they had asked for Accessibility.
    pub fn take_left(&self) -> Option<(Step, bool)> {
        let mut state = self.lock();
        if state.view != View::Onboarding || state.left_told {
            return None;
        }
        state.left_told = true;
        Some((state.step, state.accessibility_asked))
    }

    /// What the window shows. A step whose permission was turned off since
    /// comes back (the key isn't read here: it only matters for moving on).
    /// `hotkey_down`: the hotkey isn't working right now.
    pub fn status(&self, permissions: &dyn Permissions, hotkey_down: bool) -> OnboardingStatus {
        let microphone = permissions.check(Permission::Microphone);
        let accessibility = permissions.check(Permission::Accessibility);
        let mut state = self.lock();
        let readiness = Readiness {
            welcome_passed: state.welcome_passed,
            speech_chosen: true,
            microphone: microphone == PermissionStatus::Granted,
            accessibility: accessibility == PermissionStatus::Granted,
        };
        let step = readiness.resume_at(state.step);
        let missing = state.completed
            && (microphone != PermissionStatus::Granted
                || accessibility != PermissionStatus::Granted);
        state.lost_reads = if missing {
            state.lost_reads.saturating_add(1)
        } else {
            0
        };
        state.step = step;
        state.asked_again |= state.lost_reads >= 2;
        OnboardingStatus {
            view: state.view,
            step,
            microphone,
            accessibility,
            needs_relaunch: accessibility == PermissionStatus::Granted && hotkey_down,
            sharing_on_throughout: !state.sharing_was_off,
            asked_again: state.asked_again,
        }
    }

    /// Moves to `step` if every step before it is done.
    ///
    /// # Errors
    ///
    /// [`NotYet`] with the first step that isn't done; the user stays where
    /// they were.
    pub fn go_to(&self, step: Step, readiness: Readiness) -> Result<Step, NotYet> {
        if !readiness.allows(step) {
            return Err(NotYet(readiness.first_unmet()));
        }
        self.lock().step = step;
        Ok(step)
    }
}

/// What the app has of what dictation needs. `read_key` is asked only if
/// `with_key`; otherwise the key counts as saved.
pub fn readiness(
    permissions: &dyn Permissions,
    welcome_passed: bool,
    with_key: bool,
    read_key: impl FnOnce() -> bool,
) -> Readiness {
    Readiness {
        welcome_passed,
        speech_chosen: !with_key || read_key(),
        microphone: permissions.check(Permission::Microphone) == PermissionStatus::Granted,
        accessibility: permissions.check(Permission::Accessibility) == PermissionStatus::Granted,
    }
}

/// Whether an engine is chosen: the provider in the settings has a key saved
/// (see below), or the settings choose an on-device model. A keychain that
/// can't answer (locked, timed out) counts as no key: onboarding then asks
/// for one, and saving it again does no harm.
///
/// With the on-device engine (L1) no key is needed, so the step counts as
/// done and the keychain isn't read.
pub fn key_saved<S: SecretStore + 'static>(config: &Arc<Config<S>>, live: &LiveEngine) -> bool {
    key_saved_or(config, live, false)
}

/// [`key_saved`], with `no_answer` as the result when the keychain can't answer.
/// Opening the window after setup was finished passes true (N1, #206, #259): after
/// an update signed differently, macOS's prompt holds the read back, and that
/// isn't a reason to send the person to add a key that is still there.
pub fn key_saved_or<S: SecretStore + 'static>(
    config: &Arc<Config<S>>,
    live: &LiveEngine,
    no_answer: bool,
) -> bool {
    let settings = config.settings().unwrap_or_default();
    if settings.transcription == dictation_core::settings::Transcription::Local {
        return true;
    }
    match live.read_key(config) {
        Ok(key) => key.is_some(),
        Err(error) => {
            tracing::warn!(%error, "onboarding: couldn't check for a saved key");
            no_answer
        }
    }
}

/// Asks the OS for `permission`: the system prompt while it hasn't been
/// asked, System Settings at the permission's page once it was refused.
/// Accessibility gets both: macOS shows its prompt at most once, and the
/// user turns the app on in the list either way.
///
/// # Errors
///
/// The prompt or System Settings couldn't be opened; the message is for the
/// user.
pub fn request(
    permissions: &dyn Permissions,
    permission: Permission,
    open_settings: impl Fn(Permission) -> std::io::Result<()>,
) -> Result<(), String> {
    let status = permissions.check(permission);
    let prompt = |permission| {
        permissions.request(permission).map_err(|error| {
            tracing::warn!(%error, ?permission, "onboarding: the prompt failed");
            "macOS didn't show its prompt. Open System Settings → Privacy & Security and allow Vocet there.".to_owned()
        })
    };
    let settings = |permission| {
        open_settings(permission).map_err(|error| {
            tracing::warn!(%error, ?permission, "onboarding: couldn't open System Settings");
            "Couldn't open System Settings. Open it yourself: System Settings → Privacy & Security, then allow Vocet there.".to_owned()
        })
    };
    match (permission, status) {
        (_, PermissionStatus::Granted) => Ok(()),
        (Permission::Microphone, PermissionStatus::NotDetermined) => prompt(permission),
        (Permission::Microphone, PermissionStatus::Denied) => settings(permission),
        (Permission::Accessibility, _) => {
            prompt(permission)?;
            settings(permission)
        }
    }
}

/// The marker [`relaunch`] leaves in `config_dir`.
#[must_use]
pub fn resume_marker(config_dir: &Path) -> PathBuf {
    config_dir.join(RESUME_MARKER)
}

/// Leaves the marker that makes the next launch show onboarding.
///
/// # Errors
///
/// If the marker can't be written.
pub fn leave_resume_marker(config_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    std::fs::write(resume_marker(config_dir), b"")
}

/// Whether the last launch asked this one to show onboarding (Relaunch); the
/// marker is removed, so only this launch does.
#[must_use]
pub fn take_resume_marker(config_dir: &Path) -> bool {
    std::fs::remove_file(resume_marker(config_dir)).is_ok()
}

/// Whether onboarding shows at launch: when setup isn't finished, when
/// dictation can't work yet, or when the last launch was Relaunch. A scripted
/// test launch (`scripted`) gets it only after Relaunch, so it can't take the
/// focus from the window the test pastes into.
#[must_use]
pub fn show_at_launch(
    readiness: Readiness,
    completed: bool,
    resumed: bool,
    scripted: bool,
) -> bool {
    resumed || (!scripted && (!completed || readiness.needs_onboarding()))
}

/// Whether a launch is a scripted test (debug builds): it records a debug
/// audio file or runs the mock engine, unless it asks to be treated like a
/// person's launch (`--debug-onboarding`).
#[must_use]
pub fn scripted_launch(audio_file: bool, mock_engine: bool, like_a_person: bool) -> bool {
    (audio_file || mock_engine) && !like_a_person
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use dictation_platform::fakes::{FakePermissions, MemSecretStore};
    use dictation_ports::PermissionStatus::{Denied, Granted, NotDetermined};

    use super::*;

    fn granted(microphone: bool, accessibility: bool) -> FakePermissions {
        let permissions = FakePermissions::new();
        let status = |yes| if yes { Granted } else { NotDetermined };
        permissions.set(Permission::Microphone, status(microphone));
        permissions.set(Permission::Accessibility, status(accessibility));
        permissions
    }

    fn temp(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dictation-onboarding-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    const PASSED: Readiness = Readiness {
        welcome_passed: true,
        speech_chosen: false,
        microphone: false,
        accessibility: false,
    };

    #[test]
    fn n1_a_permission_lost_after_finishing_is_said_until_setup_is_finished_again() {
        let (lost, back) = (granted(false, false), granted(true, true));
        assert!(
            !Onboarding::new().status(&lost, false).asked_again,
            "a first run hasn't lost anything"
        );

        let finished = || Onboarding::with(true, false, None);
        assert!(
            !finished().status(&back, false).asked_again,
            "nothing has gone"
        );

        let blip = finished();
        assert!(
            !blip.status(&lost, false).asked_again,
            "one read is no proof"
        );
        assert!(
            !blip.status(&back, false).asked_again,
            "and a good read forgets it"
        );
        assert!(
            !blip.status(&lost, false).asked_again,
            "so reads have to be in a row"
        );

        let polled = finished();
        polled.status(&lost, false);
        let status = polled.status(&lost, false);
        assert!(status.asked_again, "two in a row, as the window polls");
        assert_eq!(status.step, Step::Permissions, "its step comes back");
        assert!(
            polled.status(&back, false).asked_again,
            "until setup is finished"
        );
        polled.finish();
        assert!(
            !polled.status(&back, false).asked_again,
            "finishing ends it"
        );

        let again = finished();
        again.open(Readiness {
            microphone: false,
            ..Readiness::READY
        });
        assert!(
            again.status(&lost, false).asked_again,
            "the read that opened the window counts, so it says why at once"
        );
        again.start_over_if_finished();
        assert!(
            !again.status(&back, false).asked_again,
            "Run setup again is a new walk"
        );
    }

    #[test]
    fn u10_a_note_that_sharing_was_off_lasts_until_the_walk_ends() {
        let onboarding = Onboarding::new();
        onboarding.note_sharing_off();
        let permissions = granted(true, true);
        onboarding.open(Readiness::READY);
        assert!(
            onboarding.status(&permissions, false).sharing_on_throughout,
            "setup never opened: nothing to note"
        );
        onboarding.note_sharing_off();
        assert!(!onboarding.status(&permissions, false).sharing_on_throughout);
        onboarding.finish();
        onboarding.start_over_if_finished();
        assert!(
            onboarding.status(&permissions, false).sharing_on_throughout,
            "Run setup again is a new walk"
        );
    }

    /// An unfinished setup starts again at the first step that isn't done;
    /// one that was finished is reopened at the summary, past Welcome.
    #[test]
    fn only_finishing_is_kept_across_launches() {
        let dir = temp("marker");
        let onboarding = Onboarding::load(&dir);
        assert!(onboarding.in_progress());
        assert!(!onboarding.welcome_passed(), "a fresh install");
        onboarding.open(Readiness::default());
        onboarding.pass_welcome();
        onboarding.go_to(Step::Try, Readiness::READY).unwrap();
        assert!(onboarding.in_progress());
        drop(onboarding);

        let relaunched = Onboarding::load(&dir);
        assert!(!relaunched.welcome_passed());
        assert_eq!(relaunched.open(Readiness::default()), Step::Welcome);
        drop(relaunched);

        // With settings saved, or after Relaunch, Welcome is behind the user.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SETTINGS_FILE), "{}").unwrap();
        let with_settings = Onboarding::load(&dir);
        assert!(with_settings.welcome_passed());
        let missing = Readiness {
            accessibility: false,
            ..Readiness::READY
        };
        assert_eq!(with_settings.open(missing), Step::Permissions);
        with_settings.finish();
        assert!(dir.join(COMPLETED_MARKER).exists());

        let finished = Onboarding::load(&dir);
        assert!(finished.completed() && finished.welcome_passed());
        assert!(!finished.in_progress());
        assert_eq!(finished.open(Readiness::READY), Step::Ready);
        assert_eq!(
            finished.open(Readiness {
                accessibility: false,
                ..Readiness::READY
            }),
            Step::Permissions
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_window_starts_on_settings_and_a_new_user_opens_at_welcome() {
        let onboarding = Onboarding::new();
        assert_eq!(onboarding.view(), View::Settings);
        assert_eq!(onboarding.open(Readiness::default()), Step::Welcome);
        assert_eq!(onboarding.view(), View::Onboarding);
        onboarding.close();
        assert_eq!(onboarding.view(), View::Settings);
    }

    #[test]
    fn the_status_reports_the_permissions_and_follows_them() {
        let permissions = granted(false, false);
        permissions.set(Permission::Microphone, Denied);
        let onboarding = Onboarding::new();
        onboarding.open(Readiness::default());
        onboarding.pass_welcome();
        onboarding
            .go_to(Step::Permissions, Readiness::READY)
            .unwrap();
        let status = onboarding.status(&permissions, false);
        assert_eq!(status.view, View::Onboarding);
        assert_eq!(status.step, Step::Permissions);
        assert_eq!(status.microphone, PermissionStatus::Denied);
        assert_eq!(status.accessibility, PermissionStatus::NotDetermined);

        // Granted in System Settings while the window is open.
        permissions.set(Permission::Microphone, Granted);
        let status = onboarding.status(&permissions, false);
        assert_eq!(status.microphone, PermissionStatus::Granted);
        assert_eq!(status.step, Step::Permissions);
    }

    #[test]
    fn needs_relaunch_means_allowed_but_the_hotkey_is_down() {
        let onboarding = Onboarding::new();
        assert!(
            !onboarding
                .status(&granted(true, true), false)
                .needs_relaunch
        );
        assert!(onboarding.status(&granted(true, true), true).needs_relaunch);
        // Not allowed yet: that is the usual wait, not this.
        assert!(
            !onboarding
                .status(&granted(true, false), true)
                .needs_relaunch
        );
    }

    #[test]
    fn each_step_is_reached_only_when_the_ones_before_are_done() {
        let onboarding = Onboarding::new();
        let none = Readiness::default();
        onboarding.open(none);
        assert_eq!(
            onboarding.go_to(Step::Speech, none),
            Err(NotYet(Step::Welcome))
        );
        assert_eq!(onboarding.go_to(Step::Speech, PASSED), Ok(Step::Speech));
        assert_eq!(
            onboarding.go_to(Step::Permissions, PASSED),
            Err(NotYet(Step::Speech))
        );
        let chosen = Readiness {
            speech_chosen: true,
            ..PASSED
        };
        assert_eq!(
            onboarding.go_to(Step::Permissions, chosen),
            Ok(Step::Permissions)
        );
        assert_eq!(
            onboarding.go_to(Step::Try, chosen),
            Err(NotYet(Step::Permissions))
        );
        assert_eq!(onboarding.go_to(Step::Try, Readiness::READY), Ok(Step::Try));
        assert_eq!(
            onboarding.go_to(Step::Ready, Readiness::READY),
            Ok(Step::Ready)
        );
        // Back is always allowed.
        assert_eq!(onboarding.go_to(Step::Welcome, none), Ok(Step::Welcome));
    }

    #[test]
    fn reopening_resumes_where_the_user_was() {
        let onboarding = Onboarding::new();
        onboarding.pass_welcome();
        onboarding.open(Readiness::READY);
        onboarding.go_to(Step::Try, Readiness::READY).unwrap();
        // The user went back a step, then closed the window.
        onboarding
            .go_to(Step::Permissions, Readiness::READY)
            .unwrap();
        onboarding.close();
        assert_eq!(onboarding.open(Readiness::READY), Step::Permissions);
        assert_eq!(
            onboarding.status(&granted(true, true), false).step,
            Step::Permissions
        );
    }

    #[test]
    fn a_permission_turned_off_since_brings_its_step_back() {
        let onboarding = Onboarding::new();
        onboarding.pass_welcome();
        onboarding.open(Readiness::READY);
        onboarding.go_to(Step::Try, Readiness::READY).unwrap();
        // Accessibility turned off in System Settings.
        let permissions = granted(true, false);
        assert_eq!(
            onboarding.status(&permissions, false).step,
            Step::Permissions
        );
        // And it stays there once reopened.
        onboarding.close();
        assert_eq!(
            onboarding.open(readiness(&permissions, true, false, || true)),
            Step::Permissions
        );
    }

    /// U8 criterion 6.
    #[test]
    fn finishing_marks_setup_complete_across_launches() {
        let dir = temp("finish");
        let onboarding = Onboarding::load(&dir);
        onboarding.pass_welcome();
        onboarding.open(Readiness::READY);
        assert!(onboarding.in_progress());
        onboarding.finish();
        assert!(onboarding.completed());
        assert!(!onboarding.in_progress());
        assert_eq!(onboarding.view(), View::Settings);
        let relaunched = Onboarding::load(&dir);
        assert!(relaunched.completed());
        assert!(!relaunched.in_progress());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_setup_again_starts_over_only_after_finishing() {
        let onboarding = Onboarding::new();
        onboarding.pass_welcome();
        onboarding.open(Readiness::READY);
        onboarding.go_to(Step::Try, Readiness::READY).unwrap();
        onboarding.start_over_if_finished();
        assert_eq!(onboarding.open(Readiness::READY), Step::Try, "still going");

        onboarding.finish();
        onboarding.start_over_if_finished();
        assert_eq!(onboarding.open(Readiness::READY), Step::Welcome);
        assert!(onboarding.completed());
        assert!(onboarding.welcome_passed());
        assert!(!onboarding.in_progress());
    }

    #[test]
    fn readiness_reads_the_key_only_when_asked() {
        let asked = Cell::new(false);
        let permissions = granted(true, false);
        let ready = readiness(&permissions, true, false, || {
            asked.set(true);
            false
        });
        assert!(!asked.get());
        assert_eq!(
            ready,
            Readiness {
                welcome_passed: true,
                speech_chosen: true,
                microphone: true,
                accessibility: false,
            }
        );
        assert!(!readiness(&permissions, true, true, || false).speech_chosen);
        assert!(readiness(&permissions, true, true, || true).speech_chosen);
    }

    #[test]
    fn a_saved_openrouter_key_counts() {
        let dir = temp("key");
        let secrets = MemSecretStore::new();
        let config = Arc::new(Config::new(&dir, secrets));
        let live = LiveEngine::fixed();
        assert!(!key_saved(&config, &live));
        config.set_api_key("sk-or-made-up-key").unwrap();
        assert!(key_saved(&config, &live));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn l1_the_on_device_engine_needs_no_key() {
        let dir = temp("local-key");
        let config = Arc::new(Config::new(&dir, MemSecretStore::new()));
        let live = LiveEngine::fixed();
        assert!(!key_saved(&config, &live));
        let settings = dictation_core::settings::Settings {
            transcription: dictation_core::settings::Transcription::Local,
            ..dictation_core::settings::Settings::default()
        };
        config.save_settings(&settings).unwrap();
        assert!(key_saved(&config, &live));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_microphone_prompts_first_then_opens_system_settings() {
        let opened = std::sync::Mutex::new(Vec::new());
        let open = |permission| {
            opened.lock().unwrap().push(permission);
            Ok(())
        };
        let permissions = FakePermissions::new();
        permissions.answer_requests_with(Denied);
        request(&permissions, Permission::Microphone, open).unwrap();
        assert_eq!(permissions.requests(), [Permission::Microphone]);
        assert!(opened.lock().unwrap().is_empty());
        // Refused: only System Settings can help now.
        request(&permissions, Permission::Microphone, open).unwrap();
        assert_eq!(permissions.requests(), [Permission::Microphone]);
        assert_eq!(*opened.lock().unwrap(), [Permission::Microphone]);
    }

    #[test]
    fn accessibility_prompts_and_opens_system_settings() {
        let opened = std::sync::Mutex::new(Vec::new());
        let permissions = FakePermissions::new();
        // macOS keeps reporting "not determined" until the user turns it on.
        permissions.answer_requests_with(NotDetermined);
        request(&permissions, Permission::Accessibility, |permission| {
            opened.lock().unwrap().push(permission);
            Ok(())
        })
        .unwrap();
        assert_eq!(permissions.requests(), [Permission::Accessibility]);
        assert_eq!(*opened.lock().unwrap(), [Permission::Accessibility]);
    }

    #[test]
    fn a_granted_permission_asks_nothing() {
        let permissions = FakePermissions::all_granted();
        for permission in [Permission::Microphone, Permission::Accessibility] {
            request(&permissions, permission, |_| panic!("opened settings")).unwrap();
        }
        assert!(permissions.requests().is_empty());
    }

    #[test]
    fn failures_say_what_to_do_instead() {
        let permissions = FakePermissions::new();
        permissions.fail_requests(true);
        let error = request(&permissions, Permission::Microphone, |_| Ok(())).unwrap_err();
        assert!(error.contains("System Settings"), "{error}");

        let permissions = FakePermissions::new();
        permissions.set(Permission::Microphone, Denied);
        let error = request(&permissions, Permission::Microphone, |_| {
            Err(std::io::Error::other("no open"))
        })
        .unwrap_err();
        assert!(
            error.starts_with("Couldn't open System Settings."),
            "{error}"
        );
    }

    #[test]
    fn the_resume_marker_is_read_once() {
        let dir = temp("marker");
        assert!(!take_resume_marker(&dir));
        leave_resume_marker(&dir).unwrap();
        assert!(resume_marker(&dir).exists());
        assert!(take_resume_marker(&dir));
        assert!(!take_resume_marker(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn onboarding_shows_at_launch_when_something_is_missing() {
        let missing = Readiness {
            speech_chosen: false,
            ..Readiness::READY
        };
        assert!(show_at_launch(missing, true, false, false));
        assert!(!show_at_launch(Readiness::READY, true, false, false));
        // After Relaunch, even with everything in place.
        assert!(show_at_launch(Readiness::READY, true, true, false));
        // A scripted test launch only after Relaunch.
        assert!(!show_at_launch(missing, false, false, true));
        assert!(show_at_launch(Readiness::READY, true, true, true));
    }

    /// U8 criterion 7: setup not finished comes back at launch, even when
    /// dictation could already work.
    #[test]
    fn unfinished_setup_shows_at_launch() {
        assert!(show_at_launch(Readiness::READY, false, false, false));
    }

    #[test]
    fn test_launches_are_scripted_unless_they_ask_not_to_be() {
        assert!(!scripted_launch(false, false, false), "a person's launch");
        assert!(scripted_launch(true, false, false));
        assert!(scripted_launch(false, true, false));
        assert!(!scripted_launch(true, true, true), "--debug-onboarding");
    }

    #[test]
    fn not_yet_says_which_step_comes_first() {
        assert_eq!(
            NotYet(Step::Speech).to_string(),
            "Choose how speech becomes text first."
        );
        for step in Step::ALL {
            assert!(NotYet(step).to_string().ends_with("first."));
        }
    }
}
