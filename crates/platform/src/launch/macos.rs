//! [`LaunchAtLogin`] on macOS with `SMAppService.mainApp` (macOS 13+): the
//! app shows up in System Settings, General, Login Items, where the user can
//! also switch it off. It works only from an app bundle.
//!
//! The native calls sit behind [`LoginItemService`], a three-call facade, so
//! the register/unregister/refusal logic in [`MacLaunchAtLogin`] is tested
//! with a scripted fake and never touches the host's login items.

use std::fmt;
use std::sync::Arc;

use dictation_ports::{LaunchAtLogin, LaunchAtLoginError, LaunchAtLoginState};
use objc2_foundation::NSError;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// Why the OS refused a register or unregister: its `NSError`, as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServiceError {
    /// `localizedDescription`.
    pub(crate) description: String,
    /// `code`.
    pub(crate) code: isize,
}

impl From<&NSError> for ServiceError {
    fn from(error: &NSError) -> Self {
        Self {
            description: error.localizedDescription().to_string(),
            code: error.code(),
        }
    }
}

/// The three `SMAppService` calls the adapter makes.
pub(crate) trait LoginItemService: Send + Sync {
    fn status(&self) -> SMAppServiceStatus;
    fn register(&self) -> Result<(), ServiceError>;
    fn unregister(&self) -> Result<(), ServiceError>;
}

/// The real `SMAppService.mainApp`.
struct MainAppService;

impl LoginItemService for MainAppService {
    fn status(&self) -> SMAppServiceStatus {
        // SAFETY: `mainAppService` and `status` take no arguments and have
        // no preconditions; SMAppService may be used from any thread.
        unsafe { SMAppService::mainAppService().status() }
    }

    fn register(&self) -> Result<(), ServiceError> {
        // SAFETY: as in `status`; the call returns an NSError instead of
        // throwing.
        unsafe { SMAppService::mainAppService().registerAndReturnError() }
            .map_err(|error| ServiceError::from(&*error))
    }

    fn unregister(&self) -> Result<(), ServiceError> {
        // SAFETY: as in `register`.
        unsafe { SMAppService::mainAppService().unregisterAndReturnError() }
            .map_err(|error| ServiceError::from(&*error))
    }
}

/// The main app's login item.
#[derive(Clone)]
pub struct MacLaunchAtLogin {
    service: Arc<dyn LoginItemService>,
}

impl Default for MacLaunchAtLogin {
    fn default() -> Self {
        Self {
            service: Arc::new(MainAppService),
        }
    }
}

impl fmt::Debug for MacLaunchAtLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacLaunchAtLogin").finish_non_exhaustive()
    }
}

impl MacLaunchAtLogin {
    /// The adapter over `service` instead of the real one (tests).
    #[cfg(test)]
    pub(crate) fn with_service(service: Arc<dyn LoginItemService>) -> Self {
        Self { service }
    }
}

/// What a status means to us. `NotFound` (macOS can't find the app's
/// service, say outside a bundle) reads as off; `set_enabled` then reports
/// the OS's reason.
fn state_of(status: SMAppServiceStatus) -> LaunchAtLoginState {
    if status == SMAppServiceStatus::Enabled {
        LaunchAtLoginState::Enabled
    } else if status == SMAppServiceStatus::RequiresApproval {
        LaunchAtLoginState::RequiresApproval
    } else {
        LaunchAtLoginState::Disabled
    }
}

fn os_error(action: &str, error: &ServiceError) -> LaunchAtLoginError {
    LaunchAtLoginError::Os(format!(
        "macOS could not {action} the login item: {} (code {})",
        error.description, error.code
    ))
}

impl LaunchAtLogin for MacLaunchAtLogin {
    fn state(&self) -> Result<LaunchAtLoginState, LaunchAtLoginError> {
        Ok(state_of(self.service.status()))
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), LaunchAtLoginError> {
        let result = if enabled {
            self.service.register()
        } else {
            self.service.unregister()
        };
        match result {
            Ok(()) => Ok(()),
            // Registering twice, or unregistering what isn't registered, is
            // the state asked for already: check before calling it a failure.
            Err(error) => match self.state() {
                Ok(state) if state.is_enabled() == enabled => Ok(()),
                _ => Err(os_error(
                    if enabled { "register" } else { "remove" },
                    &error,
                )),
            },
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::VecDeque;
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use super::*;

    /// A call the adapter made, in order.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Call {
        Status,
        Register,
        Unregister,
    }

    #[derive(Default)]
    struct Script {
        calls: Vec<Call>,
        statuses: VecDeque<SMAppServiceStatus>,
        outcomes: VecDeque<Result<(), ServiceError>>,
    }

    /// A scripted `SMAppService`: each `status` and each register/unregister
    /// takes the next scripted answer, and every call is logged. Running out
    /// of answers panics, so an unexpected extra call can't pass.
    #[derive(Default)]
    pub(crate) struct FakeService {
        script: Mutex<Script>,
    }

    impl FakeService {
        pub(crate) fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        pub(crate) fn then_status(&self, status: SMAppServiceStatus) -> &Self {
            self.lock().statuses.push_back(status);
            self
        }

        pub(crate) fn then_ok(&self) -> &Self {
            self.lock().outcomes.push_back(Ok(()));
            self
        }

        pub(crate) fn then_refuse(&self, description: &str, code: isize) -> &Self {
            self.lock().outcomes.push_back(Err(ServiceError {
                description: description.to_owned(),
                code,
            }));
            self
        }

        /// The calls so far; clears the log.
        pub(crate) fn take_calls(&self) -> Vec<Call> {
            std::mem::take(&mut self.lock().calls)
        }

        fn lock(&self) -> MutexGuard<'_, Script> {
            self.script.lock().unwrap_or_else(PoisonError::into_inner)
        }

        fn outcome(&self, call: Call) -> Result<(), ServiceError> {
            let mut script = self.lock();
            script.calls.push(call);
            script
                .outcomes
                .pop_front()
                .unwrap_or_else(|| panic!("unscripted {call:?}"))
        }
    }

