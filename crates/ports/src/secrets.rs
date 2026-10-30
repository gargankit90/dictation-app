//! API keys in the OS keychain.

use std::fmt;

use thiserror::Error;

/// A secret value, such as an API key. `Debug` never shows the value, so a
/// stray `{:?}` or `tracing` field can't leak it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a secret value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value. Use it only where the secret is needed (an HTTP header),
    /// never in a log or error message.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Why a [`SecretStore`] call failed. Never contains the secret.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SecretError {
    /// The user or OS denied keychain access.
    #[error("keychain access was denied")]
    AccessDenied,
    /// The keychain failed for another reason.
    #[error("keychain error: {0}")]
    Store(String),
}

/// Named secrets in the OS keychain. Keys live nowhere else
/// (`docs/ENGINEERING.md`).
///
/// Threading: callable from any thread. Real implementations may block on
/// the keychain (and on macOS, an access prompt), so the pipeline calls them
/// from a blocking task.
pub trait SecretStore: Send + Sync {
    /// The secret stored under `name`, or `None` if there is none.
    ///
    /// # Errors
    ///
    /// [`SecretError::AccessDenied`] if access is refused, and
    /// [`SecretError::Store`] otherwise.
    ///
    /// Threading: may block on the keychain; call from a blocking task.
    fn get(&self, name: &str) -> Result<Option<Secret>, SecretError>;

    /// Stores `secret` under `name`, replacing any previous value.
    ///
    /// # Errors
    ///
    /// As for [`get`](Self::get); on error the previous value is kept.
    ///
    /// Threading: as for [`get`](Self::get).
    fn set(&self, name: &str, secret: &Secret) -> Result<(), SecretError>;

    /// Removes the secret under `name`. Removing one that doesn't exist
    /// succeeds.
    ///
    /// # Errors
    ///
    /// As for [`get`](Self::get).
    ///
    /// Threading: as for [`get`](Self::get).
    fn delete(&self, name: &str) -> Result<(), SecretError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_the_value() {
        let secret = Secret::new("gsk_live_123");
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert_eq!(secret.expose(), "gsk_live_123");
    }
}
