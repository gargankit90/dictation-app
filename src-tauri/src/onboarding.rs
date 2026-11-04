//! Onboarding (M5.2): the first-run steps in the settings window.
//!
//! The `main` window shows one of two views, the settings form or the
//! onboarding steps ([`View`]); the app decides which (at launch, from the
//! tray) and the window asks with `get_onboarding` whenever it is shown, so
//! no window needs to listen to events and the window's capability stays
//! "the app's commands and nothing else".
//!
//! The steps and their order are `dictation_core::onboarding`. The app keeps
//! where the user is ([`Onboarding`]) in a file in the config directory
//! (U8), so going back, reopening from the tray and quitting all resume at
//! the same step, unless a permission has been turned off since (then that
//! step comes back). The conditions are read each time: the permissions from
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

use dictation_core::onboarding::{Readiness, SetupProgress, Step, TryOutcome};
use dictation_pipeline::config::{Config, SETTINGS_FILE};
use dictation_ports::{Permission, PermissionStatus, Permissions, SecretStore};
use serde::{Deserialize, Serialize};

use crate::engine::LiveEngine;

/// The file whose presence makes the next launch show onboarding: left by
/// Relaunch, removed by the launch that reads it.
const RESUME_MARKER: &str = "onboarding-resume";

/// The file that keeps [`SetupProgress`] across launches.
pub const PROGRESS_FILE: &str = "onboarding-progress.json";

/// What the settings window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum View {
    /// The settings form.
    Settings,
    /// The onboarding steps.
    Onboarding,
}

/// A permission's state, for the onboarding steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    /// Granted.
    Granted,
    /// Refused: only System Settings can grant it now.
    Denied,
    /// Not granted yet (for Accessibility, macOS can't tell this from
    /// refused).
    NotDetermined,
}

impl From<PermissionStatus> for PermissionState {
    fn from(status: PermissionStatus) -> Self {
        match status {
            PermissionStatus::Granted => Self::Granted,
            PermissionStatus::Denied => Self::Denied,
            PermissionStatus::NotDetermined => Self::NotDetermined,
        }
    }
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
    pub microphone: PermissionState,
    /// The Accessibility permission.
    pub accessibility: PermissionState,
    /// macOS says Accessibility is allowed but the hotkey can't use it: a
    /// relaunch makes it take.
    pub needs_relaunch: bool,
    /// How the practice dictation went.
    pub try_outcome: TryOutcome,
    /// The user cancelled the model download.
    pub download_paused: bool,
}

/// The OS's permissions, for the onboarding commands.
pub struct OsPermissions(pub Arc<dyn Permissions>);

/// Where the user is in onboarding. Managed state; what matters across
/// launches is in [`PROGRESS_FILE`].
#[derive(Debug)]
pub struct Onboarding {
    state: Mutex<State>,
    /// `None`: kept in memory only (tests).
    file: Option<PathBuf>,
}

#[derive(Debug)]
struct State {
    view: View,
    progress: SetupProgress,
    /// Onboarding was opened at some point, here or in an earlier launch. A
    /// scripted test launch that never opens it isn't setting anything up.
    opened: bool,
}

impl Default for Onboarding {
    fn default() -> Self {
        Self::with(SetupProgress::default(), false, None)
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
    /// The settings view, onboarding not started, kept in memory only.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn with(progress: SetupProgress, opened: bool, file: Option<PathBuf>) -> Self {
        Self {
            state: Mutex::new(State {
                view: View::Settings,
                progress,
                opened,
            }),
            file,
        }
    }

    /// What the last launch kept in `config_dir`. With no file, someone with
    /// a settings file set the app up before these steps existed; anyone else
    /// is new.
    #[must_use]
    pub fn load(config_dir: &Path) -> Self {
        let file = config_dir.join(PROGRESS_FILE);
        let kept = std::fs::read_to_string(&file).ok();
        let (progress, opened) = match kept {
            // A file that can't be understood starts over: the safe way
            // round, since events then wait for Welcome again.
            Some(text) => (SetupProgress::from_json(&text).unwrap_or_default(), true),
            None if config_dir.join(SETTINGS_FILE).exists() => {
                (SetupProgress::ALREADY_SET_UP, false)
            }
            None => (SetupProgress::default(), false),
        };
        Self::with(progress, opened, Some(file))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes `progress` to the file (to a temporary file first, so a crash
    /// can't leave half of it). A failure only costs resuming, so it is
    /// logged and the app goes on.
    fn keep(&self, progress: &SetupProgress) {
        let Some(file) = &self.file else { return };
        let temporary = file.with_extension("json.tmp");
        let written = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&temporary, progress.to_json()))
            .and_then(|()| std::fs::rename(&temporary, file));
        if let Err(error) = written {
            tracing::warn!(%error, "couldn't keep the setup progress");
        }
    }

