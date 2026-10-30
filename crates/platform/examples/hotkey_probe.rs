//! Manual check for M1.4: prints hotkey events, a missing Accessibility
//! permission and the Secure Input warning to stdout.
//!
//! ```sh
//! cargo run -p dictation-platform --example hotkey_probe            # RightAlt
//! cargo run -p dictation-platform --example hotkey_probe -- Ctrl+Shift+Space
//! ```
//!
//! Accessibility is granted to the app the probe runs in (Terminal, iTerm,
//! …). Only Pressed, Released and CancelPressed are printed, never which
//! keys were typed. Stop it with Ctrl+C.

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    use std::process::ExitCode;
    use std::sync::mpsc;
    use std::time::Duration;

    use dictation_core::error::DictationError;
    use dictation_core::hotkey_spec::HotkeySpec;
    use dictation_platform::hotkey::MacHotkey;
    use dictation_platform::permissions::macos::MacPermissions;
    use dictation_platform::secure_input::{SecureInputEvent, SecureInputWatcher};
    use dictation_ports::{HotkeyError, HotkeySource, Permission, PermissionStatus, Permissions};

    let spec = match std::env::args().nth(1).map(|s| s.parse::<HotkeySpec>()) {
        None => HotkeySpec::default(),
        Some(Ok(spec)) => spec,
        Some(Err(e)) => {
            println!("invalid hotkey: {e}");
            return ExitCode::FAILURE;
        }
    };
    let missing = DictationError::PermissionMissing(Permission::Accessibility);

    let permissions = MacPermissions::new();
    if permissions.check(Permission::Accessibility) != PermissionStatus::Granted {
        println!("PermissionMissing: {missing}");
        println!(
            "Grant it to this terminal in System Settings > Privacy & Security > Accessibility; waiting..."
        );
        if let Err(e) = permissions.request(Permission::Accessibility) {
            println!("could not show the prompt: {e}");
        }
        while permissions.check(Permission::Accessibility) != PermissionStatus::Granted {
            std::thread::sleep(Duration::from_secs(1));
        }
        println!("Accessibility granted");
    }

    let (tx, rx) = mpsc::channel();
    let _secure_input = match SecureInputWatcher::start_macos(|event| match event {
        SecureInputEvent::On => println!(
            "WARNING: Secure Input is on (a password field has focus); only modifier-only hotkeys work"
        ),
        SecureInputEvent::Off => println!("Secure Input is off"),
    }) {
        Ok(watcher) => watcher,
        Err(e) => {
            println!("could not watch Secure Input: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut hotkey = MacHotkey::new(spec);
    match hotkey.start(Box::new(move |event| {
        let _ = tx.send(event);
    })) {
        Ok(()) => {}
        Err(HotkeyError::PermissionDenied) => {
            println!("PermissionMissing: {missing}");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            println!("hotkey failed: {e}");
            return ExitCode::FAILURE;
        }
    }
    println!("listening for {spec}; press Ctrl+C to stop");
    for event in rx {
        println!("{event:?}");
    }
    ExitCode::SUCCESS
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("hotkey_probe runs on macOS only");
}
