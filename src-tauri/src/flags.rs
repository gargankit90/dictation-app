//! A launch's command-line flags, from this process or forwarded from a
//! second launch (`dictation_platform::single_instance`).
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
//! - `--debug-set-api-key-stdin` and `--debug-delete-api-key` store (the key
//!   comes from stdin, never an argument) and remove the Groq API key through
//!   the same path as the Settings window, then exit without starting the
//!   app. They run in their own process, never in a running app, so a
//!   forwarded second launch ignores them.
//!
//! - `--debug-launch-at-login-on`, `-off` and `-status` set or print the
//!   login item (M5.3) through the same code as the Settings window, then
//!   exit, again in a process of their own.
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
//! - `--debug-provider-url <url>` sends the real engine's requests (and
//!   Settings' Test key) to `url` instead of the provider's own endpoint, so
//!   the signed test build can talk to a local stand-in provider (T4a). Only
//!   a loopback `http` URL is taken (`http://127.0.0.1:<port>/…`,
//!   `http://localhost…`, `http://[::1]…`); anything else is ignored with a
//!   warning, so even a debug build can't be pointed at another host. Like
//!   the audio flags it shapes the pipeline, so only the first launch's
//!   counts.
//!
//! A release build treats them like any other unknown flag: logged and
//! ignored.
//!
//! `--transcriber <name>` picks the speech-to-text engine. It's a normal flag
//! in every build: it only affects the process that starts with it (a
//! forwarded second launch can't switch the running app's engine). `settings`
//! (the default) is the real provider chosen in the settings, with its key
//! from the keychain; `mock` needs no network, key or permission and is what
//! the e2e scenarios run with.

use std::path::PathBuf;

use dictation_pipeline::test_hooks::{
    DEBUG_AUDIO_FILE_FLAG, DEBUG_SAVE_AUDIO_FLAG, KeyCommand, LaunchCommand, PRESS_FLAG,
    RELEASE_FLAG,
};
use dictation_ports::HotkeyEvent;

/// `--mock-text <text>` (or `=<text>`): what the mock transcriber answers
/// (debug builds only).
pub const MOCK_TEXT_FLAG: &str = "--mock-text";

/// `--mock-text-sequence <a,b,c>` (or `=<a,b,c>`): what the mock
/// transcriber answers to the first dictations, one each, in order (debug
/// builds only).
pub const MOCK_TEXT_SEQUENCE_FLAG: &str = "--mock-text-sequence";

/// `--debug-provider-url <url>` (or `=<url>`): where the real engine sends
/// its requests instead of the provider's endpoint (debug builds only;
/// loopback `http` URLs only).
pub const DEBUG_PROVIDER_URL_FLAG: &str = "--debug-provider-url";

/// Whether this build acts on the test hooks.
pub const TEST_FLAGS: bool = cfg!(debug_assertions);

/// `--transcriber <name>` or `--transcriber=<name>`.
pub const TRANSCRIBER_FLAG: &str = "--transcriber";

/// The speech-to-text engine to run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TranscriberChoice {
    /// The provider, model and language from the settings and the provider's
    /// key from the keychain, rebuilt whenever they're saved (M4.4).
    #[default]
    Settings,
    /// `transcribe::mock::MockTranscriber`: fixed text, no network.
    Mock,
}

impl TranscriberChoice {
    /// Every engine, for the "known engines" warning.
    pub const ALL: [Self; 2] = [Self::Settings, Self::Mock];

    /// The known engine names, comma-separated.
    #[must_use]
    pub fn known_names() -> String {
        Self::ALL.map(Self::name).join(", ")
    }