    /// Switches the window to onboarding, at the step the user was on, and
    /// returns that step.
    pub fn open(&self, readiness: Readiness) -> Step {
        let mut state = self.lock();
        state.progress.step = readiness.resume_at(state.progress.step);
        state.view = View::Onboarding;
        state.opened = true;
        let (step, progress) = (state.progress.step, state.progress);
        drop(state);
        self.keep(&progress);
        step
    }

    /// "Run setup again": setup that was finished goes through the steps from
    /// Welcome. It stays finished, so the tray doesn't start offering "Resume
    /// setup…", and Welcome stays passed, so sending doesn't stop.
    pub fn start_over_if_finished(&self) {
        let mut state = self.lock();
        if !state.progress.completed {
            return;
        }
        state.progress.step = Step::Welcome;
        let progress = state.progress;
        drop(state);
        self.keep(&progress);
    }

    /// Switches the window back to the settings view; the step is kept for
    /// the next [`open`](Self::open).
    pub fn close(&self) {
        self.lock().view = View::Settings;
    }

    /// "Start dictating": setup is done, and the window goes back to the
    /// settings view.
    pub fn finish(&self) {
        let mut state = self.lock();
        state.view = View::Settings;
        state.progress.completed = true;
        let progress = state.progress;
        drop(state);
        self.keep(&progress);
    }

    /// The view.
    #[must_use]
    pub fn view(&self) -> View {
        self.lock().view
    }

    /// Whether the user has continued past Welcome.
    #[must_use]
    pub fn welcome_passed(&self) -> bool {
        self.lock().progress.welcome_passed
    }

    /// Whether setup was finished.
    #[must_use]
    pub fn completed(&self) -> bool {
        self.lock().progress.completed
    }

    /// Whether setup was started and not finished: the tray then offers
    /// "Resume setup…".
    #[must_use]
    pub fn in_progress(&self) -> bool {
        let state = self.lock();
        state.opened && !state.progress.completed
    }

    /// The user continued past Welcome. True the first time.
    pub fn pass_welcome(&self) -> bool {
        let mut state = self.lock();
        if state.progress.welcome_passed {
            return false;
        }
        state.progress.welcome_passed = true;
        let progress = state.progress;
        drop(state);
        self.keep(&progress);
        true
    }

    /// Keeps how the practice dictation went.
    pub fn set_try_outcome(&self, outcome: TryOutcome) {
        let mut state = self.lock();
        state.progress.try_outcome = outcome;
        let progress = state.progress;
        drop(state);
        self.keep(&progress);
    }

    /// Keeps whether the user cancelled the model download.
    pub fn set_download_paused(&self, paused: bool) {
        let mut state = self.lock();
        state.progress.download_paused = paused;
        let progress = state.progress;
        drop(state);
        self.keep(&progress);
    }

