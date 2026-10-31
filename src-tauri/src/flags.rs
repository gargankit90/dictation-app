//! A launch's command-line flags, from this process or forwarded by the
//! single-instance plugin from a second launch.
//!
//! `--press` and `--release` are test hooks and exist only in debug builds
//! ([`TEST_FLAGS`]): the single-instance socket accepts any local process,
//! so in a release build a forwarded `--press` would let any process start
//! dictation (and later paste) with this app's permissions. A release build
//! treats them like any other unknown flag: logged and ignored.

use dictation_pipeline::test_hooks::{PRESS_FLAG, RELEASE_FLAG};
use dictation_ports::HotkeyEvent;

/// Whether this build acts on `--press` and `--release`.
pub const TEST_FLAGS: bool = cfg!(debug_assertions);

/// What a launch's arguments ask for.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LaunchFlags {
    /// Synthetic hotkey events to inject, in order (debug builds only).
    pub events: Vec<HotkeyEvent>,
    /// Arguments this build doesn't act on, for a warning.
    pub ignored: Vec<String>,
}

/// Sorts `argv` (program path first) into what to act on and what to
/// ignore. macOS's `-psn_…` process serial number is dropped silently.
#[must_use]
pub fn parse(argv: &[String], test_flags: bool) -> LaunchFlags {
    let mut flags = LaunchFlags::default();
    for arg in argv.iter().skip(1) {
        match arg.as_str() {
            PRESS_FLAG if test_flags => flags.events.push(HotkeyEvent::Pressed),
            RELEASE_FLAG if test_flags => flags.events.push(HotkeyEvent::Released),
            psn if psn.starts_with("-psn_") => {}
            other => flags.ignored.push(other.to_owned()),
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use HotkeyEvent::{Pressed, Released};

    fn argv(args: &[&str]) -> Vec<String> {
        std::iter::once("dictation-app")
            .chain(args.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn debug_builds_act_on_press_and_release() {
        let flags = parse(&argv(&["--press", "--release", "--press"]), true);
        assert_eq!(flags.events, [Pressed, Released, Pressed]);
        assert!(flags.ignored.is_empty());
    }

    #[test]
    fn release_builds_ignore_press_and_release() {
        let flags = parse(&argv(&["--press", "--release"]), false);
        assert!(flags.events.is_empty());
        assert_eq!(flags.ignored, ["--press", "--release"]);
    }

    #[test]
    fn unknown_flags_are_ignored_in_both_and_psn_silently() {
        for test_flags in [true, false] {
            let flags = parse(&argv(&["-psn_0_12345", "--verbose", "--PRESS"]), test_flags);
            assert!(flags.events.is_empty(), "test_flags={test_flags}");
            assert_eq!(flags.ignored, ["--verbose", "--PRESS"]);
        }
    }

    #[test]
    fn the_program_path_is_never_a_flag() {
        assert_eq!(parse(&argv(&[]), true), LaunchFlags::default());
        // Even a program path that looks like a flag.
        assert_eq!(parse(&["--press".to_owned()], true), LaunchFlags::default());
    }

    #[test]
    fn test_flags_follow_the_build() {
        assert_eq!(TEST_FLAGS, cfg!(debug_assertions));
    }
}
