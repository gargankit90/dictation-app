//! Onboarding (M5.2): the first-run steps in the settings window.
//!
//! The `main` window shows one of two views, the settings form or the
//! onboarding steps ([`View`]); the app decides which (at launch, from the
//! tray) and the window asks with `get_onboarding` whenever it is shown, so
//! no window needs to listen to events and the window's capability stays
//! "the app's commands and nothing else".
//!
//! The steps and their order are `dictation_core::onboarding`. The app keeps
//! the step the user is on ([`Onboarding`]) while it runs, so going back and
//! reopening from the tray resume where the user was, unless a permission
//! has been turned off since (then that step comes back). The conditions are
//! read each time: the permissions from the OS (cheap, never blocks), the
//! key from the keychain only when the user moves past the key step (bounded
//! like the other key reads).
//!
//! Accessibility (#50): macOS reports a grant made in System Settings to the
//! running app (`AXIsProcessTrusted` turns true; the hotkey supervisor from
//! M5.5b starts the event tap by itself), so the step moves on by itself. In
//! case it doesn't, the step also offers Relaunch: [`relaunch`] leaves a
//! marker in the config directory and restarts the app, and the new launch
//! shows onboarding again ([`take_resume_marker`]).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use dictation_core::onboarding::{Readiness, Step};
use dictation_pipeline::config::Config;
use dictation_ports::{Permission, PermissionStatus, Permissions, SecretStore};
use serde::{Deserialize, Serialize};

use crate::engine::LiveEngine;

/// The file whose presence makes the next launch show onboarding: left by
/// Relaunch, removed by the launch that reads it.
const RESUME_MARKER: &str = "onboarding-resume";

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
}

/// The OS's permissions, for the onboarding commands.
pub struct OsPermissions(pub Arc<dyn Permissions>);

/// Where the user is in onboarding, while the app runs. Managed state.
#[derive(Debug)]
pub struct Onboarding {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    view: View,
    /// `None` until onboarding is first opened.
    step: Option<Step>,
}

impl Default for Onboarding {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                view: View::Settings,
                step: None,
            }),
        }
    }
}

/// Why the user can't go to a step yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotYet(pub Step);

impl std::fmt::Display for NotYet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = match self.0 {
            Step::Microphone => "Allow the microphone first.",
            Step::Accessibility => "Allow Accessibility first.",
            Step::Key => "Add a working API key first.",
            Step::Try => "Finish the steps before this one first.",
        };
        f.write_str(what)
    }
}

impl Onboarding {
    /// The settings view, onboarding not started.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Switches the window to onboarding, at the step the user was on (the
    /// first step not done, the first time), and returns that step.
    pub fn open(&self, readiness: Readiness) -> Step {
        let mut state = self.lock();
        let step = readiness.resume_at(state.step.unwrap_or_else(|| readiness.first_unmet()));
        state.view = View::Onboarding;
        state.step = Some(step);
        step
    }

    /// Switches the window back to the settings view; the step is kept for
    /// the next [`open`](Self::open).
    pub fn close(&self) {
        self.lock().view = View::Settings;
    }

    /// The view.
    #[must_use]
    pub fn view(&self) -> View {
        self.lock().view
    }

