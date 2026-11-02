//! Opening the system's privacy settings at a permission's page, for the
//! onboarding steps (M5.2): after a refusal only the user can grant a
//! permission, there.

use std::io;
use std::process::Command;

use dictation_ports::Permission;

/// The `x-apple.systempreferences` URL of `permission`'s page in System
/// Settings › Privacy & Security (macOS 13+ still opens these).
#[must_use]
pub fn privacy_url(permission: Permission) -> &'static str {
    match permission {
        Permission::Microphone => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        Permission::Accessibility => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
    }
}

/// Opens System Settings at `permission`'s privacy page (macOS). Elsewhere
/// there is no such page yet and this does nothing.
///
/// # Errors
///
/// If `open` can't run or says it failed.
pub fn open_privacy_settings(permission: Permission) -> io::Result<()> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    // Waits for `open` (it returns at once), so no zombie is left.
    let status = Command::new("/usr/bin/open")
        .arg(privacy_url(permission))
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "open exited with {status} for the {permission:?} settings"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_permission_has_its_own_privacy_page() {
        assert!(privacy_url(Permission::Microphone).ends_with("?Privacy_Microphone"));
        assert!(privacy_url(Permission::Accessibility).ends_with("?Privacy_Accessibility"));
        for permission in [Permission::Microphone, Permission::Accessibility] {
            assert!(privacy_url(permission).starts_with("x-apple.systempreferences:"));
        }
    }
}
