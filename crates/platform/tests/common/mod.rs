//! Scratch directories for the single-instance tests: unique names, removed
//! when the test ends (pass or fail). Pids are reused, so the name is not
//! the pid alone: a leftover from an earlier process must not be met.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A directory name under the temp dir, removed on drop. The directory is
/// not created: a first launch makes it, and some tests want it missing.
pub struct Scratch(PathBuf);

impl Scratch {
    /// `tag` should be short: socket paths are limited to about 100 bytes
    /// and the temp dir on macOS already takes about 50.
    pub fn new(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.subsec_nanos());
        let path =
            std::env::temp_dir().join(format!("si-{tag}-{}-{n}-{nanos:x}", std::process::id()));
        // Whatever sits there (a file, say) is not ours.
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::remove_file(&path);
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
