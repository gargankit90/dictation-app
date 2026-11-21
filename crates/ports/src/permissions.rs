//! OS permissions dictation needs.

use thiserror::Error;

pub use dictation_core::error::PermissionStatus;

use crate::Permission;

/// Why a [`Permissions::request`] failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PermissionError {
    /// The prompt couldn't be shown.
    #[error("could not request permission: {0}")]
    RequestFailed(String),
}

/// Checks and requests OS permissions.
///
/// Threading: callable from any thread. `check` never blocks; `request` may
/// show a system prompt and returns without waiting for the user's answer.
pub trait Permissions: Send + Sync {
    /// The current status of `permission`.
    ///
    /// Errors: none; if the OS can't say, implementations report
    /// [`PermissionStatus::NotDetermined`]. Threading: any thread; never
    /// blocks.
    fn check(&self, permission: Permission) -> PermissionStatus;

    /// Asks the OS to prompt the user for `permission` (or opens System
    /// Settings where it can't prompt). Call [`check`](Self::check) later
    /// for the answer.
    ///
    /// # Errors
    ///
    /// [`PermissionError::RequestFailed`] if the prompt couldn't be shown.
    ///
    /// Threading: any thread; returns without waiting for the user.
    fn request(&self, permission: Permission) -> Result<(), PermissionError>;
}
