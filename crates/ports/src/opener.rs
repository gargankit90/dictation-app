//! Opening an address in the browser and a folder in the file manager.

use std::path::Path;

/// Launches other programs for the user: the browser and the file manager.
///
/// This is a port so that no test can open a real browser tab or Finder
/// window by accident (F1): tests use a recording fake. Both methods may
/// block briefly while the launcher starts.
pub trait Opener: Send + Sync {
    /// Opens `url` in the default browser. Only `https` addresses are opened.
    ///
    /// # Errors
    ///
    /// A message for the log if the address is refused or the launcher
    /// can't run.
    fn open_url(&self, url: &str) -> Result<(), String>;

    /// Shows `dir` in the file manager.
    ///
    /// # Errors
    ///
    /// A message for the log if the launcher can't run.
    fn reveal_folder(&self, dir: &Path) -> Result<(), String>;
}
