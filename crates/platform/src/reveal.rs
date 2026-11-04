//! Showing a folder in the system's file manager (the tray's "Reveal log").

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::process::Command;

/// The program that opens a folder in the file manager, and its arguments:
/// `open` (Finder) on macOS, `explorer` on Windows, `xdg-open` elsewhere.
fn command_line(dir: &Path) -> (&'static str, Vec<OsString>) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    (program, vec![dir.as_os_str().to_owned()])
}

/// The environment variable that turns off host side effects (F1): the real
/// launcher and the real keychain refuse to run. The gate and nextest set it,
/// so a test that forgets the fake can't open a browser or Finder, or ask for
/// the owner's Keychain items.
pub const NO_LAUNCH_VAR: &str = "DICTATION_NO_LAUNCH";

/// Whether host side effects are off: in this crate's own tests, in any
/// build with feature `testing` (which only test targets of other crates
/// enable, never the shipped app or `tauri dev`, so every crate's tests are
/// covered with plain `cargo test`, nextest or the gate), or when
/// [`NO_LAUNCH_VAR`] is set.
#[must_use]
pub fn launching_disabled() -> bool {
    cfg!(test) || cfg!(feature = "testing") || std::env::var_os(NO_LAUNCH_VAR).is_some()
}

fn launch_allowed() -> io::Result<()> {
    if launching_disabled() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "launching programs is disabled in tests (DICTATION_NO_LAUNCH)",
        ));
    }
    Ok(())
}

/// The real [`Opener`](dictation_ports::Opener): `open`, `explorer` or
/// `xdg-open`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemOpener;

impl dictation_ports::Opener for SystemOpener {
    fn open_url(&self, url: &str) -> Result<(), String> {
        open_url(url).map_err(|error| error.to_string())
    }

    fn reveal_folder(&self, dir: &Path) -> Result<(), String> {
        reveal_folder(dir).map_err(|error| error.to_string())
    }
}

/// Opens `dir` in the file manager.
///
/// # Errors
///
/// If the file manager's launcher can't run, or launching is disabled
/// ([`NO_LAUNCH_VAR`]). Its exit code is not checked: Windows' `explorer`
/// exits with 1 even when it works.
pub fn reveal_folder(dir: &Path) -> io::Result<()> {
    launch_allowed()?;
    let (program, args) = command_line(dir);
    // Waits for the launcher (it returns at once), so no zombie is left.
    Command::new(program).args(args).status().map(|_| ())
}

/// Opens `url` in the default browser.
///
/// # Errors
///
/// If the launcher can't run, or `url` isn't an `https` address (nothing
/// else is ever opened this way).
pub fn open_url(url: &str) -> io::Result<()> {
    if !url.starts_with("https://") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only https addresses are opened",
        ));
    }
    launch_allowed()?;
    let (program, _) = command_line(Path::new(""));
    Command::new(program).arg(url).status().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_addresses_are_opened() {
        for url in ["file:///etc/passwd", "javascript:alert(1)", "/tmp/x", ""] {
            let error = open_url(url).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{url}");
        }
    }

    #[test]
    fn nothing_is_launched_under_test() {
        let url = open_url("https://example.com").unwrap_err();
        assert_eq!(url.kind(), io::ErrorKind::PermissionDenied);
        let dir = reveal_folder(Path::new("/tmp")).unwrap_err();
        assert_eq!(dir.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn the_folder_is_the_only_argument() {
        let (program, args) = command_line(Path::new("/tmp/some logs"));
        assert!(["open", "explorer", "xdg-open"].contains(&program));
        assert_eq!(args, [OsString::from("/tmp/some logs")]);
    }
}