    impl LoginItemService for FakeService {
        fn status(&self) -> SMAppServiceStatus {
            let mut script = self.lock();
            script.calls.push(Call::Status);
            script
                .statuses
                .pop_front()
                .unwrap_or_else(|| panic!("unscripted status"))
        }

        fn register(&self) -> Result<(), ServiceError> {
            self.outcome(Call::Register)
        }

        fn unregister(&self) -> Result<(), ServiceError> {
            self.outcome(Call::Unregister)
        }
    }

    fn adapter(service: &Arc<FakeService>) -> MacLaunchAtLogin {
        MacLaunchAtLogin::with_service(service.clone())
    }

    #[test]
    fn statuses_map_to_states() {
        assert_eq!(
            state_of(SMAppServiceStatus::Enabled),
            LaunchAtLoginState::Enabled
        );
        assert_eq!(
            state_of(SMAppServiceStatus::RequiresApproval),
            LaunchAtLoginState::RequiresApproval
        );
        assert_eq!(
            state_of(SMAppServiceStatus::NotRegistered),
            LaunchAtLoginState::Disabled
        );
        assert_eq!(
            state_of(SMAppServiceStatus::NotFound),
            LaunchAtLoginState::Disabled
        );
    }

    #[test]
    fn state_reads_the_service_status() {
        let service = FakeService::new();
        service.then_status(SMAppServiceStatus::Enabled);
        let login = adapter(&service);
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Enabled));
        assert_eq!(service.take_calls(), [Call::Status]);
    }

    #[test]
    fn register_and_unregister_use_requested_operation() {
        let service = FakeService::new();
        service.then_ok().then_ok();
        let login = adapter(&service);

        assert_eq!(login.set_enabled(true), Ok(()));
        // A native success is trusted: no re-query.
        assert_eq!(service.take_calls(), [Call::Register]);

        assert_eq!(login.set_enabled(false), Ok(()));
        assert_eq!(service.take_calls(), [Call::Unregister]);
    }

    #[test]
    fn already_in_requested_state_is_idempotent() {
        let service = FakeService::new();
        // Registering what is registered already: the OS errors, the status
        // says it's on, which is what was asked for.
        service
            .then_refuse("already registered", 1)
            .then_status(SMAppServiceStatus::Enabled);
        let login = adapter(&service);
        assert_eq!(login.set_enabled(true), Ok(()));
        assert_eq!(service.take_calls(), [Call::Register, Call::Status]);

        // Unregistering what isn't registered.
        service
            .then_refuse("not registered", 2)
            .then_status(SMAppServiceStatus::NotRegistered);
        assert_eq!(login.set_enabled(false), Ok(()));
        assert_eq!(service.take_calls(), [Call::Unregister, Call::Status]);
    }

    #[test]
    fn refusal_with_mismatching_status_is_reported() {
        let service = FakeService::new();
        // Outside an app bundle, say.
        service
            .then_refuse("Operation not permitted", 1)
            .then_status(SMAppServiceStatus::NotFound);
        let login = adapter(&service);
        assert_eq!(
            login.set_enabled(true),
            Err(LaunchAtLoginError::Os(
                "macOS could not register the login item: Operation not permitted (code 1)"
                    .to_owned()
            ))
        );
        assert_eq!(service.take_calls(), [Call::Register, Call::Status]);

        service
            .then_refuse("Invalid argument", 22)
            .then_status(SMAppServiceStatus::Enabled);
        assert_eq!(
            login.set_enabled(false),
            Err(LaunchAtLoginError::Os(
                "macOS could not remove the login item: Invalid argument (code 22)".to_owned()
            ))
        );
        assert_eq!(service.take_calls(), [Call::Unregister, Call::Status]);

        // Nothing sticks: the next attempt asks the OS again and succeeds.
        service.then_ok();
        assert_eq!(login.set_enabled(true), Ok(()));
        assert_eq!(service.take_calls(), [Call::Register]);
    }

    #[test]
    fn requires_approval_is_not_enabled() {
        let service = FakeService::new();
        // The item is registered but the user switched it off in System
        // Settings, so it won't start at login: a failed register is not a
        // success.
        service
            .then_refuse("Operation not permitted", 1)
            .then_status(SMAppServiceStatus::RequiresApproval);
        let login = adapter(&service);
        assert_eq!(
            login.set_enabled(true),
            Err(LaunchAtLoginError::Os(
                "macOS could not register the login item: Operation not permitted (code 1)"
                    .to_owned()
            ))
        );
        assert_eq!(service.take_calls(), [Call::Register, Call::Status]);

        service.then_status(SMAppServiceStatus::RequiresApproval);
        let state = login.state();
        assert_eq!(state, Ok(LaunchAtLoginState::RequiresApproval));
        assert_eq!(state.map(LaunchAtLoginState::is_enabled), Ok(false));
    }
}
