//! A log file that rotates by size and keeps a bounded number of files, so
//! the log directory can't grow without limit (the daily rotation of
//! `tracing-appender` bounds the number of files, not their size).
//!
//! Files are named `<prefix>.<UTC timestamp>` (`dictation-app.log.
//! 2026-11-01T153045.123Z`), so sorting the names sorts them by age and
//! `cat dictation-app.log*` reads the log in order. Old daily files from
//! earlier builds (`dictation-app.log.2026-10-31`) sort before the new names
//! and age out like the rest.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A file that starts a new one when the next write would pass `max_bytes`,
/// and deletes the oldest files beyond `keep`.
///
/// The directory therefore never holds more than `keep * max_bytes` bytes of
/// this log: a single write longer than `max_bytes` is cut to fit.
pub struct RotatingFile {
    dir: PathBuf,
    prefix: String,
    max_bytes: u64,
    keep: usize,
    now: Box<dyn Fn() -> SystemTime + Send>,
    current: Option<(File, u64)>,
}

impl RotatingFile {
    /// Logs into `dir`, continuing the newest file if it has room and
    /// starting a new one otherwise.
    ///
    /// # Errors
    ///
    /// If the directory can't be created or read, or the file opened.
    pub fn open(dir: &Path, prefix: &str, max_bytes: u64, keep: usize) -> io::Result<Self> {
        Self::open_with_clock(dir, prefix, max_bytes, keep, Box::new(SystemTime::now))
    }

    /// [`open`](Self::open) with the time of day injected, for tests.
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open).
    pub fn open_with_clock(
        dir: &Path,
        prefix: &str,
        max_bytes: u64,
        keep: usize,
        now: Box<dyn Fn() -> SystemTime + Send>,
    ) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let mut file = Self {
            dir: dir.to_owned(),
            prefix: prefix.to_owned(),
            max_bytes: max_bytes.max(1),
            keep: keep.max(1),
            now,
            current: None,
        };
        file.current = match file.files()?.pop() {
            Some(newest) => {
                let size = fs::metadata(&newest)?.len();
                if size < file.max_bytes {
                    let opened = OpenOptions::new().append(true).open(&newest)?;
                    Some((opened, size))
                } else {
                    None
                }
            }
            None => None,
        };
        if file.current.is_none() {
            file.start_new()?;
        }
        file.prune();
        Ok(file)
    }

    /// The log files in `dir`, oldest first.
    fn files(&self) -> io::Result<Vec<PathBuf>> {
        let start = format!("{}.", self.prefix);
        let mut files: Vec<PathBuf> = fs::read_dir(&self.dir)?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(&start))
            .map(|entry| entry.path())
            .collect();
        files.sort();
        Ok(files)
    }

    /// Starts a file named for now (a millisecond later if that name is
    /// taken, so names always increase).
    fn start_new(&mut self) -> io::Result<()> {
        let mut at = (self.now)();
        let file = loop {
            let name = format!("{}.{}", self.prefix, timestamp(at));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.dir.join(name))
            {
                Ok(file) => break file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    at += std::time::Duration::from_millis(1);
                }
                Err(error) => return Err(error),
            }
        };
        self.current = Some((file, 0));
        Ok(())
    }

    /// Deletes the oldest files beyond `keep`. A file that can't be deleted
    /// stays; the next rotation tries again.
    fn prune(&self) {
        let Ok(files) = self.files() else { return };
        let excess = files.len().saturating_sub(self.keep);
        for old in files.into_iter().take(excess) {
            let _ = fs::remove_file(old);
        }
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Reports the whole buffer as written even when it is cut, or
        // `write_all` would loop and rotate for the rest.
        let data = &buf[..buf
            .len()
            .min(usize::try_from(self.max_bytes).unwrap_or(usize::MAX))];
        let size = self.current.as_ref().map_or(0, |(_, size)| *size);
        if self.current.is_none() || (size > 0 && size + data.len() as u64 > self.max_bytes) {
            self.start_new()?;
            self.prune();
        }
        if let Some((file, size)) = &mut self.current {
            file.write_all(data)?;
            *size += data.len() as u64;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.current {
            Some((file, _)) => file.flush(),
            None => Ok(()),
        }
    }
}

/// `2026-11-01T153045.123Z` for `at` (UTC), with no characters that need
/// quoting in a shell or are illegal in a Windows file name.
fn timestamp(at: SystemTime) -> String {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let (year, month, day) = civil_from_days(secs / 86_400);
    let rest = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}{:02}{:02}.{:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        since.subsec_millis()
    )
}

