//! A launch's command-line flags, from this process or forwarded by the
//! single-instance plugin from a second launch.
//!
//! The test hooks exist only in debug builds ([`TEST_FLAGS`]):
//!
//! - `--press` and `--release` inject hotkey events. The single-instance
//!   socket accepts any local process, so in a release build a forwarded
//!   `--press` would let any process start dictation (and later paste) with
//!   this app's permissions.
//! - `--debug-audio-file <wav>` records that file instead of the
//!   microphone, and `--debug-save-audio` saves every recording as a WAV
//!   (debug mode). Both choose how the pipeline is built, so only the first
//!   launch's count; a second launch's are logged and ignored.
//!
//! - `--mock-text <text>` makes the mock transcriber answer with that text
//!   instead of its default, so a test can look for it where it is pasted.
//!   Like the audio flags it shapes the pipeline, so only the first
//!   launch's counts.
//!
//! - `--mock-text-sequence <a,b,c>` makes the mock answer with those texts,
//!   one per transcription in order, and then with its usual text. For a test
//!   that must tell which dictation's text ended up where (the texts can't
//!   hold a comma). Only the first launch's counts.
//!
//! A release build treats them like any other unknown flag: logged and
//! ignored.
//!
//! `--transcriber <name>` picks the speech-to-text engine. It's a normal flag
//! in every build: it only affects the process that starts with it (a
//! forwarded second launch can't switch the running app's engine), and the
//! only engine so far, `mock`, needs no network, key or permission.

use std::path::PathBuf;

use dictation_pipeline::test_hooks::{
    DEBUG_AUDIO_FILE_FLAG, DEBUG_SAVE_AUDIO_FLAG, PRESS_FLAG, RELEASE_FLAG,
};
use dictation_ports::HotkeyEvent;

/// `--mock-text <text>` (or `=<text>`): what the mock transcriber answers
/// (debug builds only).
pub const MOCK_TEXT_FLAG: &str = "--mock-text";

/// `--mock-text-sequence <a,b,c>` (or `=<a,b,c>`): what the mock
/// transcriber answers to the first dictations, one each, in order (debug
/// builds only).
pub const MOCK_TEXT_SEQUENCE_FLAG: &str = "--mock-text-sequence";

/// Whether this build acts on the test hooks.
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
    /// Every engine, for the "known engines" warning.
    pub const ALL: [Self; 1] = [Self::Mock];

    /// The known engine names, comma-separated.
    #[must_use]
    pub fn known_names() -> String {
        Self::ALL.map(Self::name).join(", ")
    }

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
    /// The WAV to record instead of the microphone (debug builds only); the
    /// last one given wins.
    pub audio_file: Option<PathBuf>,
    /// Save every recording as a WAV (debug builds only).
    pub save_audio: bool,
    /// The text the mock transcriber answers with (debug builds only); the
    /// last one given wins.
    pub mock_text: Option<String>,
    /// The texts the mock transcriber answers the first dictations with (debug
    /// builds only); the last one given wins.
    pub mock_text_sequence: Option<Vec<String>>,
    /// The engine `--transcriber` asked for; the last one wins.
    pub transcriber: Option<TranscriberChoice>,
    /// Engine names `--transcriber` was given that no engine has.
    pub unknown_engines: Vec<String>,
    /// Arguments this build doesn't act on, for a warning.
    pub ignored: Vec<String>,
}

impl LaunchFlags {
    /// Whether these flags would change how the pipeline is built (its
    /// audio or its mock transcript), which only the first launch can do.
    pub fn configure_pipeline(&self) -> bool {
        self.audio_file.is_some()
            || self.save_audio
            || self.mock_text.is_some()
            || self.mock_text_sequence.is_some()
    }
}

