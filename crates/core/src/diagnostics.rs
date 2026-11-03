//! Error codes and what to do about them (E1, #209).
//!
//! Every error the user can see has a stable [`ErrorCode`]. One table,
//! [`ErrorCode::info`], maps each code to a plain message, a next step and
//! the [`FixAction`] that does it, so the overlay, the tray and the
//! Troubleshoot window say the same thing.
//!
//! Analytics keeps its own coarse `ErrorType` (`dictation-ports`); a code
//! refines it (several codes share one type) and the mapping is checked in
//! `dictation-ports`' tests, so there is one taxonomy, not two. The code of
//! an error is chosen from its kind, never from free text that could hold a
//! transcript, except for a transcription failure, whose reason is one of
//! the app's own fixed sentences ([`ErrorCode::from_reason`]).

use std::collections::VecDeque;

use serde::Serialize;

use crate::error::{DictationError, Permission};

/// A stable name for a way dictation can fail. The `snake_case` text
/// ([`ErrorCode::as_str`]) is what the user sees and quotes in a report; do
/// not rename one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The microphone permission is refused.
    MicDenied,
    /// The microphone is allowed but failed to record.
    MicFailed,
    /// The Accessibility permission is missing.
    AccessibilityMissing,
    /// The global hotkey doesn't work.
    HotkeyUnregistered,
    /// No API key is saved.
    KeyMissing,
    /// The provider rejected the API key.
    KeyRejected,
    /// The key can't be read from the Keychain.
    KeyUnreadable,
    /// The provider couldn't be reached.
    NetworkUnreachable,
    /// The provider is limiting requests.
    ProviderQuota,
    /// The provider answered with a redirect.
    ProviderRedirect,
    /// The on-device model isn't downloaded.
    ModelMissing,
    /// The on-device model is still loading.
    ModelLoading,
    /// The on-device model couldn't be loaded.
    ModelLoadingFailed,
    /// The engine failed in some other way.
    EngineFailed,
    /// Nothing recognisable was said.
    NoSpeech,
    /// The text couldn't be pasted.
    PasteFailed,
    /// A password field had focus.
    SecureInput,
}

/// What the user can do about an error, in one click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FixAction {
    /// Open System Settings at the Microphone page.
    OpenMicrophoneSettings,
    /// Open System Settings at the Accessibility page.
    OpenAccessibilitySettings,
    /// Open this app's settings.
    OpenSettings,
    /// Open the Troubleshoot window.
    OpenTroubleshoot,
    /// Nothing to click: the next dictation is the retry.
    TryAgain,
}

/// One row of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeInfo {
    /// The code.
    pub code: ErrorCode,
    /// What went wrong, in a few plain words.
    pub message: &'static str,
    /// What to do about it.
    pub next_step: &'static str,
    /// The button that does it.
    pub action: FixAction,
    /// The button's label.
    pub action_label: &'static str,
}

impl FixAction {
    /// The label of the button for this action.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenMicrophoneSettings => "Open Microphone Settings",
            Self::OpenAccessibilitySettings => "Open Accessibility Settings",
            Self::OpenSettings => "Open Settings",
            Self::OpenTroubleshoot => "Troubleshoot",
            Self::TryAgain => "Try again",
        }
    }
}

const fn row(
    code: ErrorCode,
    message: &'static str,
    next_step: &'static str,
    action: FixAction,
) -> CodeInfo {
    CodeInfo {
        code,
        message,
        next_step,
        action,
        action_label: action.label(),
    }
}

/// The longest [`ErrorCode::user_text`], in characters: the overlay shows two
/// lines of about 55 characters.
pub const USER_TEXT_MAX_CHARS: usize = 105;