    /// The engine called `name` on the command line.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "settings" => Some(Self::Settings),
            "mock" => Some(Self::Mock),
            _ => None,
        }
    }

    /// The name `--transcriber` takes for this engine.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Settings => "settings",
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
    /// The API key commands, in order (debug builds only): done instead of
    /// starting the app.
    pub key_commands: Vec<KeyCommand>,
    /// The launch-at-login commands, in order (debug builds only): done
    /// instead of starting the app.
    pub launch_commands: Vec<LaunchCommand>,
    /// The text the mock transcriber answers with (debug builds only); the
    /// last one given wins.
    pub mock_text: Option<String>,
    /// The texts the mock transcriber answers the first dictations with (debug
    /// builds only); the last one given wins.
    pub mock_text_sequence: Option<Vec<String>>,
    /// Where the real engine sends its requests instead of the provider's own
    /// endpoint (debug builds only, loopback only); the last one given wins.
    pub provider_url: Option<String>,
    /// `--debug-provider-url` values refused because they aren't a loopback
    /// `http` URL, for a warning.
    pub refused_provider_urls: Vec<String>,
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
            || self.provider_url.is_some()
    }

    fn provider_url(&mut self, url: &str) {
        if is_loopback_http(url) {
            self.provider_url = Some(url.to_owned());
        } else {
            self.refused_provider_urls.push(url.to_owned());
        }
    }
}

/// Whether `url` is plain `http` to this machine: `127.0.0.0/8`, `::1` or
/// `localhost`, with no user name or password.
fn is_loopback_http(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    // An IPv6 host comes back in brackets ("[::1]").
    let loopback = url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    url.scheme() == "http" && loopback && url.username().is_empty() && url.password().is_none()
}

fn split_texts(texts: &str) -> Vec<String> {
    texts.split(',').map(str::to_owned).collect()
}

