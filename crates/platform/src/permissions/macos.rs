//! [`Permissions`] on macOS. Accessibility only for now; Microphone comes
//! with the audio adapter.

use std::ffi::c_void;

use dictation_ports::{Permission, PermissionError, PermissionStatus, Permissions};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: *const c_void;
}

/// Opaque `CFDictionaryKeyCallBacks` / `CFDictionaryValueCallBacks`; only
/// their addresses are passed.
#[repr(C)]
struct CallBacks {
    _private: [u8; 0],
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: *const c_void;
    static kCFTypeDictionaryKeyCallBacks: CallBacks;
    static kCFTypeDictionaryValueCallBacks: CallBacks;
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        count: isize,
        key_callbacks: *const CallBacks,
        value_callbacks: *const CallBacks,
    ) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

/// Whether this process may observe and post key events (Accessibility).
#[must_use]
pub fn is_accessibility_trusted() -> bool {
    // SAFETY: AXIsProcessTrusted takes no arguments, has no preconditions
    // and is safe to call from any thread.
    unsafe { AXIsProcessTrusted() }
}

/// Shows the system Accessibility prompt if this process isn't trusted yet
/// (macOS shows it at most once per app; after that it does nothing) and
/// returns whether it is trusted now.
fn prompt_for_accessibility() -> Result<bool, PermissionError> {
    // SAFETY: the two statics are immutable CF constants provided by the
    // frameworks we link, valid for the life of the process. The key and
    // value arrays hold one valid CF object each and outlive the call;
    // CFDictionaryCreate retains them through the kCFType callbacks.
    let options = unsafe {
        let keys = [kAXTrustedCheckOptionPrompt];
        let values = [kCFBooleanTrue];
        CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            &raw const kCFTypeDictionaryKeyCallBacks,
            &raw const kCFTypeDictionaryValueCallBacks,
        )
    };
    if options.is_null() {
        return Err(PermissionError::RequestFailed(
            "could not build the prompt options".into(),
        ));
    }
    // SAFETY: `options` is the non-null CFDictionary created above, which we
    // own (Create rule) and release exactly once after the call.
    let trusted = unsafe {
        let trusted = AXIsProcessTrustedWithOptions(options);
        CFRelease(options);
        trusted
    };
    Ok(trusted)
}

/// macOS permissions.
///
/// macOS can't say whether Accessibility was refused or never asked for, so
/// an untrusted app reports [`PermissionStatus::NotDetermined`]: callers
/// treat anything but `Granted` as missing, and
/// [`request`](Permissions::request) shows the prompt (once) or does nothing.
/// The user grants it in System Settings › Privacy & Security ›
/// Accessibility; it takes effect without restarting, though a running
/// hotkey source has to be started again.
#[derive(Debug, Clone, Copy, Default)]
pub struct MacPermissions;

impl MacPermissions {
    /// The macOS permission checks.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Permissions for MacPermissions {
    fn check(&self, permission: Permission) -> PermissionStatus {
        match permission {
            Permission::Accessibility if is_accessibility_trusted() => PermissionStatus::Granted,
            // Microphone isn't checked until the audio adapter lands.
            Permission::Accessibility | Permission::Microphone => PermissionStatus::NotDetermined,
        }
    }

    fn request(&self, permission: Permission) -> Result<(), PermissionError> {
        match permission {
            Permission::Accessibility => prompt_for_accessibility().map(|_| ()),
            Permission::Microphone => Err(PermissionError::RequestFailed(
                "microphone permission is not supported yet".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Whether the test runner is trusted depends on the machine, so this
    // only checks that `check` agrees with the raw call.
    #[test]
    fn accessibility_check_matches_ax_is_process_trusted() {
        let expected = if is_accessibility_trusted() {
            PermissionStatus::Granted
        } else {
            PermissionStatus::NotDetermined
        };
        assert_eq!(
            MacPermissions::new().check(Permission::Accessibility),
            expected
        );
    }

    #[test]
    fn microphone_is_not_supported_yet() {
        let permissions = MacPermissions::new();
        assert_eq!(
            permissions.check(Permission::Microphone),
            PermissionStatus::NotDetermined
        );
        assert!(permissions.request(Permission::Microphone).is_err());
    }
}
