//! [`Permissions`] on macOS: Accessibility and Microphone.

use std::ffi::c_void;

use block2::RcBlock;
use dictation_ports::{Permission, PermissionError, PermissionStatus, Permissions};
use objc2::runtime::Bool;
use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};

// SAFETY: these declarations match the C signatures in ApplicationServices
// (AXUIElement.h); each call site's `unsafe` block upholds its contract. Both
// functions return `Boolean`, an `unsigned char` (MacTypes.h), so `u8`, not
// `bool`: any non-zero value is true (#46), as for
// `IsSecureEventInputEnabled` in `secure_input.rs`.
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
    static kAXTrustedCheckOptionPrompt: *const c_void;
}

/// Opaque `CFDictionaryKeyCallBacks` / `CFDictionaryValueCallBacks`; only
/// their addresses are passed.
#[repr(C)]
struct CallBacks {
    _private: [u8; 0],
}

// SAFETY: these declarations match the C signatures in CoreFoundation
// (CFDictionary.h, CFNumber.h, CFBase.h); each call site's `unsafe` block
// upholds its contract.
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
    unsafe { AXIsProcessTrusted() != 0 }
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
        let trusted = AXIsProcessTrustedWithOptions(options) != 0;
        CFRelease(options);
        trusted
    };
    Ok(trusted)
}

/// The microphone (TCC) status, without prompting.
fn microphone_status() -> PermissionStatus {
    match handy_recorder::permission_status() {
        handy_recorder::Permission::Granted => PermissionStatus::Granted,
        handy_recorder::Permission::Denied => PermissionStatus::Denied,
        // NotDetermined, or no answer: `request` can still prompt.
        _ => PermissionStatus::NotDetermined,
    }
}

/// Shows the system microphone prompt if the user hasn't answered yet (it
/// does nothing after that) and returns without waiting for the answer.
///
/// The app's Info.plist must have `NSMicrophoneUsageDescription`, or macOS
/// kills the process here.
fn request_microphone() -> Result<(), PermissionError> {
    // SAFETY: AVMediaTypeAudio is an immutable framework constant, valid for
    // the life of the process.
    let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
        return Err(PermissionError::RequestFailed(
            "AVMediaTypeAudio is missing".into(),
        ));
    };
    // The answer arrives on an arbitrary queue; `check` reads it later.
    let handler = RcBlock::new(|granted: Bool| {
        tracing::info!(granted = granted.as_bool(), "microphone prompt answered");
    });
    // SAFETY: `audio` is AVMediaTypeAudio, one of the two media types the
    // method accepts (others throw). The block is retained (copied) by
    // AVFoundation for as long as it needs it, so ours may drop after the
    // call; it captures nothing and is safe to call from any queue.
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &handler) };
    Ok(())
}

/// macOS permissions.
///
/// Microphone: [`check`](Permissions::check) reads the TCC status and
/// [`request`](Permissions::request) shows the system prompt the first
/// time; after a refusal only System Settings › Privacy & Security ›
/// Microphone can grant it.
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
            Permission::Accessibility => PermissionStatus::NotDetermined,
            Permission::Microphone => microphone_status(),
        }
    }

    fn request(&self, permission: Permission) -> Result<(), PermissionError> {
        match permission {
            Permission::Accessibility => prompt_for_accessibility().map(|_| ()),
            Permission::Microphone => request_microphone(),
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

    // Depends on the machine too; never `request` here: a test binary has
    // no usage description, so macOS would kill it.
    #[test]
    fn microphone_check_matches_the_tcc_status() {
        let expected = match handy_recorder::permission_status() {
            handy_recorder::Permission::Granted => PermissionStatus::Granted,
            handy_recorder::Permission::Denied => PermissionStatus::Denied,
            _ => PermissionStatus::NotDetermined,
        };
        assert_eq!(
            MacPermissions::new().check(Permission::Microphone),
            expected
        );
    }
}