/// Sorts `argv` (program path first) into what to act on and what to
/// ignore. macOS's `-psn_…` process serial number is dropped silently.
#[must_use]
pub fn parse(argv: &[String], test_flags: bool) -> LaunchFlags {
    let mut flags = LaunchFlags::default();
    let mut args = argv.iter().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            PRESS_FLAG if test_flags => flags.events.push(HotkeyEvent::Pressed),
            RELEASE_FLAG if test_flags => flags.events.push(HotkeyEvent::Released),
            DEBUG_SAVE_AUDIO_FLAG if test_flags => flags.save_audio = true,
            key_flag if test_flags && KeyCommand::from_flag(key_flag).is_some() => {
                flags.key_commands.extend(KeyCommand::from_flag(key_flag));
            }
            login_flag if test_flags && LaunchCommand::from_flag(login_flag).is_some() => {
                flags
                    .launch_commands
                    .extend(LaunchCommand::from_flag(login_flag));
            }
            // A flag right after it is not its path (#73): the path is missing,
            // so this one is ignored with a warning and the next flag still acts.
            DEBUG_AUDIO_FILE_FLAG if test_flags => {
                match args.next_if(|next| !next.starts_with("--")) {
                    Some(path) => flags.audio_file = Some(PathBuf::from(path)),
                    None => flags.ignored.push(arg.clone()),
                }
            }
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
            DEBUG_PROVIDER_URL_FLAG if test_flags => match args.next() {
                Some(url) => flags.provider_url(url),
                None => flags.ignored.push(arg.clone()),
            },
            other
                if test_flags
                    && other
                        .strip_prefix(DEBUG_PROVIDER_URL_FLAG)
                        .and_then(|rest| rest.strip_prefix('='))
                        .is_some() =>
            {
                if let Some(url) = other
                    .strip_prefix(DEBUG_PROVIDER_URL_FLAG)
                    .and_then(|rest| rest.strip_prefix('='))
                {
                    flags.provider_url(url);
                }
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
    /// The unknown `--transcriber` names worth a warning. A forwarded launch
    /// (`forwarded`) is the second process's flags arriving in the running
    /// app, which ignores `--transcriber` anyway, and that process already
    /// warned about its own argv, so the running app stays quiet (#70).
    #[must_use]
    pub fn unknown_engines_to_warn_about(&self, forwarded: bool) -> &[String] {
        if forwarded {
            &[]
        } else {
            &self.unknown_engines
        }
    }

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

    /// #73: a flag after `--debug-audio-file` is not its path.
    #[test]
    fn debug_audio_file_without_a_path_does_not_swallow_the_next_flag() {
        for next in [
            "--press",
            "--release",
            "--debug-save-audio",
            "--transcriber=mock",
        ] {
            let flags = parse(&argv(&["--debug-audio-file", next, "--press"]), true);
            assert_eq!(flags.audio_file, None, "{next}");
            assert_eq!(flags.ignored, ["--debug-audio-file"], "{next}");
            assert!(flags.events.contains(&Pressed), "{next} still acts");
        }
        // Nothing after it at all.
        let flags = parse(&argv(&["--debug-audio-file"]), true);
        assert_eq!(flags.audio_file, None);
        assert_eq!(flags.ignored, ["--debug-audio-file"]);
        // A real path, even a dashed one that is not a flag, still counts.
        let flags = parse(
            &argv(&["--debug-audio-file", "-weird.wav", "--press"]),
            true,
        );
        assert_eq!(flags.audio_file, Some(PathBuf::from("-weird.wav")));
        assert_eq!(flags.events, [Pressed]);
        let flags = parse(&argv(&["--debug-audio-file", "/tmp/a.wav"]), true);
        assert_eq!(flags.audio_file, Some(PathBuf::from("/tmp/a.wav")));
    }

    /// #70: the running app doesn't repeat the warning the second process
    /// already gave for its own argv.
    #[test]
    fn a_forwarded_launch_does_not_repeat_the_unknown_engine_warning() {
        let flags = parse(&argv(&["--transcriber", "whisperr"]), true);
        assert_eq!(flags.unknown_engines_to_warn_about(false), ["whisperr"]);
        assert!(flags.unknown_engines_to_warn_about(true).is_empty());
        // The name is still recorded (and still ignored by the running app).
        assert_eq!(flags.unknown_engines, ["whisperr"]);
        assert_eq!(flags.transcriber, None);
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
        let flags = parse(&argv(&["--transcriber", "settings"]), true);
        assert_eq!(flags.transcriber, Some(TranscriberChoice::Settings));
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
        assert_eq!(choice, TranscriberChoice::Settings, "the real engine");
        assert_eq!(TranscriberChoice::from_name(choice.name()), Some(choice));
        assert_eq!(TranscriberChoice::from_name("Mock"), None);
        assert_eq!(TranscriberChoice::known_names(), "settings, mock");
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
    fn debug_builds_take_the_key_commands_in_order() {
        let flags = parse(
            &argv(&[
                "--debug-delete-api-key",
                "--debug-set-api-key-stdin",
                "--press",
            ]),
            true,
        );
        assert_eq!(flags.key_commands, [KeyCommand::Delete, KeyCommand::Set]);
        assert_eq!(flags.events, [Pressed]);
        assert!(flags.ignored.is_empty());
        assert!(parse(&argv(&["--press"]), true).key_commands.is_empty());
    }

    #[test]
    fn debug_builds_take_the_launch_at_login_commands_in_order() {
        let flags = parse(
            &argv(&[
                "--debug-launch-at-login-on",
                "--debug-launch-at-login-status",
                "--debug-launch-at-login-off",
            ]),
            true,
        );
        assert_eq!(
            flags.launch_commands,
            [LaunchCommand::On, LaunchCommand::Status, LaunchCommand::Off]
        );
        assert!(flags.ignored.is_empty());
        // Release builds treat them as unknown flags.
        let flags = parse(&argv(&["--debug-launch-at-login-on"]), false);
        assert!(flags.launch_commands.is_empty());
        assert_eq!(flags.ignored, ["--debug-launch-at-login-on"]);
    }

    #[test]
    fn release_builds_ignore_the_key_commands() {
        let flags = parse(
            &argv(&["--debug-set-api-key-stdin", "--debug-delete-api-key"]),
            false,
        );
        assert!(flags.key_commands.is_empty());
        assert_eq!(
            flags.ignored,
            ["--debug-set-api-key-stdin", "--debug-delete-api-key"]
        );
    }

    #[test]
    fn near_miss_key_flags_are_not_commands() {
        let flags = parse(
            &argv(&["--debug-set-api-key", "--debug-delete-api-key=1"]),
            true,
        );
        assert!(flags.key_commands.is_empty());
        assert_eq!(flags.ignored.len(), 2);
    }

    const LOCAL: &str = "http://127.0.0.1:18731/v1";

    #[test]
    fn debug_builds_take_a_loopback_provider_url_in_either_form() {
        let flags = parse(&argv(&["--debug-provider-url", LOCAL, "--press"]), true);
        assert_eq!(flags.provider_url.as_deref(), Some(LOCAL));
        assert_eq!(flags.events, [Pressed]);
        assert!(flags.ignored.is_empty());
        assert!(flags.configure_pipeline(), "only the first launch's counts");
        for url in [
            "http://localhost:9/v1",
            "http://LOCALHOST/v1",
            "http://127.0.0.2:80",
            "http://[::1]:18731/v1",
        ] {
            let flags = parse(&argv(&[&format!("--debug-provider-url={url}")]), true);
            assert_eq!(flags.provider_url.as_deref(), Some(url));
        }
        // The last one wins; without a value the flag is ignored.
        let flags = parse(
            &argv(&[
                "--debug-provider-url=http://localhost:1",
                "--debug-provider-url",
                LOCAL,
            ]),
            true,
        );
        assert_eq!(flags.provider_url.as_deref(), Some(LOCAL));
        let flags = parse(&argv(&["--debug-provider-url"]), true);
        assert_eq!(flags.provider_url, None);
        assert_eq!(flags.ignored, ["--debug-provider-url"]);
    }

    /// Even a debug build can't be pointed at another machine, over TLS or
    /// not, or at a URL carrying credentials.
    #[test]
    fn a_provider_url_that_is_not_loopback_http_is_refused() {
        for url in [
            "https://127.0.0.1:18731/v1",
            "http://api.groq.com/openai/v1",
            "http://10.0.0.1/v1",
            "http://localhost.example.com/v1",
            "http://user:pw@127.0.0.1/v1",
            "file:///tmp/x",
            "127.0.0.1:18731",
            "",
        ] {
            let flags = parse(&argv(&["--debug-provider-url", url]), true);
            assert_eq!(flags.provider_url, None, "{url}");
            assert_eq!(flags.refused_provider_urls, [url], "{url}");
            assert!(!flags.configure_pipeline(), "{url}");
        }
    }

    /// T4a criterion 1: a release build never honours the override; the flag
    /// and its value are unknown arguments there.
    #[test]
    fn release_builds_ignore_the_provider_url() {
        let flags = parse(
            &argv(&[
                "--debug-provider-url",
                LOCAL,
                &format!("--debug-provider-url={LOCAL}"),
            ]),
            false,
        );
        assert_eq!(flags.provider_url, None);
        assert!(flags.refused_provider_urls.is_empty());
        assert!(!flags.configure_pipeline());
        assert_eq!(
            flags.ignored,
            [
                "--debug-provider-url".to_owned(),
                LOCAL.to_owned(),
                format!("--debug-provider-url={LOCAL}")
            ]
        );
    }

    /// The build the app runs as decides: a test build (debug) honours the
    /// override and a release build doesn't. Run under `--release` this
    /// checks the release side.
    #[test]
    fn the_provider_url_is_honoured_only_in_test_builds() {
        let flags = parse(&argv(&["--debug-provider-url", LOCAL]), TEST_FLAGS);
        assert_eq!(flags.provider_url.is_some(), cfg!(debug_assertions));
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