/// The table: every code once. A code missing here fails
/// `every_code_has_one_row_in_order`.
const TABLE: [CodeInfo; 17] = [
    row(
        ErrorCode::MicDenied,
        "Microphone is off for dictation-app.",
        "Allow it in Privacy & Security, then relaunch.",
        FixAction::OpenMicrophoneSettings,
    ),
    row(
        ErrorCode::MicFailed,
        "The microphone didn't work.",
        "Check that one is connected, then try again.",
        FixAction::TryAgain,
    ),
    row(
        ErrorCode::AccessibilityMissing,
        "Accessibility is off for dictation-app.",
        "Allow it in Privacy & Security, Accessibility.",
        FixAction::OpenAccessibilitySettings,
    ),
    row(
        ErrorCode::HotkeyUnregistered,
        "The hotkey isn't working.",
        "Allow Accessibility; it comes back by itself.",
        FixAction::OpenAccessibilitySettings,
    ),
    row(
        ErrorCode::KeyMissing,
        "No API key is saved.",
        "Add one in Settings, or use a model on this Mac.",
        FixAction::OpenSettings,
    ),
    row(
        ErrorCode::KeyRejected,
        "The provider rejected your API key.",
        "Check or re-enter it in Settings.",
        FixAction::OpenSettings,
    ),
    row(
        ErrorCode::KeyUnreadable,
        "Couldn't read your key from the Keychain.",
        "Unlock it, or enter the key again in Settings.",
        FixAction::OpenSettings,
    ),
    row(
        ErrorCode::NetworkUnreachable,
        "The provider couldn't be reached.",
        "Check your internet, or use a model on this Mac.",
        FixAction::TryAgain,
    ),
    row(
        ErrorCode::ProviderQuota,
        "The provider is limiting requests.",
        "Wait a minute and try again.",
        FixAction::TryAgain,
    ),
    row(
        ErrorCode::ProviderRedirect,
        "The provider sent a redirect.",
        "Check the provider in Settings.",
        FixAction::OpenSettings,
    ),
    row(
        ErrorCode::ModelMissing,
        "The on-device model isn't downloaded.",
        "Download one in Settings.",
        FixAction::OpenSettings,
    ),
    row(
        ErrorCode::ModelLoading,
        "The on-device model is still loading.",
        "Try again in a moment.",
        FixAction::TryAgain,
    ),
    row(
        ErrorCode::ModelLoadingFailed,
        "The on-device model couldn't be loaded.",
        "Pick or download another model in Settings.",
        FixAction::OpenSettings,
    ),
    row(
        ErrorCode::EngineFailed,
        "Transcription failed.",
        "Try again. If it keeps happening, report a problem.",
        FixAction::OpenTroubleshoot,
    ),
    row(
        ErrorCode::NoSpeech,
        "No speech was recognised.",
        "Speak closer to the microphone and try again.",
        FixAction::TryAgain,
    ),
    row(
        ErrorCode::PasteFailed,
        "The text couldn't be pasted.",
        "Check Accessibility, then dictate again.",
        FixAction::OpenAccessibilitySettings,
    ),
    row(
        ErrorCode::SecureInput,
        "A password field is focused.",
        "Click a normal text field and dictate again.",
        FixAction::TryAgain,
    ),
];

impl ErrorCode {
    /// Every code, in the table's order.
    pub const ALL: [Self; 17] = [
        Self::MicDenied,
        Self::MicFailed,
        Self::AccessibilityMissing,
        Self::HotkeyUnregistered,
        Self::KeyMissing,
        Self::KeyRejected,
        Self::KeyUnreadable,
        Self::NetworkUnreachable,
        Self::ProviderQuota,
        Self::ProviderRedirect,
        Self::ModelMissing,
        Self::ModelLoading,
        Self::ModelLoadingFailed,
        Self::EngineFailed,
        Self::NoSpeech,
        Self::PasteFailed,
        Self::SecureInput,
    ];

