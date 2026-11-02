//! #144: real panics, in a process of their own (the panic hook and the
//! global subscriber can be set once per process): the hook the app installs
//! first reports a panic before logging starts, to stderr, and once logging
//! has started, to the log; in both, a literal message is shown and a
//! formatted one, holding a unique sentinel, is not (at info level).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{LEVEL_VARIABLE, LOG_FILE_PREFIX, Limits, init_with, install_panic_hook};

/// Set in the child: where it logs.
const CHILD_DIR: &str = "DICTATION_144_PANIC_CHILD_DIR";
const TEST_NAME: &str =
    "logging::spawned_panic::panics_are_reported_before_and_after_logging_starts";

/// In a formatted panic message only; must never be printed.
const SENTINEL: &str = "fmt-panic-sentinel-Zr81Kx";

/// What the child's literal panics say (written out again in `panic!`,
/// which needs a literal).
const EARLY_LITERAL: &str = "early literal panic 6Hq2";
const LATE_LITERAL: &str = "late literal panic 6Hq2";

/// The line the hook writes while logging hasn't started.
const BEFORE_LOGGING: &str = "panicked before logging started";

#[test]
fn panics_are_reported_before_and_after_logging_starts() {
    if let Some(dir) = std::env::var_os(CHILD_DIR) {
        child(Path::new(&dir));
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "dictation-144-panic-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let output = Command::new(std::env::current_exe().unwrap())
        .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_DIR, &dir)
        .env(LEVEL_VARIABLE, "info")
        .env("RUST_BACKTRACE", "0")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let log: String = log_files(&dir)
        .iter()
        .map(|file| std::fs::read_to_string(file).unwrap())
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "child failed ({}):\n{stdout}\n{stderr}",
        output.status
    );

    // Before logging: both panics went through our hook, to stderr, and the
    // default hook ("thread '…' panicked at …") never ran.
    let early: Vec<&str> = stderr
        .lines()
        .filter(|line| line.contains(BEFORE_LOGGING))
        .collect();
    assert_eq!(early.len(), 2, "{stderr}");
    assert!(early[0].contains(EARLY_LITERAL), "{stderr}");
    assert!(early[1].contains("formatted, <"), "{stderr}");
    assert!(
        !stderr.contains("panicked at"),
        "the default hook ran:\n{stderr}"
    );

    // After: in the log file, the literal shown, the formatted one hidden.
    let late: Vec<&str> = log
        .lines()
        .filter(|line| line.contains("a thread panicked"))
        .collect();
    assert_eq!(late.len(), 2, "{log}");
    assert!(late[0].contains(LATE_LITERAL), "{log}");
    assert!(late[1].contains("formatted, <"), "{log}");
    assert!(!log.contains(BEFORE_LOGGING), "{log}");
    assert!(!log.contains(EARLY_LITERAL), "{log}");

    for (name, text) in [("stdout", &stdout), ("stderr", &stderr), ("log", &log)] {
        assert!(
            !text.contains(SENTINEL),
            "the sentinel is in the {name}:\n{text}"
        );
    }
}

/// The app's order (`crate::run`): the hook first, logging later.
fn child(dir: &Path) {
    install_panic_hook();
    panic_on_a_thread(|| panic!("early literal panic 6Hq2"));
    panic_on_a_thread(|| panic!("early formatted panic {}", sentinel()));
    init_with(dir, Limits::APP).unwrap();
    panic_on_a_thread(|| panic!("late literal panic 6Hq2"));
    panic_on_a_thread(|| panic!("late formatted panic {}", sentinel()));
}

/// Opaque to the compiler, so the message is formatted at run time.
fn sentinel() -> &'static str {
    std::hint::black_box(SENTINEL)
}

/// Panics on a thread of its own and waits for it, so the panics are
/// reported in order.
fn panic_on_a_thread(body: fn()) {
    assert!(std::thread::spawn(body).join().is_err());
}

fn log_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(LOG_FILE_PREFIX))
        })
        .collect();
    files.sort();
    files
}