fn split_texts(texts: &str) -> Vec<String> {
    texts.split(',').map(str::to_owned).collect()
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
            DEBUG_SAVE_AUDIO_FLAG if test_flags => flags.save_audio = true,
            DEBUG_AUDIO_FILE_FLAG if test_flags => match args.next() {
                Some(path) => flags.audio_file = Some(PathBuf::from(path)),
                None => flags.ignored.push(arg.clone()),
            },
            other if test_flags && audio_file_value(other).is_some() => {
                flags.audio_file = audio_file_value(other).map(PathBuf::from);
            }
            MOCK_TEXT_FLAG if test_flags => match args.next() {
                Some(text) => flags.mock_text = Some(text.clone()),
                None => flags.ignored.push(arg.clone()),
            },
            other
                if test_flags
                    && other
                        .strip_prefix(MOCK_TEXT_FLAG)
                        .and_then(|rest| rest.strip_prefix('='))
                        .is_some() =>
            {
                flags.mock_text = other
                    .strip_prefix(MOCK_TEXT_FLAG)
                    .and_then(|rest| rest.strip_prefix('='))
                    .map(str::to_owned);
            }
            MOCK_TEXT_SEQUENCE_FLAG if test_flags => match args.next() {
                Some(texts) => flags.mock_text_sequence = Some(split_texts(texts)),
                None => flags.ignored.push(arg.clone()),
            },
            other
                if test_flags
                    && other
                        .strip_prefix(MOCK_TEXT_SEQUENCE_FLAG)
                        .and_then(|rest| rest.strip_prefix('='))
                        .is_some() =>
            {
                flags.mock_text_sequence = other
                    .strip_prefix(MOCK_TEXT_SEQUENCE_FLAG)
                    .and_then(|rest| rest.strip_prefix('='))
                    .map(split_texts);
            }
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

/// `wav` in `--debug-audio-file=wav`.
fn audio_file_value(arg: &str) -> Option<&str> {
    arg.strip_prefix(DEBUG_AUDIO_FILE_FLAG)?
        .strip_prefix('=')
        .filter(|value| !value.is_empty())
}

impl LaunchFlags {
    fn transcriber(&mut self, name: &str) {
        match TranscriberChoice::from_name(name) {
            Some(choice) => self.transcriber = Some(choice),
            None => self.unknown_engines.push(name.to_owned()),
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
        assert_eq!(flags.unknown_engines, ["whisper"]);
        assert!(flags.ignored.is_empty());
        // The value is consumed even when unknown, so the next flag still counts.
        assert_eq!(flags.events, [Pressed]);

        let flags = parse(&argv(&["--transcriber=", "--transcriber"]), true);
        assert_eq!(flags.transcriber, None);
        assert_eq!(flags.unknown_engines, [""]);
        assert_eq!(flags.ignored, ["--transcriber"]);

        let flags = parse(&argv(&["--transcriberx=mock"]), true);
        assert_eq!(flags.ignored, ["--transcriberx=mock"]);
    }

    #[test]
    fn engine_names_round_trip() {
        let choice = TranscriberChoice::default();
        assert_eq!(choice, TranscriberChoice::Mock);
        assert_eq!(TranscriberChoice::from_name(choice.name()), Some(choice));
        assert_eq!(TranscriberChoice::from_name("Mock"), None);
        assert_eq!(TranscriberChoice::known_names(), "mock");
        for choice in TranscriberChoice::ALL {
            assert_eq!(TranscriberChoice::from_name(choice.name()), Some(choice));
        }
    }

    #[test]
    fn test_flags_follow_the_build() {
        assert_eq!(TEST_FLAGS, cfg!(debug_assertions));
    }

    #[test]
    fn debug_builds_take_the_audio_flags_in_either_form() {
        let flags = parse(
            &argv(&[
                "--debug-audio-file",
                "/tmp/a.wav",
                "--debug-save-audio",
                "--press",
            ]),
            true,
        );
        assert_eq!(flags.audio_file, Some(PathBuf::from("/tmp/a.wav")));
        assert!(flags.save_audio);
        assert_eq!(flags.events, [Pressed]);
        assert!(flags.ignored.is_empty());
        assert!(flags.configure_pipeline());

        let flags = parse(&argv(&["--debug-audio-file=/tmp/b c.wav"]), true);
        assert_eq!(flags.audio_file, Some(PathBuf::from("/tmp/b c.wav")));
        assert!(!flags.save_audio);
    }

    #[test]
    fn a_debug_audio_file_flag_without_a_path_is_ignored() {
        let flags = parse(&argv(&["--debug-audio-file"]), true);
        assert_eq!(flags.audio_file, None);
        assert_eq!(flags.ignored, ["--debug-audio-file"]);
        let flags = parse(&argv(&["--debug-audio-file="]), true);
        assert_eq!(flags.ignored, ["--debug-audio-file="]);
        assert!(!flags.configure_pipeline());
    }

    #[test]
    fn debug_builds_take_the_mock_text_in_either_form() {
        let flags = parse(&argv(&["--mock-text", "hello world", "--press"]), true);
        assert_eq!(flags.mock_text.as_deref(), Some("hello world"));
        assert_eq!(flags.events, [Pressed]);
        assert!(flags.ignored.is_empty());
        assert!(flags.configure_pipeline());

        let flags = parse(&argv(&["--mock-text=a=b", "--mock-text=last"]), true);
        assert_eq!(flags.mock_text.as_deref(), Some("last"));
        // The flag without a value is ignored, and so is nothing else.
        let flags = parse(&argv(&["--mock-text"]), true);
        assert_eq!(flags.mock_text, None);
        assert_eq!(flags.ignored, ["--mock-text"]);
        assert!(!flags.configure_pipeline());
    }

    #[test]
    fn release_builds_ignore_the_mock_text() {
        let flags = parse(&argv(&["--mock-text", "x", "--mock-text=y"]), false);
        assert_eq!(flags.mock_text, None);
        assert_eq!(flags.ignored, ["--mock-text", "x", "--mock-text=y"]);
    }

    fn strings(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|&t| t.to_owned()).collect()
    }

    #[test]
    fn debug_builds_take_the_mock_text_sequence_in_either_form() {
        let flags = parse(&argv(&["--mock-text-sequence", "one,two,three"]), true);
        assert_eq!(
            flags.mock_text_sequence,
            Some(strings(&["one", "two", "three"]))
        );
        assert_eq!(flags.mock_text, None);
        assert!(flags.ignored.is_empty());
        assert!(flags.configure_pipeline());

        let flags = parse(
            &argv(&["--mock-text-sequence=a=b,c", "--mock-text-sequence=x"]),
            true,
        );
        assert_eq!(flags.mock_text_sequence, Some(strings(&["x"])));
        // Without a value it is ignored.
        let flags = parse(&argv(&["--mock-text-sequence"]), true);
        assert_eq!(flags.mock_text_sequence, None);
        assert_eq!(flags.ignored, ["--mock-text-sequence"]);
        assert!(!flags.configure_pipeline());
    }

    #[test]
    fn release_builds_ignore_the_mock_text_sequence() {
        let flags = parse(
            &argv(&["--mock-text-sequence", "x,y", "--mock-text-sequence=z"]),
            false,
        );
        assert_eq!(flags.mock_text_sequence, None);
        assert_eq!(
            flags.ignored,
            ["--mock-text-sequence", "x,y", "--mock-text-sequence=z"]
        );
    }

    #[test]
    fn release_builds_ignore_the_audio_flags() {
        let flags = parse(
            &argv(&[
                "--debug-audio-file",
                "/tmp/a.wav",
                "--debug-save-audio",
                "--debug-audio-file=x.wav",
            ]),
            false,
        );
        assert_eq!(flags.audio_file, None);
        assert!(!flags.save_audio);
        // The path after the flag is just another unknown argument.
        assert_eq!(
            flags.ignored,
            [
                "--debug-audio-file",
                "/tmp/a.wav",
                "--debug-save-audio",
                "--debug-audio-file=x.wav"
            ]
        );
    }
}
