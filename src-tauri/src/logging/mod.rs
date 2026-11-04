//! Logging with `tracing` (M5.3): info and above to a size-bounded rotating
//! file testers can send, and to stderr.
//!
//! What may be logged (`docs/ENGINEERING.md`): never an API key, at any
//! level; never a transcript at info or above, and at debug only its length.
//! That is enforced where the data enters the log, by types and summaries
//! that cannot print it (`dictation_core::redact`, `dictation_ports::Secret`,
//! the `Debug` of `Event`, `Command` and the error enums), and, as a last
//! resort, by [`scrub::ScrubbingWriter`], which cleans every line that is
//! written. Panics are reported through the same log instead of by Rust's
//! default hook ([`log_panic`]): a literal message shown, a formatted one
//! hidden (#144). `DICTATION_LOG=debug` turns on debug output for our own
//! crates, for testers who are asked to.

pub mod rotation;
pub mod scrub;
#[cfg(test)]
mod spawned_panic;
#[cfg(test)]
mod spawned_privacy;

use std::any::Any;
use std::backtrace::{Backtrace, BacktraceStatus};
use std::fmt as std_fmt;
use std::io;
use std::path::Path;
use std::str::FromStr;
use std::sync::Once;

use dictation_core::redact::Hidden;
use tracing::Subscriber;
use tracing::subscriber::NoSubscriber;
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::fmt::{self, MakeWriter};
use tracing_subscriber::prelude::*;

use rotation::RotatingFile;
use scrub::ScrubbingWriter;

/// The log files' name; a timestamp is appended
/// (`dictation-app.log.s00000007.2026-11-01T153045.123Z`).
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

/// The subscriber the app uses, writing to `file` (if there is one) and
/// `console`.
fn subscriber<F, C>(
    level: LevelFilter,
    file: Option<F>,
    console: C,
) -> impl Subscriber + Send + Sync
where
    F: for<'a> MakeWriter<'a> + Send + Sync + 'static,
    C: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::registry()
        .with(filter(level))
        .with(file.map(|file| fmt::layer().with_ansi(false).with_writer(file)))
        .with(fmt::layer().with_ansi(false).with_writer(console))
}

/// The app's subscriber: the rotating log in `dir` (if it can be opened) and
/// `console`, both scrubbed. The error is why there is no file, if so.
fn app_subscriber<C>(
    dir: &Path,
    level: LevelFilter,
    console: C,
    limits: Limits,
) -> (impl Subscriber + Send + Sync, Option<io::Error>)
where
    C: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let (file, failure) =
        match RotatingFile::open(dir, LOG_FILE_PREFIX, limits.file_bytes, limits.keep) {
            Ok(file) => (Some(ScrubbingWriter::new(file)), None),
            Err(error) => (None, Some(error)),
        };
    (subscriber(level, file, console), failure)
}

/// Starts logging into `dir`, at the level [`LEVEL_VARIABLE`] asks for.
///
/// A log directory that can't be used (read-only, a file in the way, a full
/// disk) is not a reason to stop the app: logging then goes to stderr alone,
/// and says so once.
///
/// # Errors
///
/// If a global subscriber is already set.
pub fn init(dir: &Path) -> anyhow::Result<()> {
    init_with(dir, Limits::APP)
}

/// How big a log file gets and how many are kept.
#[derive(Clone, Copy, Debug)]
struct Limits {
    file_bytes: u64,
    keep: usize,
}

impl Limits {
    /// [`MAX_FILE_BYTES`] and [`KEEP_FILES`].
    const APP: Self = Self {
        file_bytes: MAX_FILE_BYTES,
        keep: KEEP_FILES,
    };
}

/// [`init`] with other file limits; only tests make them small.
fn init_with(dir: &Path, limits: Limits) -> anyhow::Result<()> {
    let level = level_from(std::env::var(LEVEL_VARIABLE).ok().as_deref());
    let (subscriber, failure) =
        app_subscriber(dir, level, ScrubbingWriter::new(io::stderr()), limits);
    // Already installed by the app's first line (`crate::run`); here too
    // for callers that start logging without it.
    install_panic_hook();
    subscriber.try_init()?;
    if let Some(error) = failure {
        tracing::warn!(
            %error,
            dir = %dir.display(),
            "could not open the log file; logging to stderr only"
        );
    }
    Ok(())
}

/// Replaces Rust's default panic hook with [`log_panic`]. The app calls it
/// first, before anything that can panic or fail, logging included (#144):
/// the default hook prints the panic message to stderr as it is. Once is
/// enough; later calls do nothing.
pub fn install_panic_hook() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| std::panic::set_hook(Box::new(log_panic)));
}

