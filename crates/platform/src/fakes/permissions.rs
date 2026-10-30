use std::sync::{Arc, Mutex};

use dictation_ports::{Permission, PermissionError, PermissionStatus, Permissions};

use super::lock;

/// [`Permissions`] the test sets. Every permission starts
/// [`NotDetermined`](PermissionStatus::NotDetermined); a
/// [`request`](Permissions::request) for an undetermined one answers with
/// [`Granted`](PermissionStatus::Granted) unless
/// [`answer_requests_with`](Self::answer_requests_with) says otherwise.
#[derive(Debug, Clone)]
pub struct FakePermissions {
    state: Arc<Mutex<State>>,
}

#[derive(Debug)]
struct State {
    microphone: PermissionStatus,
    accessibility: PermissionStatus,
    answer: PermissionStatus,
    requests: Vec<Permission>,
    fail_requests: bool,
}

impl State {
    fn status(&mut self, permission: Permission) -> &mut PermissionStatus {
        match permission {
            Permission::Microphone => &mut self.microphone,
            Permission::Accessibility => &mut self.accessibility,
        }
    }
}

impl Default for FakePermissions {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                microphone: PermissionStatus::NotDetermined,
                accessibility: PermissionStatus::NotDetermined,
                answer: PermissionStatus::Granted,
                requests: Vec::new(),
                fail_requests: false,
            })),
        }
    }
}

impl FakePermissions {
    /// Nothing asked yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything granted.
    #[must_use]
    pub fn all_granted() -> Self {
        let permissions = Self::new();
        permissions.set(Permission::Microphone, PermissionStatus::Granted);
        permissions.set(Permission::Accessibility, PermissionStatus::Granted);
        permissions
    }

    /// Sets `permission`'s status, as if the user changed it in System
    /// Settings.
    pub fn set(&self, permission: Permission, status: PermissionStatus) {
        *lock(&self.state).status(permission) = status;
    }

    /// What the user answers to future prompts.
    pub fn answer_requests_with(&self, answer: PermissionStatus) {
        lock(&self.state).answer = answer;
    }

    /// Makes every request fail with [`PermissionError::RequestFailed`].
    pub fn fail_requests(&self, fail: bool) {
        lock(&self.state).fail_requests = fail;
    }

    /// Every permission requested so far, oldest first.
    #[must_use]
    pub fn requests(&self) -> Vec<Permission> {
        lock(&self.state).requests.clone()
    }
}

impl Permissions for FakePermissions {
    fn check(&self, permission: Permission) -> PermissionStatus {
        *lock(&self.state).status(permission)
    }

    fn request(&self, permission: Permission) -> Result<(), PermissionError> {
        let mut state = lock(&self.state);
        if state.fail_requests {
            return Err(PermissionError::RequestFailed("prompt unavailable".into()));
        }
        state.requests.push(permission);
        let answer = state.answer;
        let status = state.status(permission);
        // Like macOS: once answered, asking again doesn't prompt.
        if *status == PermissionStatus::NotDetermined {
            *status = answer;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Permission::{Accessibility, Microphone};
    use PermissionStatus::{Denied, Granted, NotDetermined};

    #[test]
    fn starts_undetermined_and_reports_what_was_set() {
        let permissions = FakePermissions::new();
        assert_eq!(permissions.check(Microphone), NotDetermined);
        assert_eq!(permissions.check(Accessibility), NotDetermined);

        permissions.set(Microphone, Denied);
        assert_eq!(permissions.check(Microphone), Denied);
        assert_eq!(permissions.check(Accessibility), NotDetermined);

        let granted = FakePermissions::all_granted();
        assert_eq!(granted.check(Microphone), Granted);
        assert_eq!(granted.check(Accessibility), Granted);
    }

    #[test]
    fn request_answers_only_undetermined_permissions() {
        let permissions = FakePermissions::new();
        permissions.set(Accessibility, Denied);
        permissions.answer_requests_with(Granted);

        permissions.request(Microphone).unwrap();
        permissions.request(Accessibility).unwrap();
        assert_eq!(permissions.check(Microphone), Granted);
        assert_eq!(permissions.check(Accessibility), Denied);
        assert_eq!(permissions.requests(), [Microphone, Accessibility]);
    }

    #[test]
    fn user_can_deny_the_prompt() {
        let permissions = FakePermissions::new();
        permissions.answer_requests_with(Denied);
        permissions.request(Microphone).unwrap();
        assert_eq!(permissions.check(Microphone), Denied);
    }

    #[test]
    fn failed_request_changes_nothing() {
        let permissions = FakePermissions::new();
        permissions.fail_requests(true);
        assert!(matches!(
            permissions.request(Microphone),
            Err(PermissionError::RequestFailed(_))
        ));
        assert_eq!(permissions.check(Microphone), NotDetermined);
        assert!(permissions.requests().is_empty());
    }
}
