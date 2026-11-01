//! [`LaunchAtLogin`] on macOS with `SMAppService.mainApp` (macOS 13+): the
//! app shows up in System Settings, General, Login Items, where the user can
//! also switch it off. It works only from an app bundle.

use dictation_ports::{LaunchAtLogin, LaunchAtLoginError, LaunchAtLoginState};
use objc2_foundation::{NSError, NSOperatingSystemVersion, NSProcessInfo};
use objc2_service_management::{SMAppService, SMAppServiceStatus};

/// The main app's login item.
///
/// `SMAppService` arrived in macOS 13, but the app runs on 10.13 and later.
/// Touching the class on an older system makes `objc2` panic, so every
/// operation checks first and says [`LaunchAtLoginError::Unsupported`].
#[derive(Debug, Clone, Copy)]
pub struct MacLaunchAtLogin {
    supported: bool,
}

impl MacLaunchAtLogin {
    /// The login item of this Mac; unsupported before macOS 13.
    #[must_use]
    pub fn new() -> Self {
        Self::with_support(is_macos_13_or_later())
    }

    /// With the OS support decided by the caller, to test the unsupported
    /// path on any Mac.
    #[must_use]
    pub fn with_support(supported: bool) -> Self {
        Self { supported }
    }

    fn check(self) -> Result<(), LaunchAtLoginError> {
        if self.supported {
            Ok(())
        } else {
            Err(LaunchAtLoginError::Unsupported)
        }
    }
}

impl Default for MacLaunchAtLogin {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether the running macOS has `SMAppService` (13 and later), without
/// touching the class.
fn is_macos_13_or_later() -> bool {
    let version = NSOperatingSystemVersion {
        majorVersion: 13,
        minorVersion: 0,
        patchVersion: 0,
    };
    NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(version)
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

fn os_error(action: &str, error: &NSError) -> LaunchAtLoginError {
    LaunchAtLoginError::Os(format!(
        "macOS could not {action} the login item: {} (code {})",
        error.localizedDescription(),
        error.code()
    ))
}

impl LaunchAtLogin for MacLaunchAtLogin {
    fn state(&self) -> Result<LaunchAtLoginState, LaunchAtLoginError> {
        self.check()?;
        // SAFETY: `mainAppService` and `status` take no arguments and have
        // no preconditions; SMAppService may be used from any thread.
        let status = unsafe { SMAppService::mainAppService().status() };
        Ok(state_of(status))
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), LaunchAtLoginError> {
        self.check()?;
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

    #[test]
    fn before_macos_13_both_operations_say_unsupported_without_the_class() {
        // Reaching SMAppService on a system without it would panic, so this
        // passing proves the check comes first.
        let login = MacLaunchAtLogin::with_support(false);
        assert_eq!(login.state(), Err(LaunchAtLoginError::Unsupported));
        assert_eq!(
            login.set_enabled(true),
            Err(LaunchAtLoginError::Unsupported)
        );
        assert_eq!(
            login.set_enabled(false),
            Err(LaunchAtLoginError::Unsupported)
        );
    }

    #[test]
    fn this_mac_is_macos_13_or_later_by_the_version_check() {
        // CI and dev machines are on 13+; this exercises the real call.
        assert!(is_macos_13_or_later());
        assert!(MacLaunchAtLogin::new().check().is_ok());
    }
}
