//! Starting the app when the user logs in.

use thiserror::Error;

/// Whether the OS will start the app at login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchAtLoginState {
    /// The app is registered and allowed to start at login.
    Enabled,
    /// It is not registered.
    Disabled,
    /// It is registered but the user has to allow it in the OS settings
    /// (macOS: System Settings, General, Login Items), for example after
    /// switching it off there. It will not start at login until then.
    RequiresApproval,
}

impl LaunchAtLoginState {
    /// Whether the app will start at login now.
    #[must_use]
    pub fn is_enabled(self) -> bool {
        self == Self::Enabled
    }
}

/// Why a [`LaunchAtLogin`] call failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LaunchAtLoginError {
    /// This OS has no adapter yet.
    #[error("launch at login isn't supported on this system")]
    Unsupported,
    /// The OS refused or failed; the text is its message (never a key or a
    /// transcript).
    #[error("{0}")]
    Os(String),
}

/// Registers the app to start at login, and reports the OS's own state.
///
/// Threading: callable from any thread; the calls are short but may talk to
/// system services, so the shell runs them on the blocking pool.
pub trait LaunchAtLogin: Send + Sync {
    /// What the OS says right now. It can change behind the app's back, when
    /// the user edits their login items.
    ///
    /// # Errors
    ///
    /// [`LaunchAtLoginError::Unsupported`] without an adapter.
    fn state(&self) -> Result<LaunchAtLoginState, LaunchAtLoginError>;

    /// Registers (`true`) or unregisters (`false`) the app. Doing what is
    /// already the case is not an error.
    ///
    /// # Errors
    ///
    /// [`LaunchAtLoginError::Os`] if the OS refuses (macOS: the app is not
    /// running from an app bundle, say), and
    /// [`LaunchAtLoginError::Unsupported`] without an adapter.
    fn set_enabled(&self, enabled: bool) -> Result<(), LaunchAtLoginError>;
}
