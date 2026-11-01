//! Logging with `tracing` (M5.3): info and above to a size-bounded rotating
//! file testers can send, and to stderr.
//!
//! What may be logged (`docs/ENGINEERING.md`): never an API key, at any
//! level; never a transcript at info or above, and at debug only its length.
//! That is enforced where the data enters the log, by types and summaries
//! that cannot print it (`dictation_core::redact`, `dictation_ports::Secret`,
//! the `Debug` of `Event`, `Command` and the error enums), and, as a last
//! resort, by [`scrub::ScrubbingWriter`], which cleans every line that is
//! written. `DICTATION_LOG=debug` turns on debug output for our own crates,
//! for testers who are asked to.

pub mod rotation;
pub mod scrub;

use std::path::Path;
use std::str::FromStr;

use tracing::Subscriber;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::{self, MakeWriter};
use tracing_subscriber::prelude::*;

use rotation::RotatingFile;
use scrub::ScrubbingWriter;

/// The log files' name; a timestamp is appended
/// (`dictation-app.log.2026-11-01T153045.123Z`).
pub const LOG_FILE_PREFIX: &str = "dictation-app.log";

/// A log file is closed when it reaches this size.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Log files kept in all, the current one included, so the log directory
/// holds at most `KEEP_FILES * MAX_FILE_BYTES` (10 MiB).
pub const KEEP_FILES: usize = 5;

/// The environment variable that sets the level for our own crates.
pub const LEVEL_VARIABLE: &str = "DICTATION_LOG";

/// Our own crates' `tracing` targets.
const OWN_CRATES: [&str; 6] = [
    "dictation_app_lib",
    "dictation_core",
    "dictation_pipeline",
    "dictation_platform",
    "dictation_ports",
    "dictation_transcribe",
];

/// The level asked for in [`LEVEL_VARIABLE`] (`error`, `warn`, `info`,
/// `debug`, `trace` or `off`); info if it is missing or not one of those.
#[must_use]
pub fn level_from(value: Option<&str>) -> LevelFilter {
    value
        .map(str::trim)
        // `from_str` reads an empty string as the lowest level.
        .filter(|value| !value.is_empty())
        .and_then(|value| LevelFilter::from_str(value).ok())
        .unwrap_or(LevelFilter::INFO)
}

/// Our crates log at `level`; everything else (Tauri, HTTP, TLS) at info at
/// most, so a debug log is ours, not a flood from dependencies.
fn filter(level: LevelFilter) -> Targets {
    let others = level.min(LevelFilter::INFO);
    Targets::new()
        .with_default(others)
        .with_targets(OWN_CRATES.map(|name| (name, level)))
}

/// The subscriber the app uses, writing to `file` and `console`.
fn subscriber<F, C>(level: LevelFilter, file: F, console: C) -> impl Subscriber + Send + Sync
where
    F: for<'a> MakeWriter<'a> + Send + Sync + 'static,
    C: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::registry()
        .with(filter(level))
        .with(fmt::layer().with_ansi(false).with_writer(file))
        .with(fmt::layer().with_ansi(false).with_writer(console))
}

