//! The launch-at-login setting (M5.3): what the settings screen shows and
//! what saving does to the OS.
//!
//! The settings file keeps the last value applied, but the user can switch
//! the login item off in System Settings, so the screen shows the OS's own
//! state ([`with_os_state`]). Changing the OS can take a long time
//! (unregistering the login item has taken 40 s, #238), so it is its own
//! command: [`LoginItem::set`] is a plain blocking call the shell runs off
//! the main thread, and Settings disables the switch until it answers.

use std::sync::{Arc, Mutex};

use dictation_core::settings::Settings;
use dictation_ports::{LaunchAtLogin, LaunchAtLoginError, LaunchAtLoginState};
use thiserror::Error;

/// Why saving the launch-at-login choice didn't take effect. The text is for
/// the user.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LaunchProblem {
    /// This OS has no login item support yet.
    #[error("Launch at login isn't available on this system yet.")]
    Unsupported,
    /// The OS refused.
    #[error("Couldn't change launch at login: {0}")]
    Os(String),
    /// Registered, but the user has to allow it in the OS settings.
    #[error(
        "Launch at login needs your approval: open System Settings → General → Login Items, and allow Vocet."
    )]
    NeedsApproval,
    /// The OS said yes, but its state didn't change.
    #[error("The system didn't change launch at login. Try again.")]
    NotApplied,
    /// The OS changed, but the setting couldn't be written to the settings
    /// file (S1 review, R2).
    #[error(
        "Launch at login is {} now, but Vocet couldn't save that setting. Try again, or see Troubleshoot.",
        if *enabled { "on" } else { "off" }
    )]
    NotStored {
        /// What the OS has now.
        enabled: bool,
        /// Why the write failed: for the log, not shown to the user.
        reason: String,
    },
}

impl From<LaunchAtLoginError> for LaunchProblem {
    fn from(error: LaunchAtLoginError) -> Self {
        match error {
            LaunchAtLoginError::Unsupported => Self::Unsupported,
            LaunchAtLoginError::Os(message) => Self::Os(message),
        }
    }
}

/// `saved` with `launch_at_login` set to what the OS says. If the OS can't
/// say, the saved value stays.
#[must_use]
pub fn with_os_state(login: &dyn LaunchAtLogin, saved: Settings) -> Settings {
    reconcile(saved, login.state())
}

fn reconcile(
    mut saved: Settings,
    state: Result<LaunchAtLoginState, LaunchAtLoginError>,
) -> Settings {
    match state {
        Ok(state) => {
            if state.is_enabled() != saved.launch_at_login {
                tracing::info!(
                    saved = saved.launch_at_login,
                    os = state.is_enabled(),
                    "launch at login was changed outside the app"
                );
            }
            saved.launch_at_login = state.is_enabled();
        }
        Err(error) => tracing::debug!(%error, "can't read the login item; using the saved value"),
    }
    saved
}

/// What a change of the login item ended in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Whether the app starts at login now, as the OS says (or, if it can't
    /// say, as it was before).
    pub enabled: bool,
    /// Why the wish didn't come true, if it didn't.
    pub problem: Option<LaunchProblem>,
}

/// Makes the OS match `wanted` and says what it really ended up with, so a
/// stored flag never claims more than is true. Does nothing to the OS if it
/// already matches.
#[must_use]
pub fn change(login: &dyn LaunchAtLogin, wanted: bool) -> Outcome {
    let before = login.state().ok();
    // A registered item the user hasn't approved is cleaned up when they
    // switch the setting off, and tried again when they switch it on.
    let in_sync = match before {
        Some(LaunchAtLoginState::Enabled) => wanted,
        Some(LaunchAtLoginState::Disabled) => !wanted,
        Some(LaunchAtLoginState::RequiresApproval) => false,
        // The OS can't say (no adapter, say): nothing to switch off.
        None => !wanted,
    };
    let was = before.is_some_and(LaunchAtLoginState::is_enabled);
    if in_sync {
        return Outcome {
            enabled: was,
            problem: None,
        };
    }
    if let Err(error) = login.set_enabled(wanted) {
        return Outcome {
            enabled: was,
            problem: Some(error.into()),
        };
    }
    let after = match login.state() {
        Ok(after) => after,
        Err(error) => {
            return Outcome {
                enabled: was,
                problem: Some(error.into()),
            };
        }
    };
    let problem = match after {
        LaunchAtLoginState::RequiresApproval if wanted => Some(LaunchProblem::NeedsApproval),
        // Switching off leaves an unapproved item the user must remove
        // themselves; nothing will start, which is what they asked for.
        LaunchAtLoginState::RequiresApproval => None,
        state if state.is_enabled() == wanted => None,
        _ => Some(LaunchProblem::NotApplied),
    };
    Outcome {
        enabled: after.is_enabled(),
        problem,
    }
}

