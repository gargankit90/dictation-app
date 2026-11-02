//! Errors the coordinator reports to the user.

use std::fmt;

use thiserror::Error;

/// A permission the app needs from the OS before it can dictate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Recording from the microphone.
    Microphone,
    /// Pasting into other apps (macOS Accessibility).
    Accessibility,
}

/// Why a dictation failed. The `Display` text is shown to the user and
/// logged at info level, so it never contains the transcript or an API key:
/// a `reason` is cut to one short line with any key hidden
/// ([`crate::redact::summary`]).
#[derive(Clone, PartialEq, Eq, Error)]
pub enum DictationError {
    /// The engine returned an error; `reason` is a short, user-safe summary.
    #[error("Transcription failed: {}", crate::redact::summary(.reason))]
    TranscriptionFailed {
        /// What went wrong, without the audio or any key.
        reason: String,
    },
    /// The engine heard nothing it could turn into text.
    #[error("No speech was recognised.")]
    EmptyTranscript,
    /// The text could not be pasted into the focused app.
    #[error("Could not insert the text: {}", crate::redact::summary(.reason))]
    InsertFailed {
        /// What went wrong, without the transcript.
        reason: String,
    },
    /// A password field has focus (macOS Secure Input), so the text was
    /// not pasted.
    #[error("A password field is focused; dictation wasn't pasted.")]
    SecureInput,
    /// The OS has not granted a permission dictation needs. The microphone's
    /// message says where to allow it and that the app must be reopened
    /// then: macOS doesn't tell a running app about a grant made in System
    /// Settings after a refusal (#126, `docs/ENGINEERING.md`).
    #[error("{}", permission_message(*.0))]
    PermissionMissing(Permission),
    /// The global hotkey doesn't work (macOS: often because Accessibility
    /// is off). The app keeps trying and says when it works again.
    #[error(
        "The dictation hotkey isn't working. Allow dictation-app in System Settings, Privacy & Security, Accessibility; it comes back by itself."
    )]
    HotkeyFailed,
}

/// Like the derived `Debug`, but a `reason` shows as its
/// [`crate::redact::summary`], so logging `?error` can't leak a key.
impl fmt::Debug for DictationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TranscriptionFailed { reason } => f
                .debug_struct("TranscriptionFailed")
                .field("reason", &crate::redact::summary(reason))
                .finish(),
            Self::InsertFailed { reason } => f
                .debug_struct("InsertFailed")
                .field("reason", &crate::redact::summary(reason))
                .finish(),
            Self::EmptyTranscript => f.write_str("EmptyTranscript"),
            Self::SecureInput => f.write_str("SecureInput"),
            Self::PermissionMissing(permission) => f
                .debug_tuple("PermissionMissing")
                .field(permission)
                .finish(),
            Self::HotkeyFailed => f.write_str("HotkeyFailed"),
        }
    }
}

fn permission_message(permission: Permission) -> &'static str {
    match permission {
        Permission::Microphone => {
            "Microphone permission is missing. Allow it in System Settings, Privacy & Security, Microphone, then reopen the app."
        }
        Permission::Accessibility => "Accessibility permission is missing.",
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
                "Microphone permission is missing. Allow it in System Settings, Privacy & Security, Microphone, then reopen the app.",
            ),
            (
                DictationError::PermissionMissing(Permission::Accessibility),
                "Accessibility permission is missing.",
            ),
            (
                DictationError::HotkeyFailed,
                "The dictation hotkey isn't working. Allow dictation-app in System Settings, Privacy & Security, Accessibility; it comes back by itself.",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn a_reason_is_cut_to_one_line_without_keys() {
        let key = "sk-proj-NOTAREALKEY0123456789abcdef";
        let long = format!("{key}\n{}", "x ".repeat(300));
        for error in [
            DictationError::TranscriptionFailed {
                reason: long.clone(),
            },
            DictationError::InsertFailed { reason: long },
        ] {
            let text = error.to_string();
            assert!(!text.contains(key), "{text}");
            assert!(!text.contains('\n'), "{text}");
            assert!(text.chars().count() < 260, "{text}");
        }
    }
}