    /// What the window shows. A step whose permission was turned off since
    /// comes back (the key isn't read here: it only matters for moving on).
    pub fn status(&self, permissions: &dyn Permissions) -> OnboardingStatus {
        let microphone = permissions.check(Permission::Microphone);
        let accessibility = permissions.check(Permission::Accessibility);
        let readiness = Readiness {
            microphone: microphone == PermissionStatus::Granted,
            accessibility: accessibility == PermissionStatus::Granted,
            key_saved: true,
        };
        let mut state = self.lock();
        let step = readiness.resume_at(state.step.unwrap_or_else(|| readiness.first_unmet()));
        state.step = Some(step);
        OnboardingStatus {
            view: state.view,
            step,
            microphone: microphone.into(),
            accessibility: accessibility.into(),
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
        self.lock().step = Some(step);
        Ok(step)
    }
}

/// What the app has of what dictation needs. `read_key` is asked only if
/// `with_key`; otherwise the key counts as saved.
pub fn readiness(
    permissions: &dyn Permissions,
    with_key: bool,
    read_key: impl FnOnce() -> bool,
) -> Readiness {
    Readiness {
        microphone: permissions.check(Permission::Microphone) == PermissionStatus::Granted,
        accessibility: permissions.check(Permission::Accessibility) == PermissionStatus::Granted,
        key_saved: !with_key || read_key(),
    }
}

/// Whether the provider in the settings has a key saved. A keychain that
/// can't answer (locked, timed out) counts as no key: onboarding then asks
/// for one, and saving it again does no harm.
pub fn key_saved<S: SecretStore + 'static>(config: &Arc<Config<S>>, live: &LiveEngine) -> bool {
    let provider = config.settings().unwrap_or_default().provider;
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
            "macOS didn't show its prompt. Open System Settings, Privacy & Security, and allow dictation-app there.".to_owned()
        })
    };
    let settings = |permission| {
        open_settings(permission).map_err(|error| {
            tracing::warn!(%error, ?permission, "onboarding: couldn't open System Settings");
            "Couldn't open System Settings. Open it yourself: Privacy & Security, then allow dictation-app there.".to_owned()
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

/// Whether onboarding shows at launch: when dictation can't work yet, or the
/// last launch was Relaunch. A scripted test launch (`scripted`) gets it only
/// after Relaunch, so it can't take the focus from the window the test
/// pastes into.
#[must_use]
pub fn show_at_launch(readiness: Readiness, resumed: bool, scripted: bool) -> bool {
    resumed || (!scripted && readiness.needs_onboarding())
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

    #[test]
    fn the_window_starts_on_settings_and_opens_at_the_first_missing_step() {
        let onboarding = Onboarding::new();
        assert_eq!(onboarding.view(), View::Settings);
        let step = onboarding.open(Readiness {
            microphone: true,
            ..Readiness::default()
        });
        assert_eq!(step, Step::Accessibility);
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
        let status = onboarding.status(&permissions);
        assert_eq!(status.view, View::Onboarding);
        assert_eq!(status.step, Step::Microphone);
        assert_eq!(status.microphone, PermissionState::Denied);
        assert_eq!(status.accessibility, PermissionState::NotDetermined);

        // Granted in System Settings while the window is open.
        permissions.set(Permission::Microphone, Granted);
        let status = onboarding.status(&permissions);
        assert_eq!(status.microphone, PermissionState::Granted);
        // Still on the step until the user (or the window) moves on.
        assert_eq!(status.step, Step::Microphone);
    }

    #[test]
    fn each_step_is_reached_only_when_the_ones_before_are_done() {
        let onboarding = Onboarding::new();
        let none = Readiness::default();
        onboarding.open(none);
        assert_eq!(
            onboarding.go_to(Step::Accessibility, none),
            Err(NotYet(Step::Microphone))
        );
        let microphone = Readiness {
            microphone: true,
            ..none
        };
        assert_eq!(
            onboarding.go_to(Step::Accessibility, microphone),
            Ok(Step::Accessibility)
        );
        assert_eq!(
            onboarding.go_to(Step::Key, microphone),
            Err(NotYet(Step::Accessibility))
        );
        let permissions = Readiness {
            accessibility: true,
            ..microphone
        };
        assert_eq!(onboarding.go_to(Step::Key, permissions), Ok(Step::Key));
        assert_eq!(
            onboarding.go_to(Step::Try, permissions),
            Err(NotYet(Step::Key))
        );
        assert_eq!(onboarding.go_to(Step::Try, Readiness::READY), Ok(Step::Try));
        // Back is always allowed.
        assert_eq!(
            onboarding.go_to(Step::Microphone, none),
            Ok(Step::Microphone)
        );
    }

    #[test]
    fn reopening_resumes_where_the_user_was() {
        let onboarding = Onboarding::new();
        onboarding.open(Readiness::READY);
        onboarding.go_to(Step::Key, Readiness::READY).unwrap();
        // The user went back a step, then closed the window.
        onboarding
            .go_to(Step::Accessibility, Readiness::READY)
            .unwrap();
        onboarding.close();
        assert_eq!(onboarding.open(Readiness::READY), Step::Accessibility);
        assert_eq!(
            onboarding.status(&granted(true, true)).step,
            Step::Accessibility
        );
    }

    #[test]
    fn a_permission_turned_off_since_brings_its_step_back() {
        let onboarding = Onboarding::new();
        onboarding.open(Readiness::READY);
        onboarding.go_to(Step::Try, Readiness::READY).unwrap();
        // Accessibility turned off in System Settings.
        let permissions = granted(true, false);
        assert_eq!(onboarding.status(&permissions).step, Step::Accessibility);
        // And it stays there once reopened.
        onboarding.close();
        assert_eq!(
            onboarding.open(readiness(&permissions, false, || true)),
            Step::Accessibility
        );
    }

    #[test]
    fn readiness_reads_the_key_only_when_asked() {
        let asked = Cell::new(false);
        let permissions = granted(true, false);
        let ready = readiness(&permissions, false, || {
            asked.set(true);
            false
        });
        assert!(!asked.get());
        assert_eq!(
            ready,
            Readiness {
                microphone: true,
                accessibility: false,
                key_saved: true
            }
        );
        assert!(!readiness(&permissions, true, || false).key_saved);
        assert!(readiness(&permissions, true, || true).key_saved);
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
            key_saved: false,
            ..Readiness::READY
        };
        assert!(show_at_launch(missing, false, false));
        assert!(!show_at_launch(Readiness::READY, false, false));
        // After Relaunch, even with everything in place.
        assert!(show_at_launch(Readiness::READY, true, false));
        // A scripted test launch only after Relaunch.
        assert!(!show_at_launch(missing, false, true));
        assert!(show_at_launch(Readiness::READY, true, true));
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
            NotYet(Step::Key).to_string(),
            "Add a working API key first."
        );
        for step in Step::ALL {
            assert!(NotYet(step).to_string().ends_with("first."));
        }
    }
}