    /// The stable `snake_case` name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MicDenied => "mic_denied",
            Self::MicFailed => "mic_failed",
            Self::AccessibilityMissing => "accessibility_missing",
            Self::HotkeyUnregistered => "hotkey_unregistered",
            Self::KeyMissing => "key_missing",
            Self::KeyRejected => "key_rejected",
            Self::KeyUnreadable => "key_unreadable",
            Self::NetworkUnreachable => "network_unreachable",
            Self::ProviderQuota => "provider_quota",
            Self::ProviderRedirect => "provider_redirect",
            Self::ModelMissing => "model_missing",
            Self::ModelLoading => "model_loading",
            Self::ModelLoadingFailed => "model_loading_failed",
            Self::EngineFailed => "engine_failed",
            Self::NoSpeech => "no_speech",
            Self::PasteFailed => "paste_failed",
            Self::SecureInput => "secure_input",
        }
    }

    /// This code's row of the table.
    #[must_use]
    pub fn info(self) -> &'static CodeInfo {
        // The variants, `ALL` and `TABLE` are in the same order (a test
        // pins it), so the discriminant is the row.
        &TABLE[self as usize]
    }

    /// What the overlay and the tray show for this code: the table's
    /// message and next step in one short text (at most
    /// [`USER_TEXT_MAX_CHARS`] characters, so it fits the overlay).
    #[must_use]
    pub fn user_text(self) -> String {
        let info = self.info();
        format!("{} {}", info.message, info.next_step)
    }

    /// The code of an error the user saw.
    #[must_use]
    pub fn of(error: &DictationError) -> Self {
        match error {
            DictationError::TranscriptionFailed { reason } => Self::from_reason(reason),
            DictationError::EmptyTranscript => Self::NoSpeech,
            DictationError::InsertFailed { .. } => Self::PasteFailed,
            DictationError::SecureInput => Self::SecureInput,
            DictationError::PermissionMissing(Permission::Microphone) => Self::MicDenied,
            DictationError::PermissionMissing(Permission::Accessibility) => {
                Self::AccessibilityMissing
            }
            DictationError::HotkeyFailed => Self::HotkeyUnregistered,
        }
    }

    /// The code for a transcription failure's `reason`, which is one of the
    /// app's own sentences (`TranscribeError::user_message` in
    /// `dictation-ports`, whose tests check that every variant lands on the
    /// right code). Anything else is [`ErrorCode::EngineFailed`].
    #[must_use]
    pub fn from_reason(reason: &str) -> Self {
        let reason = reason.to_ascii_lowercase();
        // Most specific first.
        let table: [(&str, Self); 12] = [
            ("add your api key", Self::KeyMissing),
            ("still reading your api key", Self::KeyUnreadable),
            ("read your api key from the keychain", Self::KeyUnreadable),
            ("rejected your api key", Self::KeyRejected),
            ("limiting requests", Self::ProviderQuota),
            ("answered with a redirect", Self::ProviderRedirect),
            ("didn't answer in time", Self::NetworkUnreachable),
            ("couldn't reach the provider", Self::NetworkUnreachable),
            ("model isn't downloaded", Self::ModelMissing),
            ("model is still loading", Self::ModelLoading),
            (
                "couldn't load the on-device model",
                Self::ModelLoadingFailed,
            ),
            ("the microphone failed", Self::MicFailed),
        ];
        table
            .iter()
            .find(|(needle, _)| reason.contains(needle))
            .map_or(Self::EngineFailed, |&(_, code)| code)
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An error that happened, for the report: its code and when (seconds since
/// the Unix epoch). Nothing else about it is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoggedError {
    /// What kind of error.
    pub code: ErrorCode,
    /// When, in seconds since the Unix epoch.
    pub at: u64,
}

/// The most recent errors, newest last. Holds codes and times only.
#[derive(Debug, Clone)]
pub struct ErrorLog {
    capacity: usize,
    entries: VecDeque<LoggedError>,
}

/// How many errors the app remembers.
pub const ERROR_LOG_CAPACITY: usize = 10;

impl Default for ErrorLog {
    fn default() -> Self {
        Self::new(ERROR_LOG_CAPACITY)
    }
}

impl ErrorLog {
    /// A log keeping at most `capacity` errors.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::new(),
        }
    }

    /// Remembers `error` as happening at `at`, dropping the oldest if full.
    pub fn record(&mut self, error: &DictationError, at: u64) {
        self.record_code(ErrorCode::of(error), at);
    }

    /// Like [`record`](Self::record), for a code already known.
    pub fn record_code(&mut self, code: ErrorCode, at: u64) {
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(LoggedError { code, at });
    }

    /// The remembered errors, oldest first.
    #[must_use]
    pub fn recent(&self) -> Vec<LoggedError> {
        self.entries.iter().copied().collect()
    }

    /// The newest error, if any.
    #[must_use]
    pub fn last(&self) -> Option<LoggedError> {
        self.entries.back().copied()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_code_has_one_row_in_order() {
        assert_eq!(TABLE.len(), ErrorCode::ALL.len());
        for (code, row) in ErrorCode::ALL.iter().zip(TABLE.iter()) {
            assert_eq!(*code, row.code);
            assert_eq!(code.info().code, *code);
        }
        // `ALL` is complete: a match without a wildcard breaks the build
        // when a variant is added, and this keeps the array in step.
        for code in ErrorCode::ALL {
            let _: () = match code {
                ErrorCode::MicDenied
                | ErrorCode::MicFailed
                | ErrorCode::AccessibilityMissing
                | ErrorCode::HotkeyUnregistered
                | ErrorCode::KeyMissing
                | ErrorCode::KeyRejected
                | ErrorCode::KeyUnreadable
                | ErrorCode::NetworkUnreachable
                | ErrorCode::ProviderQuota
                | ErrorCode::ProviderRedirect
                | ErrorCode::ModelMissing
                | ErrorCode::ModelLoading
                | ErrorCode::ModelLoadingFailed
                | ErrorCode::EngineFailed
                | ErrorCode::NoSpeech
                | ErrorCode::PasteFailed
                | ErrorCode::SecureInput => (),
            };
        }
    }

    #[test]
    fn names_are_unique_snake_case_and_match_the_wire() {
        let mut seen = HashSet::new();
        for code in ErrorCode::ALL {
            let name = code.as_str();
            assert!(seen.insert(name), "{name} twice");
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{name}"
            );
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::String(name.to_owned())
            );
            assert_eq!(code.to_string(), name);
        }
    }

    #[test]
    fn every_code_says_what_to_do() {
        for code in ErrorCode::ALL {
            let info = code.info();
            assert!(info.message.ends_with('.'), "{code}: {}", info.message);
            assert!(info.next_step.ends_with('.'), "{code}: {}", info.next_step);
            assert_eq!(info.action_label, info.action.label(), "{code}");
            assert!(
                code.user_text().chars().count() <= USER_TEXT_MAX_CHARS,
                "{code} is too long for the overlay and tray"
            );
        }
    }

    #[test]
    fn the_examples_from_the_issue_read_well() {
        let info = ErrorCode::MicDenied.info();
        assert_eq!(info.message, "Microphone is off for dictation-app.");
        assert_eq!(info.action, FixAction::OpenMicrophoneSettings);
        assert_eq!(
            ErrorCode::KeyRejected.info().action,
            FixAction::OpenSettings
        );
        assert_eq!(
            ErrorCode::HotkeyUnregistered.info().action,
            FixAction::OpenAccessibilitySettings
        );
    }

    #[test]
    fn each_dictation_error_has_a_code() {
        let cases = [
            (
                DictationError::TranscriptionFailed {
                    reason: "The provider rejected your API key. Check it in Settings.".into(),
                },
                ErrorCode::KeyRejected,
            ),
            (DictationError::EmptyTranscript, ErrorCode::NoSpeech),
            (
                DictationError::InsertFailed { reason: "x".into() },
                ErrorCode::PasteFailed,
            ),
            (DictationError::SecureInput, ErrorCode::SecureInput),
            (
                DictationError::PermissionMissing(Permission::Microphone),
                ErrorCode::MicDenied,
            ),
            (
                DictationError::PermissionMissing(Permission::Accessibility),
                ErrorCode::AccessibilityMissing,
            ),
            (DictationError::HotkeyFailed, ErrorCode::HotkeyUnregistered),
        ];
        for (error, code) in cases {
            assert_eq!(ErrorCode::of(&error), code, "{error:?}");
        }
    }

    #[test]
    fn unknown_reasons_are_engine_failures() {
        assert_eq!(
            ErrorCode::from_reason("Something went wrong: boom."),
            ErrorCode::EngineFailed
        );
        assert_eq!(ErrorCode::from_reason(""), ErrorCode::EngineFailed);
        assert_eq!(
            ErrorCode::from_reason("the microphone failed (no microphone found)"),
            ErrorCode::MicFailed
        );
    }

    #[test]
    fn the_log_keeps_the_newest_codes_only() {
        let mut log = ErrorLog::new(2);
        assert_eq!(log.last(), None);
        log.record(&DictationError::SecureInput, 10);
        log.record(&DictationError::EmptyTranscript, 20);
        log.record(&DictationError::HotkeyFailed, 30);
        assert_eq!(
            log.recent(),
            [
                LoggedError {
                    code: ErrorCode::NoSpeech,
                    at: 20
                },
                LoggedError {
                    code: ErrorCode::HotkeyUnregistered,
                    at: 30
                },
            ]
        );
        assert_eq!(log.last().map(|e| e.at), Some(30));
        assert_eq!(ErrorLog::new(0).capacity, 1);
    }
}
