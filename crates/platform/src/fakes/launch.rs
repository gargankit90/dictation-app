use std::sync::{Arc, Mutex};

use dictation_ports::{LaunchAtLogin, LaunchAtLoginError, LaunchAtLoginState};

use super::lock;

#[derive(Debug)]
struct State {
    state: LaunchAtLoginState,
    /// What `set_enabled(true)` leaves behind instead of `Enabled`.
    enabling_gives: LaunchAtLoginState,
    fail: Option<LaunchAtLoginError>,
    calls: Vec<bool>,
}

/// A [`LaunchAtLogin`] the test controls: it starts
/// [`Disabled`](LaunchAtLoginState::Disabled), and `set_enabled` flips it.
#[derive(Debug, Clone)]
pub struct FakeLaunchAtLogin {
    state: Arc<Mutex<State>>,
}

impl Default for FakeLaunchAtLogin {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                state: LaunchAtLoginState::Disabled,
                enabling_gives: LaunchAtLoginState::Enabled,
                fail: None,
                calls: Vec::new(),
            })),
        }
    }
}

impl FakeLaunchAtLogin {
    /// Starts disabled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Changes the state behind the app's back, as the user would in System
    /// Settings.
    pub fn set_state(&self, state: LaunchAtLoginState) {
        lock(&self.state).state = state;
    }

    /// Makes every later `set_enabled(true)` end in `state` (say
    /// `RequiresApproval`), without an error.
    pub fn enabling_gives(&self, state: LaunchAtLoginState) {
        lock(&self.state).enabling_gives = state;
    }

    /// Makes every later call fail with `error`.
    pub fn fail_with(&self, error: LaunchAtLoginError) {
        lock(&self.state).fail = Some(error);
    }

    /// The `enabled` arguments of the `set_enabled` calls so far.
    #[must_use]
    pub fn calls(&self) -> Vec<bool> {
        lock(&self.state).calls.clone()
    }
}

impl LaunchAtLogin for FakeLaunchAtLogin {
    fn state(&self) -> Result<LaunchAtLoginState, LaunchAtLoginError> {
        let state = lock(&self.state);
        match &state.fail {
            Some(error) => Err(error.clone()),
            None => Ok(state.state),
        }
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), LaunchAtLoginError> {
        let mut state = lock(&self.state);
        state.calls.push(enabled);
        if let Some(error) = &state.fail {
            return Err(error.clone());
        }
        state.state = if enabled {
            state.enabling_gives
        } else {
            LaunchAtLoginState::Disabled
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_follows_set_enabled_and_scripted_changes() {
        let login = FakeLaunchAtLogin::new();
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Disabled));
        login.set_enabled(true).unwrap();
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Enabled));
        login.set_state(LaunchAtLoginState::Disabled);
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Disabled));
        login.enabling_gives(LaunchAtLoginState::RequiresApproval);
        login.set_enabled(true).unwrap();
        assert_eq!(login.state(), Ok(LaunchAtLoginState::RequiresApproval));
        login.set_enabled(false).unwrap();
        assert_eq!(login.calls(), [true, true, false]);
        login.fail_with(LaunchAtLoginError::Unsupported);
        assert_eq!(login.state(), Err(LaunchAtLoginError::Unsupported));
        assert_eq!(
            login.set_enabled(true),
            Err(LaunchAtLoginError::Unsupported)
        );
    }
}