/// Reports a panic through the log (file and stderr, both scrubbed) instead
/// of Rust's default hook, which prints the panic message to stderr as it
/// is (#124): the thread, where it happened, the message as far as it is
/// safe ([`PanicMessage`]), and the backtrace if `RUST_BACKTRACE` asks for
/// one. Before logging starts, the same report goes to stderr, scrubbed.
fn log_panic(info: &std::panic::PanicHookInfo<'_>) {
    let thread = std::thread::current();
    let location = info
        .location()
        .map_or_else(|| "unknown".to_owned(), ToString::to_string);
    report_panic(
        thread.name().unwrap_or("unnamed"),
        &location,
        info.payload(),
        &mut io::stderr(),
    );
}

/// [`log_panic`]'s report; `stderr` is where it goes while no subscriber is
/// set.
fn report_panic(
    thread: &str,
    location: &str,
    payload: &(dyn Any + Send),
    stderr: &mut dyn io::Write,
) {
    let message = PanicMessage::of(payload);
    let backtrace = Backtrace::capture();
    let captured = backtrace.status() == BacktraceStatus::Captured;
    let logging = tracing::dispatcher::get_default(|dispatch| !dispatch.is::<NoSubscriber>());
    if !logging {
        let mut line = format!(
            "dictation-app: a thread panicked before logging started thread={thread} location={location} message={message}"
        );
        if captured {
            line.push_str(&format!(" backtrace={backtrace}"));
        }
        line.push('\n');
        // Nowhere to report a failed write to.
        let _ = stderr.write_all(scrub::scrub(&line).as_bytes());
    } else if captured {
        tracing::error!(thread, %location, %message, %backtrace, "a thread panicked");
    } else {
        tracing::error!(thread, %location, %message, "a thread panicked");
    }
}

/// A panic's message, as far as it can be logged (#144). A `&'static str`
/// payload is a literal in the code (`panic!("…")` without arguments,
/// `unwrap()` on `None`), which can't hold a key or a transcript, so it is
/// shown. A formatted one (a `String`: `panic!("{x}")`, `expect`, an index
/// out of bounds) can hold whatever the code had in hand, so, as for a
/// crashed pipeline job, only its length is shown, at every level: the
/// rules allow a transcript's length at debug, nothing more.
enum PanicMessage<'a> {
    Literal(&'static str),
    Formatted(Hidden<'a>),
    /// A payload of another type (`std::panic::panic_any`).
    Other,
}

impl<'a> PanicMessage<'a> {
    fn of(payload: &'a (dyn Any + Send)) -> Self {
        if let Some(literal) = payload.downcast_ref::<&'static str>() {
            Self::Literal(literal)
        } else if let Some(formatted) = payload.downcast_ref::<String>() {
            Self::Formatted(Hidden(formatted))
        } else {
            Self::Other
        }
    }
}

impl std_fmt::Display for PanicMessage<'_> {
    fn fmt(&self, f: &mut std_fmt::Formatter<'_>) -> std_fmt::Result {
        match self {
            Self::Literal(literal) => write!(f, "{literal:?}"),
            Self::Formatted(hidden) => write!(f, "formatted, {hidden}"),
            Self::Other => f.write_str("not text"),
        }
    }
}

