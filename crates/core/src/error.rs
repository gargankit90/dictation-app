//! Errors the coordinator reports to the user.

use thiserror::Error;

/// A permission the app needs from the OS before it can dictate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Recording from the microphone.
    Microphone,
    /// Pasting into other apps (macOS Accessibility).
    Accessibility,
}

/// Why a dictation failed. The `Display` text is shown to the user, so it
/// never contains the transcript or an API key.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DictationError {
    /// The engine returned an error; `reason` is a short, user-safe summary.
    #[error("Transcription failed: {reason}")]
    TranscriptionFailed {
        /// What went wrong, without the audio or any key.
        reason: String,
    },
    /// The engine heard nothing it could turn into text.
    #[error("No speech was recognised.")]
    EmptyTranscript,
    /// The text could not be pasted into the focused app.
    #[error("Could not insert the text: {reason}")]
    InsertFailed {
        /// What went wrong, without the transcript.
        reason: String,
    },
    /// A password field has focus (macOS Secure Input), so the text was
    /// not pasted.
    #[error("A password field is focused; dictation wasn't pasted.")]
    SecureInput,
    /// The OS has not granted a permission dictation needs.
    #[error("{} permission is missing.", permission_name(*.0))]
    PermissionMissing(Permission),
    /// The global hotkey stopped working (macOS: often because
    /// Accessibility was turned off).
    #[error(
        "The dictation hotkey stopped working. Check the Accessibility permission, then restart the app."
    )]
    HotkeyFailed,
}

fn permission_name(permission: Permission) -> &'static str {
    match permission {
        Permission::Microphone => "Microphone",
        Permission::Accessibility => "Accessibility",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_user_facing() {
        let cases = [
            (
                DictationError::TranscriptionFailed {
                    reason: "timed out".into(),
                },
                "Transcription failed: timed out",
            ),
            (DictationError::EmptyTranscript, "No speech was recognised."),
            (
                DictationError::InsertFailed {
                    reason: "no focused app".into(),
                },
                "Could not insert the text: no focused app",
            ),
            (
                DictationError::SecureInput,
                "A password field is focused; dictation wasn't pasted.",
            ),
            (
                DictationError::PermissionMissing(Permission::Microphone),
                "Microphone permission is missing.",
            ),
            (
                DictationError::PermissionMissing(Permission::Accessibility),
                "Accessibility permission is missing.",
            ),
            (
                DictationError::HotkeyFailed,
                "The dictation hotkey stopped working. Check the Accessibility permission, then restart the app.",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }
}