    /// What the window shows. A step whose permission was turned off since
    /// comes back (the key isn't read here: it only matters for moving on).
    /// `hotkey_down`: the hotkey isn't working right now.
    pub fn status(&self, permissions: &dyn Permissions, hotkey_down: bool) -> OnboardingStatus {
        let microphone = permissions.check(Permission::Microphone);
        let accessibility = permissions.check(Permission::Accessibility);
        let mut state = self.lock();
        let readiness = Readiness {
            welcome_passed: state.progress.welcome_passed,
            speech_chosen: true,
            microphone: microphone == PermissionStatus::Granted,
            accessibility: accessibility == PermissionStatus::Granted,
        };
        let step = readiness.resume_at(state.progress.step);
        let moved = step != state.progress.step;
        state.progress.step = step;
        let (status, progress) = (
            OnboardingStatus {
                view: state.view,
                step,
                microphone: microphone.into(),
                accessibility: accessibility.into(),
                needs_relaunch: accessibility == PermissionStatus::Granted && hotkey_down,
                try_outcome: state.progress.try_outcome,
                download_paused: state.progress.download_paused,
            },
            state.progress,
        );
        drop(state);
        if moved {
            self.keep(&progress);
        }
        status
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
        let mut state = self.lock();
        state.progress.step = step;
        let progress = state.progress;
        drop(state);
        self.keep(&progress);
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
    let settings = config.settings().unwrap_or_default();
    if settings.transcription == dictation_core::settings::Transcription::Local {
        return true;
    }
    let provider = settings.provider;
    match live.read_key(config, provider) {
        Ok(key) => key.is_some(),
        Err(error) => {
            tracing::warn!(%error, "onboarding: couldn't check for a saved key");
            false
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
        assert_eq!(status.microphone, PermissionState::Denied);
        assert_eq!(status.accessibility, PermissionState::NotDetermined);

        // Granted in System Settings while the window is open.
        permissions.set(Permission::Microphone, Granted);
        let status = onboarding.status(&permissions, false);
        assert_eq!(status.microphone, PermissionState::Granted);
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

    /// U8 criterion 7: every step survives a quit, with the choices made.
    #[test]
    fn quitting_at_any_step_resumes_at_that_step() {
        for step in Step::ALL {
            let dir = temp(&format!("resume-{step:?}"));
            let onboarding = Onboarding::load(&dir);
            onboarding.open(Readiness::default());
            onboarding.pass_welcome();
            onboarding.go_to(step, Readiness::READY).unwrap();
            onboarding.set_try_outcome(TryOutcome::Skipped);
            onboarding.set_download_paused(true);
            drop(onboarding);

            let relaunched = Onboarding::load(&dir);
            assert_eq!(relaunched.open(Readiness::READY), step);
            assert!(relaunched.welcome_passed());
            assert!(relaunched.in_progress());
            let status = relaunched.status(&granted(true, true), false);
            assert_eq!(status.try_outcome, TryOutcome::Skipped);
            assert!(status.download_paused);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_new_user_who_quits_on_welcome_is_still_held_at_welcome() {
        let dir = temp("welcome");
        let onboarding = Onboarding::load(&dir);
        onboarding.open(Readiness::default());
        drop(onboarding);
        let relaunched = Onboarding::load(&dir);
        assert!(!relaunched.welcome_passed());
        assert!(relaunched.in_progress());
        assert_eq!(relaunched.open(Readiness::default()), Step::Welcome);
        let _ = std::fs::remove_dir_all(&dir);
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
    fn someone_who_set_up_before_these_steps_counts_as_done() {
        let dir = temp("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SETTINGS_FILE), "{}").unwrap();
        let onboarding = Onboarding::load(&dir);
        assert!(onboarding.welcome_passed());
        assert!(onboarding.completed());
        assert!(!onboarding.in_progress());
        // Reopened from Settings, Help, they get the summary, or what broke.
        assert_eq!(onboarding.open(Readiness::READY), Step::Ready);
        assert_eq!(
            onboarding.open(Readiness {
                accessibility: false,
                ..Readiness::READY
            }),
            Step::Permissions
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_launch_that_never_opens_onboarding_is_not_in_progress() {
        let dir = temp("scripted");
        assert!(!Onboarding::load(&dir).in_progress());
        assert!(!dir.join(PROGRESS_FILE).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_progress_file_that_cannot_be_read_starts_over() {
        let dir = temp("bad");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(PROGRESS_FILE), "not json").unwrap();
        let onboarding = Onboarding::load(&dir);
        assert!(!onboarding.welcome_passed());
        assert!(!onboarding.completed());
        let _ = std::fs::remove_dir_all(&dir);
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
    fn the_key_counts_for_the_selected_provider_only() {
        let dir = temp("key");
        let secrets = MemSecretStore::new();
        let config = Arc::new(Config::new(&dir, secrets));
        let live = LiveEngine::fixed();
        assert!(!key_saved(&config, &live));
        config
            .set_api_key(dictation_core::settings::Provider::OpenAi, "sk-made-up-key")
            .unwrap();
        // Groq is selected (the default).
        assert!(!key_saved(&config, &live));
        config
            .set_api_key(dictation_core::settings::Provider::Groq, "gsk_made_up_key")
            .unwrap();
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