#[cfg(test)]
mod tests {
    /// U1 criterion 2: the app is called Vocet, but its log files keep the
    /// prefix, so `cat dictation-app.log*` and the rotation of old files
    /// carry on across the update.
    #[test]
    fn the_log_file_prefix_is_unchanged_by_the_rename() {
        assert_eq!(super::LOG_FILE_PREFIX, "dictation-app.log");
    }

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
            Some(ScrubbingWriter::new(capture.clone())),
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
    fn an_unusable_log_directory_leaves_stderr_logging_that_says_so() {
        // A file where the directory should be: nothing can be created in it.
        let blocker =
            std::env::temp_dir().join(format!("dictation-log-blocked-{}", std::process::id()));
        std::fs::write(&blocker, "x").unwrap();
        let console = Capture::default();
        let (subscriber, failure) = app_subscriber(
            &blocker,
            LevelFilter::INFO,
            ScrubbingWriter::new(console.clone()),
            Limits::APP,
        );
        let failure = failure.expect("the file could not be opened");
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(%failure, dir = %blocker.display(), "could not open the log file; logging to stderr only");
            tracing::info!("still logging {KEY}");
        });
        let _ = std::fs::remove_file(&blocker);
        let log = console.text();
        assert!(log.contains("logging to stderr only"), "{log}");
        assert!(log.contains("still logging"), "{log}");
        assert!(!log.contains(KEY), "scrubbed even without a file: {log}");
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
            Some(writer.clone()),
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

    /// A fake key and a transcript in one formatted message.
    fn formatted_payload() -> Box<dyn Any + Send> {
        Box::new(format!("index 5 of {TRANSCRIPT} with {KEY}"))
    }

    fn literal_payload() -> Box<dyn Any + Send> {
        Box::new("called `Option::unwrap()` on a `None` value")
    }

    /// The payloads real panics carry: a literal one is a `&'static str`,
    /// a formatted one a `String`.
    #[test]
    fn real_panics_carry_the_payload_types_the_hook_tells_apart() {
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let literal = std::panic::catch_unwind(|| panic!("a literal")).unwrap_err();
        let formatted = std::panic::catch_unwind(|| {
            panic!("{}", std::hint::black_box(TRANSCRIPT));
        })
        .unwrap_err();
        let indexed = std::panic::catch_unwind(|| {
            let items = std::hint::black_box([1, 2, 3]);
            items[std::hint::black_box(5)]
        })
        .unwrap_err();
        std::panic::set_hook(quiet);
        assert!(matches!(
            PanicMessage::of(&*literal),
            PanicMessage::Literal("a literal")
        ));
        assert!(matches!(
            PanicMessage::of(&*formatted),
            PanicMessage::Formatted(_)
        ));
        assert!(matches!(
            PanicMessage::of(&*indexed),
            PanicMessage::Formatted(_)
        ));
        assert!(matches!(PanicMessage::of(&42_u8), PanicMessage::Other));
    }

    /// #144: at info (and every other level) a literal message reaches the
    /// log; a formatted one shows only its length.
    #[test]
    fn a_literal_panic_message_is_logged_and_a_formatted_one_is_hidden() {
        for level in [LevelFilter::INFO, LevelFilter::DEBUG, LevelFilter::TRACE] {
            let log = captured(level, || {
                let mut stderr = Vec::new();
                report_panic("worker", "src/x.rs:1:2", &*literal_payload(), &mut stderr);
                report_panic("worker", "src/x.rs:3:4", &*formatted_payload(), &mut stderr);
                assert!(stderr.is_empty(), "logging has started: nothing on stderr");
            });
            assert!(
                log.contains("called `Option::unwrap()` on a `None` value"),
                "{level}: {log}"
            );
            assert!(log.contains("thread=\"worker\""), "{level}: {log}");
            assert!(log.contains("src/x.rs:3:4"), "{level}: {log}");
            assert!(log.contains("formatted, <"), "{level}: {log}");
            assert!(
                !log.contains(TRANSCRIPT) && !log.contains(KEY),
                "{level}: {log}"
            );
        }
    }

    /// Before logging starts, the report goes to stderr, under the same rule
    /// and scrubbed.
    #[test]
    fn before_logging_starts_the_report_goes_to_stderr() {
        let mut stderr = Vec::new();
        report_panic("main", "src/lib.rs:9:9", &*literal_payload(), &mut stderr);
        report_panic("main", "src/lib.rs:9:9", &*formatted_payload(), &mut stderr);
        let text = String::from_utf8(stderr).unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(
            text.lines().all(|line| line.contains(
                "a thread panicked before logging started thread=main location=src/lib.rs:9:9"
            )),
            "{text}"
        );
        assert!(
            text.contains("called `Option::unwrap()` on a `None` value"),
            "{text}"
        );
        assert!(!text.contains(TRANSCRIPT) && !text.contains(KEY), "{text}");
    }

    /// #144: the app installs the hook first thing, before the key
    /// commands, the single-instance check and Tauri (whose `setup` starts
    /// logging), so nothing that runs before can panic through Rust's
    /// default hook. `spawned_panic` checks that the hook works in that
    /// order.
    #[test]
    fn the_app_installs_the_panic_hook_before_anything_else() {
        let source = include_str!("../lib.rs");
        let body = source
            .split_once("pub fn run() {")
            .expect("lib.rs has `pub fn run`")
            .1;
        let first = body
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with("//"))
            .unwrap();
        assert_eq!(first, "logging::install_panic_hook();");
        let hook = body.find("logging::install_panic_hook();").unwrap();
        let init = body.find("logging::init(").unwrap_or(usize::MAX);
        assert!(hook < init);
    }

    #[test]
    fn the_bound_on_the_directory_is_ten_mebibytes() {
        assert_eq!(MAX_FILE_BYTES * KEEP_FILES as u64, 10 * 1024 * 1024);
    }
}
