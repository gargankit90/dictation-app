//! OS permissions behind [`Permissions`](dictation_ports::Permissions).
//! [`SystemPermissions`] picks this OS's checks.

#[cfg(target_os = "macos")]
pub mod macos;

use dictation_ports::{Permission, PermissionError, PermissionStatus, Permissions};

/// This OS's permission checks: [`MacPermissions`](macos::MacPermissions) on
/// macOS. Elsewhere there's nothing to check yet, so everything reads as
/// granted and the adapters report what they can't do.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemPermissions {
    #[cfg(target_os = "macos")]
    inner: macos::MacPermissions,
}

impl SystemPermissions {
    /// This OS's permission checks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Permissions for SystemPermissions {
    fn check(&self, permission: Permission) -> PermissionStatus {
        #[cfg(target_os = "macos")]
        {
            self.inner.check(permission)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = permission;
            PermissionStatus::Granted
        }
    }

    fn request(&self, permission: Permission) -> Result<(), PermissionError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.request(permission)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = permission;
            Ok(())
        }
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn everything_reads_as_granted_without_checks() {
        let permissions = SystemPermissions::new();
        for permission in [Permission::Microphone, Permission::Accessibility] {
            assert_eq!(permissions.check(permission), PermissionStatus::Granted);
            assert_eq!(permissions.request(permission), Ok(()));
        }
    }
}
