//! A launch's command-line flags, from this process or forwarded by the
//! single-instance plugin from a second launch.
//!
//! `--press` and `--release` are test hooks and exist only in debug builds
//! ([`TEST_FLAGS`]): the single-instance socket accepts any local process,
//! so in a release build a forwarded `--press` would let any process start
//! dictation (and later paste) with this app's permissions. A release build
//! treats them like any other unknown flag: logged and ignored.
//!
//! `--transcriber <name>` picks the speech-to-text engine. It's a normal flag
//! in every build: it only affects the process that starts with it (a
//! forwarded second launch can't switch the running app's engine), and the
//! only engine so far, `mock`, needs no network, key or permission.

use dictation_pipeline::test_hooks::{PRESS_FLAG, RELEASE_FLAG};
use dictation_ports::HotkeyEvent;

/// Whether this build acts on `--press` and `--release`.
pub const TEST_FLAGS: bool = cfg!(debug_assertions);

/// `--transcriber <name>` or `--transcriber=<name>`.
pub const TRANSCRIBER_FLAG: &str = "--transcriber";

/// The speech-to-text engine to run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TranscriberChoice {
    /// `transcribe::mock::MockTranscriber`: fixed text, no network. The
    /// default until a real engine lands (M3.2).
    #[default]
    Mock,
}

impl TranscriberChoice {
    /// The engine called `name` on the command line.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "mock" => Some(Self::Mock),
            _ => None,
        }
    }

    /// The name `--transcriber` takes for this engine.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Mock => "mock",
        }
    }
}

/// What a launch's arguments ask for.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LaunchFlags {
    /// Synthetic hotkey events to inject, in order (debug builds only).
    pub events: Vec<HotkeyEvent>,
    /// The engine `--transcriber` asked for; the last one wins.
    pub transcriber: Option<TranscriberChoice>,
    /// Arguments this build doesn't act on, for a warning.
    pub ignored: Vec<String>,
}

/// Sorts `argv` (program path first) into what to act on and what to
/// ignore. macOS's `-psn_…` process serial number is dropped silently.
#[must_use]
pub fn parse(argv: &[String], test_flags: bool) -> LaunchFlags {
    let mut flags = LaunchFlags::default();
    let mut args = argv.iter().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            PRESS_FLAG if test_flags => flags.events.push(HotkeyEvent::Pressed),
            RELEASE_FLAG if test_flags => flags.events.push(HotkeyEvent::Released),
            TRANSCRIBER_FLAG => match args.next() {
                Some(name) => flags.transcriber(name),
                None => flags.ignored.push(TRANSCRIBER_FLAG.to_owned()),
            },
            psn if psn.starts_with("-psn_") => {}
            other => match other
                .strip_prefix(TRANSCRIBER_FLAG)
                .and_then(|rest| rest.strip_prefix('='))
            {
                Some(name) => flags.transcriber(name),
                None => flags.ignored.push(other.to_owned()),
            },
        }
    }
    flags
}

impl LaunchFlags {
    fn transcriber(&mut self, name: &str) {
        match TranscriberChoice::from_name(name) {
            Some(choice) => self.transcriber = Some(choice),
            None => self.ignored.push(format!("{TRANSCRIBER_FLAG} {name}")),
        }
    }
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
    fn transcriber_flag_picks_the_engine_in_every_build() {
        for test_flags in [true, false] {
            for args in [&["--transcriber", "mock"][..], &["--transcriber=mock"]] {
                let flags = parse(&argv(args), test_flags);
                assert_eq!(flags.transcriber, Some(TranscriberChoice::Mock), "{args:?}");
                assert!(flags.ignored.is_empty());
            }
        }
        assert_eq!(parse(&argv(&[]), true).transcriber, None);
    }

    #[test]
    fn unknown_or_missing_engine_is_ignored() {
        let flags = parse(&argv(&["--transcriber", "whisper", "--press"]), true);
        assert_eq!(flags.transcriber, None);
        assert_eq!(flags.ignored, ["--transcriber whisper"]);
        // The value is consumed even when unknown, so the next flag still counts.
        assert_eq!(flags.events, [Pressed]);

        let flags = parse(&argv(&["--transcriber=", "--transcriber"]), true);
        assert_eq!(flags.transcriber, None);
        assert_eq!(flags.ignored, ["--transcriber ", "--transcriber"]);

        let flags = parse(&argv(&["--transcriberx=mock"]), true);
        assert_eq!(flags.ignored, ["--transcriberx=mock"]);
    }

    #[test]
    fn engine_names_round_trip() {
        let choice = TranscriberChoice::default();
        assert_eq!(choice, TranscriberChoice::Mock);
        assert_eq!(TranscriberChoice::from_name(choice.name()), Some(choice));
        assert_eq!(TranscriberChoice::from_name("Mock"), None);
    }

    #[test]
    fn test_flags_follow_the_build() {
        assert_eq!(TEST_FLAGS, cfg!(debug_assertions));
    }
}