/// [`change`] for `settings.launch_at_login`, which then holds what the OS
/// ended up with.
///
/// # Errors
///
/// A [`LaunchProblem`] saying why the wish didn't come true; `settings` holds
/// the OS's actual state anyway.
pub fn apply(login: &dyn LaunchAtLogin, settings: &mut Settings) -> Result<(), LaunchProblem> {
    let outcome = change(login, settings.launch_at_login);
    settings.launch_at_login = outcome.enabled;
    outcome.problem.map_or(Ok(()), Err)
}

/// The OS's login item, changed one call at a time.
pub struct LoginItem {
    login: Arc<dyn LaunchAtLogin>,
    /// Held while the OS changes the item, so the screen doesn't wait on it.
    changing: Mutex<()>,
}

impl LoginItem {
    /// The login item behind `login`.
    #[must_use]
    pub fn new(login: Arc<dyn LaunchAtLogin>) -> Self {
        Self {
            login,
            changing: Mutex::new(()),
        }
    }

    /// `saved` as Settings shows it: with the OS's state ([`with_os_state`]),
    /// except while a change runs, when the OS's answer would wait for it
    /// and `saved` is shown instead. Blocks on the OS; call it off the main
    /// thread.
    #[must_use]
    pub fn shown(&self, saved: Settings) -> Settings {
        if self.changing.try_lock().is_err() {
            return saved;
        }
        with_os_state(&*self.login, saved)
    }

    /// Makes the OS start the app at login, or not, then has `store` keep
    /// what the OS ended up with. Blocks, for up to tens of seconds: call it
    /// off the main thread. A second call waits for the first.
    ///
    /// # Errors
    ///
    /// A [`LaunchProblem`] saying why the wish didn't come true, or that the
    /// result couldn't be stored.
    pub fn set(
        &self,
        wanted: bool,
        store: impl FnOnce(bool) -> Result<(), String>,
    ) -> Result<bool, LaunchProblem> {
        let _changing = self
            .changing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let started = std::time::Instant::now();
        let outcome = change(&*self.login, wanted);
        tracing::info!(
            wanted,
            enabled = outcome.enabled,
            elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "login item changed"
        );
        if let Err(reason) = store(outcome.enabled) {
            tracing::warn!(%reason, "could not store the launch-at-login result");
            return Err(outcome.problem.unwrap_or(LaunchProblem::NotStored {
                enabled: outcome.enabled,
                reason,
            }));
        }
        outcome.problem.map_or(Ok(outcome.enabled), Err)
    }
}

#[cfg(test)]
mod tests {
    use dictation_platform::fakes::FakeLaunchAtLogin;

    use super::*;

    fn settings(launch_at_login: bool) -> Settings {
        Settings {
            launch_at_login,
            ..Settings::default()
        }
    }

    #[test]
    fn the_screen_shows_the_os_state_not_the_saved_flag() {
        let login = FakeLaunchAtLogin::new();
        // Saved on, but the user removed it in System Settings.
        assert!(!with_os_state(&login, settings(true)).launch_at_login);
        login.set_state(LaunchAtLoginState::Enabled);
        assert!(with_os_state(&login, settings(false)).launch_at_login);
        // Awaiting approval is not on.
        login.set_state(LaunchAtLoginState::RequiresApproval);
        assert!(!with_os_state(&login, settings(true)).launch_at_login);
    }

    #[test]
    fn when_the_os_cannot_say_the_saved_flag_stays() {
        let login = FakeLaunchAtLogin::new();
        login.fail_with(LaunchAtLoginError::Unsupported);
        assert!(with_os_state(&login, settings(true)).launch_at_login);
        assert!(!with_os_state(&login, settings(false)).launch_at_login);
    }