/// Starts logging into `dir`, at the level [`LEVEL_VARIABLE`] asks for.
///
/// # Errors
///
/// If the directory or file can't be created or a global subscriber is
/// already set.
pub fn init(dir: &Path) -> anyhow::Result<()> {
    let file = RotatingFile::open(dir, LOG_FILE_PREFIX, MAX_FILE_BYTES, KEEP_FILES)?;
    let level = level_from(std::env::var(LEVEL_VARIABLE).ok().as_deref());
    subscriber(
        level,
        ScrubbingWriter::new(file),
        ScrubbingWriter::new(std::io::stderr()),
    )
    .try_init()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use dictation_core::coordinator::Phase;
    use dictation_core::error::{DictationError, Permission};
    use dictation_core::events::{Command, Event};
    use dictation_core::redact::Hidden;
    use dictation_ports::{Secret, TranscribeError};

    use super::*;

    /// Obviously fake.
    const KEY: &str = "gsk_dummy_NOT_A_REAL_KEY_0123456789";
    const OPENAI_KEY: &str = "sk-proj-NOTAREALKEY0123456789abcdef";
    const TRANSCRIPT: &str = "my secret transcript about the merger";

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Capture {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Runs `log` under the app's subscriber at `level`, as the file sees it.
    fn captured(level: LevelFilter, log: impl FnOnce()) -> String {
        let capture = Capture::default();
        let subscriber = subscriber(
            level,
            ScrubbingWriter::new(capture.clone()),
            ScrubbingWriter::new(io::sink()),
        );
        tracing::subscriber::with_default(subscriber, log);
        capture.text()
    }

    /// One line at every level, for every way data gets into a log.
    fn log_everything() {
        let event = Event::TranscriptReady(TRANSCRIPT.into());
        let command = Command::Insert(TRANSCRIPT.into());
        let failed = Event::TranscriptionFailed(format!("rejected {KEY}"));
        let errors = [
            TranscribeError::Network(format!("{KEY} {OPENAI_KEY}")),
            TranscribeError::Engine(format!("body: {{\"key\": \"{KEY}\"}}")),
            TranscribeError::KeychainUnavailable(format!("locked {KEY}")),
        ];
        let shown = Command::ShowError(DictationError::TranscriptionFailed {
            reason: KEY.to_owned(),
        });
        let secret = Secret::new(KEY);
        macro_rules! at {
            ($level:ident) => {
                tracing::$level!(?event, ?command, ?failed, ?shown, "events");
                tracing::$level!(?secret, "secret");
                tracing::$level!(text = %Hidden(TRANSCRIPT), "hidden");
                tracing::$level!(text = ?Hidden(TRANSCRIPT), "hidden");
                for error in &errors {
                    tracing::$level!(%error, ?error, "error");
                }
                // A dependency that logs a header, which no type protects.
                tracing::$level!("sending Authorization: Bearer {KEY}");
                tracing::$level!(request = format!("key={OPENAI_KEY}"), "request");
            };
        }
        at!(error);
        at!(warn);
        at!(info);
        at!(debug);
        at!(trace);
    }

    #[test]
    fn criterion_1_no_key_in_the_log_at_any_level() {
        for level in [
            LevelFilter::ERROR,
            LevelFilter::WARN,
            LevelFilter::INFO,
            LevelFilter::DEBUG,
            LevelFilter::TRACE,
        ] {
            let log = captured(level, log_everything);
            assert!(!log.is_empty(), "{level}: something was logged");
            for key in [KEY, OPENAI_KEY, "NOT_A_REAL_KEY", "NOTAREALKEY"] {
                assert!(!log.contains(key), "{level}: {key} in\n{log}");
            }
        }
    }

    #[test]
    fn criterion_1_no_transcript_at_info_or_above_and_none_at_debug_either() {
        for level in [
            LevelFilter::ERROR,
            LevelFilter::WARN,
            LevelFilter::INFO,
            LevelFilter::DEBUG,
            LevelFilter::TRACE,
        ] {
            let log = captured(level, log_everything);
            assert!(!log.contains("secret transcript"), "{level}:\n{log}");
            assert!(!log.contains("merger"), "{level}:\n{log}");
        }
        // The length is what debug may show.
        let log = captured(LevelFilter::DEBUG, log_everything);
        assert!(log.contains(&format!("len: {}", TRANSCRIPT.len())), "{log}");
    }

    #[test]
    fn criterion_1_the_redactions_keep_what_is_useful() {
        let log = captured(LevelFilter::INFO, || {
            let phase = Phase::Transcribing;
            let permission = Event::PermissionMissing(Permission::Microphone);
            tracing::info!(?phase, ?permission, duration = ?Duration::from_secs(2), "state");
            tracing::info!(error = %TranscribeError::Engine("the provider failed (HTTP 503)".into()), "dictation failed");
        });
        assert!(log.contains("phase=Transcribing"), "{log}");
        assert!(log.contains("PermissionMissing(Microphone)"), "{log}");
        assert!(log.contains("the provider failed (HTTP 503)"), "{log}");
    }

    #[test]
    fn registered_keys_are_hidden_even_without_a_known_shape() {
        let odd = "correct-horse-battery-staple-key";
        dictation_core::redact::register_secret(odd);
        let log = captured(LevelFilter::INFO, || {
            tracing::info!("the provider echoed {odd}");
        });
        assert!(!log.contains(odd), "{log}");
        assert!(log.contains("[key hidden]"), "{log}");
    }

    #[test]
    fn the_level_comes_from_the_variable_and_defaults_to_info() {
        assert_eq!(level_from(None), LevelFilter::INFO);
        assert_eq!(level_from(Some("debug")), LevelFilter::DEBUG);
        assert_eq!(level_from(Some(" TRACE ")), LevelFilter::TRACE);
        assert_eq!(level_from(Some("off")), LevelFilter::OFF);
        assert_eq!(level_from(Some("loud")), LevelFilter::INFO);
        assert_eq!(level_from(Some("")), LevelFilter::INFO);
    }

    #[test]
    fn debug_is_for_our_crates_only() {
        let log = captured(LevelFilter::DEBUG, || {
            tracing::debug!(target: "dictation_pipeline", "ours");
            tracing::debug!(target: "hyper_util", "theirs");
            tracing::info!(target: "hyper_util", "theirs at info");
        });
        assert!(log.contains("ours"), "{log}");
        assert!(!log.contains("theirs\n"), "{log}");
        assert!(log.contains("theirs at info"), "{log}");
    }

    #[test]
    fn init_writes_to_a_rotating_file_in_the_directory() {
        // Not callable twice per process (a global subscriber), so only the
        // pieces: the file opens in the directory and takes the scrubbed text.
        let dir = std::env::temp_dir().join(format!("dictation-log-init-{}", std::process::id()));
        let file = RotatingFile::open(&dir, LOG_FILE_PREFIX, MAX_FILE_BYTES, KEEP_FILES).unwrap();
        let writer = ScrubbingWriter::new(file);
        let subscriber = subscriber(
            LevelFilter::INFO,
            writer.clone(),
            io::sink as fn() -> io::Sink,
        );
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("hello {KEY}");
        });
        let mut found = String::new();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            assert!(
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(LOG_FILE_PREFIX)
            );
            found.push_str(&std::fs::read_to_string(entry.path()).unwrap());
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert!(found.contains("hello [key hidden]"), "{found}");
    }

    #[test]
    fn the_bound_on_the_directory_is_ten_mebibytes() {
        assert_eq!(MAX_FILE_BYTES * KEEP_FILES as u64, 10 * 1024 * 1024);
    }
}
