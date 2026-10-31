//! Logging with `tracing`: info and above to a daily-rotated file testers
//! can send, and to stderr. Nothing logged at info holds a transcript or a
//! key (docs/ENGINEERING.md).

use std::path::Path;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

/// The log files' name; the date is appended (`dictation-app.log.2026-10-31`).
pub const LOG_FILE_PREFIX: &str = "dictation-app.log";

/// Rotated files kept besides today's.
const KEEP_FILES: usize = 7;

/// Starts logging into `dir`.
///
/// # Errors
///
/// If the directory can't be created or a global subscriber is already set.
pub fn init(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let file = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .max_log_files(KEEP_FILES)
        .build(dir)?;
    tracing_subscriber::registry()
        .with(LevelFilter::INFO)
        .with(fmt::layer().with_ansi(false).with_writer(file))
        .with(fmt::layer().with_writer(std::io::stderr))
        .try_init()?;
    Ok(())
}
