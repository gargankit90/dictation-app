//! The launch-at-login setting (M5.3): what the settings screen shows and
//! what saving does to the OS.
//!
//! The settings file keeps the last value applied, but the user can switch
//! the login item off in System Settings, so the screen always shows the OS's
//! own state ([`with_os_state`]) and a save changes the OS first, then keeps
//! whatever the OS ended up with ([`apply`]).

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
        "Launch at login needs your approval: open System Settings, General, Login Items, and allow Vocet."
    )]
    NeedsApproval,
    /// The OS said yes, but its state didn't change.
    #[error("The system didn't change launch at login. Try again.")]
    NotApplied,
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
pub fn with_os_state(login: &dyn LaunchAtLogin, mut saved: Settings) -> Settings {
    match login.state() {
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

/// Makes the OS match `settings.launch_at_login`, then sets that field to
/// what the OS really ended up with, so the file never claims more than is
/// true. Does nothing to the OS if it already matches.
///
/// # Errors
///
/// A [`LaunchProblem`] saying why the wish didn't come true; `settings` holds
/// the OS's actual state anyway.
pub fn apply(login: &dyn LaunchAtLogin, settings: &mut Settings) -> Result<(), LaunchProblem> {
    let wanted = settings.launch_at_login;
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
    if in_sync {
        return Ok(());
    }
    if let Err(error) = login.set_enabled(wanted) {
        settings.launch_at_login = before.is_some_and(LaunchAtLoginState::is_enabled);
        return Err(error.into());
    }
    let after = login.state()?;
    settings.launch_at_login = after.is_enabled();
    match after {
        LaunchAtLoginState::RequiresApproval if wanted => Err(LaunchProblem::NeedsApproval),
        // Switching off leaves an unapproved item the user must remove
        // themselves; nothing will start, which is what they asked for.
        LaunchAtLoginState::RequiresApproval => Ok(()),
        state if state.is_enabled() == wanted => Ok(()),
        _ => Err(LaunchProblem::NotApplied),
    }
}

/// Puts the login item back to `before` (what [`LaunchAtLogin::state`] said
/// before [`apply`]), for a save whose settings file couldn't be written
/// after the OS was changed (T3, #121): the file and the OS then still agree
/// on the old choice. Does nothing if the OS is already there, or if `before`
/// is unknown and the item is off.
///
/// # Errors
///
/// A [`LaunchProblem`] if the OS refuses or ends up elsewhere; the caller
/// reports its own error first.
pub fn restore(
    login: &dyn LaunchAtLogin,
    before: Option<LaunchAtLoginState>,
) -> Result<(), LaunchProblem> {
    let now = login.state()?;
    if Some(now) == before || (before.is_none() && !now.is_enabled()) {
        return Ok(());
    }
    // An item that was waiting for approval is registered again, so it
    // waits again; one the OS couldn't describe was off as far as we know.
    let enabled = matches!(
        before,
        Some(LaunchAtLoginState::Enabled | LaunchAtLoginState::RequiresApproval)
    );
    login.set_enabled(enabled)?;
    let after = login.state()?;
    if before.map_or(!after.is_enabled(), |before| after == before) {
        Ok(())
    } else {
        Err(LaunchProblem::NotApplied)
    }
}

/// Why the login item couldn't be put back after a failed save, and where
/// the OS left it. The text is for the user (T3 round 2).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{}", describe_not_restored(problem, *now))]
pub struct NotRestored {
    /// Why [`restore`] failed.
    pub problem: LaunchProblem,
    /// The OS's state afterwards, if it can say.
    pub now: Option<LaunchAtLoginState>,
}

fn describe_not_restored(problem: &LaunchProblem, now: Option<LaunchAtLoginState>) -> String {
    let state = match now {
        Some(LaunchAtLoginState::Enabled) => "so Vocet still opens at login; Settings shows it on",
        Some(LaunchAtLoginState::Disabled) => {
            "so Vocet doesn't open at login; Settings shows it off"
        }
        Some(LaunchAtLoginState::RequiresApproval) => {
            "so it waits for your approval in System Settings, General, Login Items"
        }
        None => "and the system can't say its state: check System Settings, General, Login Items",
    };
    format!("Launch at login couldn't be put back ({problem}), {state}.")
}

/// [`restore`], with the OS's state read back if it fails, so the caller
/// can tell the user where the login item really is.
///
/// # Errors
///
/// [`NotRestored`] if [`restore`] fails.
pub fn restore_or_report(
    login: &dyn LaunchAtLogin,
    before: Option<LaunchAtLoginState>,
) -> Result<(), NotRestored> {
    restore(login, before).map_err(|problem| {
        let now = login.state().ok();
        tracing::warn!(%problem, ?now, "could not put the login item back after a failed save");
        NotRestored { problem, now }
    })
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
    fn restore_puts_back_the_state_from_before_the_save() {
        for before in [
            LaunchAtLoginState::Disabled,
            LaunchAtLoginState::Enabled,
            LaunchAtLoginState::RequiresApproval,
        ] {
            let login = FakeLaunchAtLogin::new();
            login.set_state(before);
            if before == LaunchAtLoginState::RequiresApproval {
                login.enabling_gives(LaunchAtLoginState::RequiresApproval);
            }
            let mut flipped = settings(!before.is_enabled());
            let _ = apply(&login, &mut flipped);
            restore(&login, Some(before)).unwrap();
            assert_eq!(login.state(), Ok(before), "from {before:?}");
        }
    }

    #[test]
    fn restore_leaves_an_os_that_is_already_there_alone() {
        let login = FakeLaunchAtLogin::new();
        restore(&login, Some(LaunchAtLoginState::Disabled)).unwrap();
        restore(&login, None).unwrap();
        assert!(login.calls().is_empty());
    }

    #[test]
    fn restore_reports_an_os_that_refuses_or_ends_up_elsewhere() {
        let login = FakeLaunchAtLogin::new();
        login.set_state(LaunchAtLoginState::Enabled);
        assert_eq!(
            restore(
                &RefusingLogin(login.clone()),
                Some(LaunchAtLoginState::Disabled)
            ),
            Err(LaunchProblem::Os("no bundle".into()))
        );
        login.set_state(LaunchAtLoginState::Disabled);
        login.enabling_gives(LaunchAtLoginState::RequiresApproval);
        assert_eq!(
            restore(&login, Some(LaunchAtLoginState::Enabled)),
            Err(LaunchProblem::NotApplied)
        );
    }

    #[test]
    fn a_failed_restore_says_where_the_login_item_was_left() {
        let login = FakeLaunchAtLogin::new();
        restore_or_report(&login, Some(LaunchAtLoginState::Disabled)).unwrap();

        for (now, says) in [
            (LaunchAtLoginState::Enabled, "still opens at login"),
            (LaunchAtLoginState::Disabled, "doesn't open at login"),
            (
                LaunchAtLoginState::RequiresApproval,
                "waits for your approval",
            ),
        ] {
            let login = FakeLaunchAtLogin::new();
            login.set_state(now);
            // Any other state than `now` needs a change, which is refused.
            let before = if now == LaunchAtLoginState::Enabled {
                LaunchAtLoginState::Disabled
            } else {
                LaunchAtLoginState::Enabled
            };
            let failed = restore_or_report(&RefusingLogin(login), Some(before)).unwrap_err();
            assert_eq!(failed.problem, LaunchProblem::Os("no bundle".into()));
            assert_eq!(failed.now, Some(now));
            let message = failed.to_string();
            assert!(message.contains("couldn't be put back"), "{message}");
            assert!(message.contains("no bundle"), "{message}");
            assert!(message.contains(says), "{now:?}: {message}");
        }

        let login = FakeLaunchAtLogin::new();
        login.fail_with(LaunchAtLoginError::Os("gone".into()));
        let failed = restore_or_report(&login, Some(LaunchAtLoginState::Enabled)).unwrap_err();
        assert_eq!(failed.now, None);
        assert!(failed.to_string().contains("can't say"), "{failed}");
    }
}
