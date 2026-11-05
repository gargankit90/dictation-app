//! The history file (U7): a file only this user can read, replaced in one
//! step. Nothing here logs; the errors hold the OS's reason and no contents.

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use dictation_ports::{HistoryStore, HistoryStoreError};

/// The history file at a path.
#[derive(Debug, Clone)]
pub struct HistoryFile {
    path: PathBuf,
    /// Next to the file, so the rename stays on one volume. One fixed name:
    /// a temporary file a crash left behind holds dictated text, and the
    /// next write or delete removes it.
    temporary: PathBuf,
}

impl HistoryFile {
    /// The file at `path` (created on the first write, with its folder).
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".tmp");
        Self {
            temporary: path.with_file_name(name),
            path,
        }
    }

    fn replace(&self, contents: &str) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        remove_if_there(&self.temporary)?;
        write_private(&self.temporary, contents.as_bytes())
            .and_then(|()| fs::rename(&self.temporary, &self.path))
            .inspect_err(|_| {
                let _ = fs::remove_file(&self.temporary);
            })
    }
}

fn failed(error: &io::Error) -> HistoryStoreError {
    HistoryStoreError(error.to_string())
}

fn remove_if_there(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Creates `path`, readable and writable only by this user from the moment
/// it exists, and syncs `bytes` to it.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// A file restored from a backup or copied by hand may be open to others.
#[cfg(unix)]
fn make_private(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode() & 0o777;
    if mode == 0o600 {
        return Ok(());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the same shape as the unix version"
)]
fn make_private(_path: &Path) -> io::Result<()> {
    Ok(())
}

impl HistoryStore for HistoryFile {
    fn read(&self) -> Result<Option<String>, HistoryStoreError> {
        let read = make_private(&self.path).and_then(|()| fs::read_to_string(&self.path));
        match read {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(failed(&error)),
        }
    }

    fn write(&self, contents: &str) -> Result<(), HistoryStoreError> {
        self.replace(contents).map_err(|error| failed(&error))
    }

    fn delete(&self) -> Result<(), HistoryStoreError> {
        remove_if_there(&self.temporary)
            .and_then(|()| remove_if_there(&self.path))
            .map_err(|error| failed(&error))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A fresh folder under the temp dir, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "dictation-history-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn file(&self) -> HistoryFile {
            HistoryFile::new(self.0.join("data").join("history.json"))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn criterion_2_writes_replace_the_contents_and_leave_no_temporary_file() {
        let dir = TempDir::new();
        let file = dir.file();
        assert_eq!(file.read().unwrap(), None);
        file.write("one").unwrap();
        file.write("two").unwrap();
        assert_eq!(file.read().unwrap().as_deref(), Some("two"));
        assert!(!file.temporary.exists());
    }

    #[cfg(unix)]
    #[test]
    fn criterion_4_the_file_has_mode_0600_after_every_write() {
        let dir = TempDir::new();
        let file = dir.file();
        file.write("one").unwrap();
        assert_eq!(mode(&file.path), 0o600);
        file.write("two").unwrap();
        assert_eq!(mode(&file.path), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn criterion_4_a_file_open_to_others_is_made_private_when_read_or_replaced() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let file = dir.file();
        file.write("one").unwrap();
        fs::set_permissions(&file.path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(file.read().unwrap().as_deref(), Some("one"));
        assert_eq!(mode(&file.path), 0o600);
        fs::set_permissions(&file.path, fs::Permissions::from_mode(0o644)).unwrap();
        file.write("two").unwrap();
        assert_eq!(mode(&file.path), 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn criterion_4_a_temporary_file_left_open_to_others_is_not_reused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let file = dir.file();
        file.write("one").unwrap();
        fs::write(&file.temporary, "left by a crash").unwrap();
        fs::set_permissions(&file.temporary, fs::Permissions::from_mode(0o644)).unwrap();
        file.write("two").unwrap();
        assert_eq!(mode(&file.path), 0o600);
        assert_eq!(file.read().unwrap().as_deref(), Some("two"));
    }

    #[test]
    fn criterion_3_delete_removes_the_file_and_any_temporary_one() {
        let dir = TempDir::new();
        let file = dir.file();
        file.delete().unwrap();
        file.write("words").unwrap();
        fs::write(&file.temporary, "left by a crash").unwrap();
        file.delete().unwrap();
        assert!(!file.path.exists());
        assert!(!file.temporary.exists());
        file.delete().unwrap();
    }

    #[test]
    fn a_failed_write_is_an_error_that_holds_no_contents_and_changes_nothing() {
        let dir = TempDir::new();
        // A file where the folder should be.
        fs::write(dir.0.join("data"), "not a folder").unwrap();
        let error = dir.file().write("my private words").unwrap_err();
        assert!(!error.to_string().contains("private"), "{error}");
    }

    #[test]
    fn a_file_that_is_not_text_is_an_error_not_a_panic() {
        let dir = TempDir::new();
        let file = dir.file();
        file.write("x").unwrap();
        fs::write(&file.path, [0xff, 0xfe]).unwrap();
        assert!(file.read().is_err());
    }
}
