//! [`LaunchAtLogin`] on macOS with `SMAppService.mainApp` (macOS 13+): the
//! app shows up in System Settings, General, Login Items, where the user can
//! also switch it off. It works only from an app bundle.

use dictation_ports::{LaunchAtLogin, LaunchAtLoginError, LaunchAtLoginState};
use objc2_foundation::NSError;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// The main app's login item.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacLaunchAtLogin;

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

fn os_error(action: &str, error: &NSError) -> LaunchAtLoginError {
    LaunchAtLoginError::Os(format!(
        "macOS could not {action} the login item: {} (code {})",
        error.localizedDescription(),
        error.code()
    ))
}

impl LaunchAtLogin for MacLaunchAtLogin {
    fn state(&self) -> Result<LaunchAtLoginState, LaunchAtLoginError> {
        // SAFETY: `mainAppService` and `status` take no arguments and have
        // no preconditions; SMAppService may be used from any thread.
        let status = unsafe { SMAppService::mainAppService().status() };
        Ok(state_of(status))
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), LaunchAtLoginError> {
        // SAFETY: as in `state`; the register and unregister calls return
        // an NSError instead of throwing.
        let result = unsafe {
            let service = SMAppService::mainAppService();
            if enabled {
                service.registerAndReturnError()
            } else {
                service.unregisterAndReturnError()
            }
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
mod tests {
    use super::*;

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
}