/// The calendar date of `days` since 1970-01-01 (proleptic Gregorian), by
/// Howard Hinnant's `civil_from_days` algorithm
/// (<https://howardhinnant.github.io/date_algorithms.html>, public domain).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use super::*;

    /// A fresh directory in the temp dir, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "dictation-log-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// A clock that stands still unless moved, shared with the file.
    #[derive(Clone)]
    struct TestClock(Arc<AtomicU64>);

    impl TestClock {
        fn at_secs(secs: u64) -> Self {
            Self(Arc::new(AtomicU64::new(secs * 1000)))
        }

        fn advance_secs(&self, secs: u64) {
            self.0.fetch_add(secs * 1000, Ordering::Relaxed);
        }

        fn boxed(&self) -> Box<dyn Fn() -> SystemTime + Send> {
            let millis = Arc::clone(&self.0);
            Box::new(move || UNIX_EPOCH + Duration::from_millis(millis.load(Ordering::Relaxed)))
        }
    }

    /// 2026-11-01T00:00:00Z
    const NOV_1: u64 = 1_793_491_200;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn total_bytes(dir: &Path) -> u64 {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len())
            .sum()
    }

    #[test]
    fn timestamps_are_utc_dates_that_sort_by_time() {
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T000000.000Z");
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_millis(NOV_1 * 1000 + 55_445_123)),
            "2026-11-01T152405.123Z"
        );
        // A leap day and a year end.
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_secs(1_709_164_800)),
            "2024-02-29T000000.000Z"
        );
        assert_eq!(
            timestamp(UNIX_EPOCH + Duration::from_secs(1_798_761_599)),
            "2026-12-31T235959.000Z"
        );
    }

    #[test]
    fn criterion_2_total_size_stays_bounded_however_much_is_logged() {
        let dir = TempDir::new();
        let clock = TestClock::at_secs(NOV_1);
        let (max_bytes, keep) = (1_000_u64, 3_usize);
        let mut log =
            RotatingFile::open_with_clock(&dir.0, "app.log", max_bytes, keep, clock.boxed())
                .unwrap();
        let line = [b'x'; 90];
        for i in 0..2_000 {
            log.write_all(&line).unwrap();
            if i % 7 == 0 {
                clock.advance_secs(1);
            }
            assert!(
                total_bytes(&dir.0) <= max_bytes * keep as u64,
                "after {i} lines: {} bytes",
                total_bytes(&dir.0)
            );
            assert!(names(&dir.0).len() <= keep);
        }
        // 180 KB went in; the newest three files are what's left.
        assert_eq!(names(&dir.0).len(), keep);
    }

    #[test]
    fn criterion_2_one_huge_write_is_cut_to_the_file_limit() {
        let dir = TempDir::new();
        let mut log = RotatingFile::open(&dir.0, "app.log", 100, 2).unwrap();
        log.write_all(&[b'y'; 10_000]).unwrap();
        log.write_all(&[b'z'; 10_000]).unwrap();
        assert!(total_bytes(&dir.0) <= 200, "{}", total_bytes(&dir.0));
    }

    #[test]
    fn the_oldest_files_go_first_and_names_sort_by_age() {
        let dir = TempDir::new();
        let clock = TestClock::at_secs(NOV_1);
        let mut log =
            RotatingFile::open_with_clock(&dir.0, "app.log", 10, 2, clock.boxed()).unwrap();
        for word in ["aaaaaaaa", "bbbbbbbb", "cccccccc", "dddddddd"] {
            log.write_all(word.as_bytes()).unwrap();
            clock.advance_secs(60);
        }
        let names = names(&dir.0);
        assert_eq!(names.len(), 2, "{names:?}");
        let text: String = names
            .iter()
            .map(|n| fs::read_to_string(dir.0.join(n)).unwrap())
            .collect();
        assert_eq!(text, "ccccccccdddddddd", "oldest first, oldest dropped");
    }

    #[test]
    fn two_files_in_the_same_millisecond_get_distinct_names() {
        let dir = TempDir::new();
        let clock = TestClock::at_secs(NOV_1);
        let mut log =
            RotatingFile::open_with_clock(&dir.0, "app.log", 4, 10, clock.boxed()).unwrap();
        for _ in 0..5 {
            log.write_all(b"abcd").unwrap();
        }
        assert_eq!(names(&dir.0).len(), 5);
    }

    #[test]
    fn a_restart_continues_the_newest_file_if_it_has_room() {
        let dir = TempDir::new();
        let clock = TestClock::at_secs(NOV_1);
        {
            let mut log =
                RotatingFile::open_with_clock(&dir.0, "app.log", 100, 3, clock.boxed()).unwrap();
            log.write_all(b"first run\n").unwrap();
            log.flush().unwrap();
        }
        clock.advance_secs(10);
        let mut log =
            RotatingFile::open_with_clock(&dir.0, "app.log", 100, 3, clock.boxed()).unwrap();
        log.write_all(b"second run\n").unwrap();
        let names = names(&dir.0);
        assert_eq!(names.len(), 1, "{names:?}");
        assert_eq!(
            fs::read_to_string(dir.0.join(&names[0])).unwrap(),
            "first run\nsecond run\n"
        );
    }

    #[test]
    fn a_full_newest_file_and_unrelated_files_are_handled() {
        let dir = TempDir::new();
        // A legacy daily file (full), and a file that isn't ours.
        fs::write(dir.0.join("app.log.2026-10-31"), [b'o'; 50]).unwrap();
        fs::write(dir.0.join("notes.txt"), "keep me").unwrap();
        let mut log = RotatingFile::open(&dir.0, "app.log", 50, 1).unwrap();
        log.write_all(b"new").unwrap();
        let names = names(&dir.0);
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.contains(&"notes.txt".to_owned()));
        assert!(!names.contains(&"app.log.2026-10-31".to_owned()), "pruned");
    }

    #[test]
    fn opening_in_a_missing_directory_creates_it() {
        let dir = TempDir::new();
        let nested = dir.0.join("a").join("b");
        let mut log = RotatingFile::open(&nested, "app.log", 100, 2).unwrap();
        log.write_all(b"x").unwrap();
        log.flush().unwrap();
        assert_eq!(names(&nested).len(), 1);
    }

    #[test]
    fn a_path_that_is_a_file_is_an_error() {
        let dir = TempDir::new();
        let file = dir.0.join("file");
        fs::write(&file, "x").unwrap();
        assert!(RotatingFile::open(&file, "app.log", 100, 2).is_err());
    }
}
