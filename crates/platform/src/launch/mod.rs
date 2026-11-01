//! Launch at login behind [`LaunchAtLogin`]. [`SystemLaunchAtLogin`] picks
//! this OS's adapter.

#[cfg(target_os = "macos")]
pub mod macos;

use dictation_ports::{LaunchAtLogin, LaunchAtLoginError, LaunchAtLoginState};

/// This OS's login item: [`MacLaunchAtLogin`](macos::MacLaunchAtLogin) on
/// macOS. Elsewhere there is no adapter yet and every call says so.
#[derive(Debug, Default, Clone)]
pub struct SystemLaunchAtLogin {
    #[cfg(target_os = "macos")]
    inner: macos::MacLaunchAtLogin,
}

impl SystemLaunchAtLogin {
    /// This OS's login item.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl LaunchAtLogin for SystemLaunchAtLogin {
    fn state(&self) -> Result<LaunchAtLoginState, LaunchAtLoginError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.state()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err(LaunchAtLoginError::Unsupported)
        }
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), LaunchAtLoginError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.set_enabled(enabled)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = enabled;
            Err(LaunchAtLoginError::Unsupported)
        }
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn without_an_adapter_every_call_says_so() {
        let login = SystemLaunchAtLogin::new();
        assert_eq!(login.state(), Err(LaunchAtLoginError::Unsupported));
        assert_eq!(
            login.set_enabled(true),
            Err(LaunchAtLoginError::Unsupported)
        );
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use objc2_service_management::SMAppServiceStatus;

    use super::macos::tests::{Call, FakeService};
    use super::*;

    #[test]
    fn delegates_to_the_mac_adapter() {
        let service = FakeService::new();
        let login = SystemLaunchAtLogin {
            inner: macos::MacLaunchAtLogin::with_service(service.clone()),
        };

        service.then_status(SMAppServiceStatus::RequiresApproval);
        assert_eq!(login.state(), Ok(LaunchAtLoginState::RequiresApproval));
        assert_eq!(service.take_calls(), [Call::Status]);

        service.then_ok();
        assert_eq!(login.set_enabled(true), Ok(()));
        assert_eq!(service.take_calls(), [Call::Register]);

        service
            .then_refuse("Operation not permitted", 1)
            .then_status(SMAppServiceStatus::Enabled);
        assert_eq!(
            login.set_enabled(false),
            Err(LaunchAtLoginError::Os(
                "macOS could not remove the login item: Operation not permitted (code 1)"
                    .to_owned()
            ))
        );
        assert_eq!(service.take_calls(), [Call::Unregister, Call::Status]);
    }
}
