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

/// Opens `dir` in the file manager.
///
/// # Errors
///
/// If the file manager's launcher can't run. Its exit code is not checked:
/// Windows' `explorer` exits with 1 even when it works.
pub fn reveal_folder(dir: &Path) -> io::Result<()> {
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
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
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
    fn the_folder_is_the_only_argument() {
        let (program, args) = command_line(Path::new("/tmp/some logs"));
        assert!(["open", "explorer", "xdg-open"].contains(&program));
        assert_eq!(args, [OsString::from("/tmp/some logs")]);
    }
}
