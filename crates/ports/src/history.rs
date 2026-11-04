//! Where the recent dictations are kept (U7).

use thiserror::Error;

/// Why the history file couldn't be read or changed. Holds the OS's reason,
/// never any of the file's contents.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the history file: {0}")]
pub struct HistoryStoreError(pub String);

/// The history file: readable only by this user, replaced in one step.
///
/// Threading: callable from any thread, but each call does file I/O, so call
/// from a blocking task or thread. Implementations must never log the
/// contents or their length.
pub trait HistoryStore: Send + Sync {
    /// What the file holds, or `None` if there is no file.
    ///
    /// # Errors
    ///
    /// If the file exists but can't be read.
    fn read(&self) -> Result<Option<String>, HistoryStoreError>;

    /// Replaces the file's contents with `contents`.
    ///
    /// # Errors
    ///
    /// If it can't be written; the file then holds what it held before.
    fn write(&self, contents: &str) -> Result<(), HistoryStoreError>;

    /// Removes the file. No file is not an error.
    ///
    /// # Errors
    ///
    /// If it exists and can't be removed.
    fn delete(&self) -> Result<(), HistoryStoreError>;
}