    #[test]
    fn turning_it_on_and_off_registers_and_unregisters() {
        let login = FakeLaunchAtLogin::new();
        let mut on = settings(true);
        apply(&login, &mut on).unwrap();
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Enabled));
        assert!(on.launch_at_login);

        let mut off = settings(false);
        apply(&login, &mut off).unwrap();
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Disabled));
        assert!(!off.launch_at_login);
        assert_eq!(login.calls(), [true, false]);
    }

    #[test]
    fn an_unchanged_choice_does_not_touch_the_os() {
        let login = FakeLaunchAtLogin::new();
        apply(&login, &mut settings(false)).unwrap();
        login.set_state(LaunchAtLoginState::Enabled);
        apply(&login, &mut settings(true)).unwrap();
        assert!(login.calls().is_empty());
    }

    #[test]
    fn an_os_refusal_is_reported_and_the_flag_keeps_the_real_state() {
        let login = FakeLaunchAtLogin::new();
        login.set_state(LaunchAtLoginState::Enabled);
        let mut off = settings(false);
        // set_enabled fails but state() still answers: script that with a
        // failing fake that is cleared for the read.
        let broken = RefusingLogin(login.clone());
        let error = apply(&broken, &mut off).unwrap_err();
        assert_eq!(error, LaunchProblem::Os("no bundle".into()));
        assert!(error.to_string().contains("no bundle"));
        assert!(off.launch_at_login, "still on in the OS");
    }

    /// Reads fine, refuses every change.
    struct RefusingLogin(FakeLaunchAtLogin);

    impl LaunchAtLogin for RefusingLogin {
        fn state(&self) -> Result<LaunchAtLoginState, LaunchAtLoginError> {
            self.0.state()
        }

        fn set_enabled(&self, _: bool) -> Result<(), LaunchAtLoginError> {
            Err(LaunchAtLoginError::Os("no bundle".into()))
        }
    }

    #[test]
    fn needing_approval_is_an_error_and_not_on() {
        let login = FakeLaunchAtLogin::new();
        login.enabling_gives(LaunchAtLoginState::RequiresApproval);
        let mut on = settings(true);
        assert_eq!(apply(&login, &mut on), Err(LaunchProblem::NeedsApproval));
        assert!(!on.launch_at_login);
        assert!(
            LaunchProblem::NeedsApproval
                .to_string()
                .contains("Login Items")
        );
    }

    #[test]
    fn switching_off_an_unapproved_item_removes_it() {
        let login = FakeLaunchAtLogin::new();
        login.set_state(LaunchAtLoginState::RequiresApproval);
        let mut off = settings(false);
        apply(&login, &mut off).unwrap();
        assert_eq!(login.calls(), [false]);
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Disabled));
    }

    #[test]
    fn no_adapter_means_unsupported() {
        let login = FakeLaunchAtLogin::new();
        login.fail_with(LaunchAtLoginError::Unsupported);
        let mut on = settings(true);
        assert_eq!(apply(&login, &mut on), Err(LaunchProblem::Unsupported));
        assert!(!on.launch_at_login);
        // Off needs nothing from an OS that can't say, so saving other
        // settings there works.
        let mut off = settings(false);
        apply(&login, &mut off).unwrap();
        assert_eq!(
            login.calls(),
            [true],
            "only the failed attempt to turn it on"
        );
    }

    #[test]
    fn a_state_that_does_not_change_is_reported() {
        let login = FakeLaunchAtLogin::new();
        login.enabling_gives(LaunchAtLoginState::Disabled);
        let mut on = settings(true);
        assert_eq!(apply(&login, &mut on), Err(LaunchProblem::NotApplied));
        assert!(!on.launch_at_login);
    }

    #[test]
    fn set_changes_the_os_and_stores_what_it_has() {
        let login = FakeLaunchAtLogin::new();
        let item = LoginItem::new(Arc::new(login.clone()));
        let stored = Mutex::new(Vec::new());
        let store = |enabled| {
            stored.lock().unwrap().push(enabled);
            Ok(())
        };
        assert_eq!(item.set(true, store), Ok(true));
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Enabled));
        assert_eq!(item.set(false, store), Ok(false));
        assert_eq!(*stored.lock().unwrap(), [true, false]);
        // The screen follows the OS, whatever the file says.
        assert!(!item.shown(settings(true)).launch_at_login);
    }

    #[test]
    fn a_refusal_stores_the_real_state_and_says_why() {
        let login = FakeLaunchAtLogin::new();
        login.enabling_gives(LaunchAtLoginState::RequiresApproval);
        let item = LoginItem::new(Arc::new(login));
        let stored = Mutex::new(None);
        let result = item.set(true, |enabled| {
            *stored.lock().unwrap() = Some(enabled);
            Ok(())
        });
        assert_eq!(result, Err(LaunchProblem::NeedsApproval));
        assert_eq!(*stored.lock().unwrap(), Some(false));
    }

    /// S1 review, R2: an unwritable settings file after the OS changed is
    /// an error the user sees, with the OS's state in it.
    #[test]
    fn a_failed_store_is_reported_with_the_real_state() {
        let login = FakeLaunchAtLogin::new();
        let item = LoginItem::new(Arc::new(login.clone()));
        let error = item.set(true, |_| Err("disk full".to_owned())).unwrap_err();
        assert_eq!(
            error,
            LaunchProblem::NotStored {
                enabled: true,
                reason: "disk full".to_owned()
            }
        );
        assert!(error.to_string().contains("Launch at login is on now"));
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Enabled));
        // A refusal says why, even if storing failed too.
        let refused = LoginItem::new(Arc::new(RefusingLogin(login)));
        assert_eq!(
            refused.set(false, |_| Err("disk full".to_owned())),
            Err(LaunchProblem::Os("no bundle".into()))
        );
    }

    /// #238: while the OS changes the item, the screen shows the saved
    /// value instead of waiting on the OS.
    #[test]
    fn the_screen_does_not_wait_for_a_change() {
        let login = FakeLaunchAtLogin::new();
        login.delay_changes(std::time::Duration::from_millis(300));
        let item = Arc::new(LoginItem::new(Arc::new(login)));
        let setting = {
            let item = Arc::clone(&item);
            std::thread::spawn(move || item.set(true, |_| Ok(())))
        };
        std::thread::sleep(std::time::Duration::from_millis(50));
        let started = std::time::Instant::now();
        assert!(!item.shown(settings(false)).launch_at_login);
        assert!(started.elapsed() < std::time::Duration::from_millis(200));
        assert_eq!(setting.join().unwrap(), Ok(true));
        assert!(item.shown(settings(false)).launch_at_login);
    }
}
