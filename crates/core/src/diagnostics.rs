//! Error codes and what to do about them (E1, #209; typed in SIMP1).
//!
//! Every error the user can see has a stable [`ErrorCode`], set where the
//! error happens (`TranscribeError::code` in `dictation-ports`, the pipeline
//! for the microphone and a crash, the coordinator for the rest). One table,
//! [`ErrorCode::info`], maps each code to a plain sentence, the
//! [`FixAction`] that fixes it and its analytics [`ErrorType`], so the
//! overlay, the tray, Troubleshoot and the usage events say the same thing.
//! An [`ErrorType`] is coarser: several codes share one, and only it is sent.

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
    /// The `OpenRouter` account has no credits left (402).
    OutOfCredits,
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

/// What the user can do about an error, in one click: the button in
/// Troubleshoot and the fix row in the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FixAction {
    /// Settings, on Speech (the key and the engine).
    OpenSpeechSettings,
    /// System Settings at the Microphone page.
    OpenMicrophoneSettings,
    /// System Settings at the Accessibility page.
    OpenAccessibilitySettings,
    /// Settings, on the dictation key.
    ChooseAnotherKey,
    /// Settings, on Speech, where the model downloads.
    DownloadModel,
    /// `OpenRouter`'s page for adding credits.
    AddCredits,
    /// The Troubleshoot page.
    Troubleshoot,
    /// Restart the app, so a permission grant takes effect.
    Relaunch,
    /// Nothing to click: the next dictation is the retry. The tray offers
    /// Troubleshoot instead.
    TryAgain,
}

impl FixAction {
    /// The button's label. The tray adds "…" to the ones that open a window.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenSpeechSettings => "Open Speech settings",
            Self::OpenMicrophoneSettings => "Open Microphone settings",
            Self::OpenAccessibilitySettings => "Open Accessibility settings",
            Self::ChooseAnotherKey => "Choose another key",
            Self::DownloadModel => "Download the model",
            Self::AddCredits => "Add credits",
            Self::Troubleshoot => "Troubleshoot",
            Self::Relaunch => "Relaunch Vocet",
            Self::TryAgain => "Try again",
        }
    }

    /// Whether this fix sends the user to System Settings for a grant that
    /// reaches the app only after a relaunch.
    #[must_use]
    pub const fn is_grant(self) -> bool {
        matches!(
            self,
            Self::OpenMicrophoneSettings | Self::OpenAccessibilitySettings
        )
    }
}

/// Why a dictation failed, as the `dictation_failed` usage event says it: a
/// coarse category of [`ErrorCode`], never the reason's text
/// (`docs/privacy.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    /// The engine failed.
    Transcription,
    /// The engine heard no words.
    EmptyTranscript,
    /// The text couldn't be pasted.
    Insert,
    /// A password field had focus.
    SecureInput,
    /// The microphone permission is missing.
    MicrophonePermission,
    /// The Accessibility permission is missing.
    AccessibilityPermission,
    /// The hotkey stopped working.
    Hotkey,
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
    /// The usage events' category.
    pub category: ErrorType,
}

const fn row(
    code: ErrorCode,
    message: &'static str,
    next_step: &'static str,
    action: FixAction,
    category: ErrorType,
) -> CodeInfo {
    CodeInfo {
        code,
        message,
        next_step,
        action,
        category,
    }
}

/// The longest [`ErrorCode::user_text`], in characters: the overlay shows three
/// lines of about 52 characters.
pub const USER_TEXT_MAX_CHARS: usize = 150;

/// The table: every code once, in [`ErrorCode::ALL`]'s order. A code missing
/// here fails `every_code_has_one_row_in_order`.
const TABLE: [CodeInfo; 17] = [
    row(
        ErrorCode::MicDenied,
        "Vocet can't use the microphone.",
        "Allow it in System Settings → Privacy & Security → Microphone, then relaunch Vocet.",
        FixAction::OpenMicrophoneSettings,
        ErrorType::MicrophonePermission,
    ),
    row(
        ErrorCode::MicFailed,
        "The microphone didn't work.",
        "Check that one is connected, then try again.",
        FixAction::TryAgain,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::AccessibilityMissing,
        "Vocet can't paste into other apps yet.",
        "Allow it in System Settings → Privacy & Security → Accessibility.",
        FixAction::OpenAccessibilitySettings,
        ErrorType::AccessibilityPermission,
    ),
    row(
        ErrorCode::HotkeyUnregistered,
        "The dictation key isn't working.",
        "Allow Vocet in System Settings → Privacy & Security → Accessibility; it starts working again by itself.",
        FixAction::OpenAccessibilitySettings,
        ErrorType::Hotkey,
    ),
    row(
        ErrorCode::KeyMissing,
        "No API key saved.",
        "Add one in Settings → Speech, or use the model on this Mac.",
        FixAction::OpenSpeechSettings,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::KeyRejected,
        "Your API key was rejected.",
        "Check the key in Settings → Speech.",
        FixAction::OpenSpeechSettings,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::KeyUnreadable,
        "Vocet couldn't read your saved key.",
        "Unlock your Keychain, or enter the key again in Settings.",
        FixAction::OpenSpeechSettings,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::NetworkUnreachable,
        "Vocet couldn't reach the provider.",
        "Check your internet, or use the model on this Mac.",
        FixAction::TryAgain,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::ProviderQuota,
        "The provider is limiting requests.",
        "Wait a minute and try again.",
        FixAction::TryAgain,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::OutOfCredits,
        "Your OpenRouter credits ran out.",
        "Add credits.",
        FixAction::AddCredits,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::ModelMissing,
        "The model on this Mac isn't downloaded.",
        "Download it in Settings → Speech.",
        FixAction::DownloadModel,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::ModelLoading,
        "The model on this Mac is still loading.",
        "Try again in a moment.",
        FixAction::TryAgain,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::ModelLoadingFailed,
        "The model on this Mac couldn't be loaded.",
        "Download it again in Settings → Speech.",
        FixAction::DownloadModel,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::EngineFailed,
        "Transcription failed.",
        "Try again. If it keeps happening, report a problem.",
        FixAction::Troubleshoot,
        ErrorType::Transcription,
    ),
    row(
        ErrorCode::NoSpeech,
        "No speech was recognised.",
        "Speak closer to the microphone and try again.",
        FixAction::TryAgain,
        ErrorType::EmptyTranscript,
    ),
    row(
        ErrorCode::PasteFailed,
        "The text couldn't be pasted.",
        "It's on the clipboard: press ⌘V. If it keeps happening, check Accessibility.",
        FixAction::OpenAccessibilitySettings,
        ErrorType::Insert,
    ),
    row(
        ErrorCode::SecureInput,
        "A password field is focused.",
        "Click a normal text field and dictate again.",
        FixAction::TryAgain,
        ErrorType::SecureInput,
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
        Self::OutOfCredits,
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
            Self::OutOfCredits => "out_of_credits",
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

    /// What the overlay, the tray and Troubleshoot show for this code: the
    /// table's message and next step in one short text (at most
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
            DictationError::TranscriptionFailed(code) => *code,
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
}

impl From<&DictationError> for ErrorType {
    fn from(error: &DictationError) -> Self {
        ErrorCode::of(error).info().category
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
                | ErrorCode::OutOfCredits
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

    /// U12 criterion 6: one word for the key in every sentence the user reads.
    #[test]
    fn the_sentences_say_dictation_key() {
        for code in ErrorCode::ALL {
            let text = code.user_text().to_lowercase();
            assert!(!text.contains("hotkey"), "{code}: {text}");
            assert!(!text.contains("shortcut"), "{code}: {text}");
        }
    }

    /// SIMP1 criterion 3: every code has a friendly sentence and a fix.
    #[test]
    fn every_code_says_what_to_do() {
        for code in ErrorCode::ALL {
            let info = code.info();
            assert!(info.message.len() > 10, "{code}: {}", info.message);
            assert!(info.message.ends_with('.'), "{code}: {}", info.message);
            assert!(info.next_step.ends_with('.'), "{code}: {}", info.next_step);
            assert!(!info.action.label().is_empty(), "{code}");
            assert!(
                code.user_text().chars().count() <= USER_TEXT_MAX_CHARS,
                "{code} is too long for the overlay and tray"
            );
        }
    }

    /// U1 criterion 5: the sentence a person sees (overlay, tray) names the
    /// fix in their terms, says Vocet, and never shows the code.
    #[test]
    fn every_user_facing_sentence_names_a_fix_and_no_code() {
        // How a next step starts: an instruction a person can follow.
        let verbs = [
            "Allow",
            "Add",
            "Open",
            "Unlock",
            "Check",
            "Download",
            "Pick",
            "Wait",
            "Try",
            "Speak",
            "Click",
            "It's on the clipboard",
        ];
        for code in ErrorCode::ALL {
            let info = code.info();
            let text = code.user_text();
            assert!(
                verbs.iter().any(|verb| info.next_step.starts_with(verb)),
                "{code}: the next step doesn't start with an instruction: {}",
                info.next_step
            );
            assert!(!text.contains(code.as_str()), "{code}: {text}");
            assert!(!text.contains('_'), "{code}: {text}");
            assert!(!text.contains("dictation-app"), "{code}: {text}");
        }
        assert_eq!(
            ErrorCode::KeyRejected.user_text(),
            "Your API key was rejected. Check the key in Settings → Speech."
        );
        assert_eq!(
            ErrorCode::MicDenied.user_text(),
            "Vocet can't use the microphone. Allow it in System Settings → Privacy & Security → Microphone, then relaunch Vocet."
        );
        assert!(
            ErrorCode::KeyMissing
                .user_text()
                .starts_with("No API key saved. Add one in Settings")
        );
    }

    /// U1 round 2: macOS doesn't tell a running app about a microphone grant
    /// (`docs/ENGINEERING.md`), so the microphone's sentence must say to
    /// relaunch; paths use arrows, never commas.
    #[test]
    fn the_microphone_sentence_says_to_relaunch_and_paths_use_arrows() {
        let text = ErrorCode::MicDenied.user_text();
        assert!(text.contains("then relaunch Vocet"), "{text}");
        assert!(text.contains("System Settings → Privacy & Security → Microphone"));
        for code in ErrorCode::ALL {
            let text = code.user_text();
            assert!(!text.contains("Settings, Privacy"), "{code}: {text}");
            assert!(!text.contains("Security, "), "{code}: {text}");
        }
        assert_eq!(
            ErrorCode::HotkeyUnregistered.user_text(),
            "The dictation key isn't working. Allow Vocet in System Settings → Privacy & Security → Accessibility; it starts working again by itself."
        );
    }

    #[test]
    fn the_examples_from_the_issue_read_well() {
        let info = ErrorCode::MicDenied.info();
        assert_eq!(info.message, "Vocet can't use the microphone.");
        assert_eq!(info.action, FixAction::OpenMicrophoneSettings);
        assert_eq!(
            ErrorCode::KeyRejected.info().action,
            FixAction::OpenSpeechSettings
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
                DictationError::TranscriptionFailed(ErrorCode::KeyRejected),
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

    /// The analytics category comes from the same row: one taxonomy.
    #[test]
    fn each_error_type_comes_from_its_code() {
        assert_eq!(
            ErrorType::from(&DictationError::TranscriptionFailed(
                ErrorCode::OutOfCredits
            )),
            ErrorType::Transcription
        );
        assert_eq!(
            ErrorType::from(&DictationError::PermissionMissing(
                Permission::Accessibility
            )),
            ErrorType::AccessibilityPermission
        );
        assert_eq!(
            ErrorType::from(&DictationError::EmptyTranscript),
            ErrorType::EmptyTranscript
        );
        assert_eq!(
            ErrorType::from(&DictationError::InsertFailed { reason: "x".into() }),
            ErrorType::Insert
        );
        assert_eq!(
            serde_json::to_value(ErrorType::MicrophonePermission).unwrap(),
            "microphone_permission"
        );
    }

    #[test]
    fn only_grants_need_a_relaunch() {
        assert!(FixAction::OpenMicrophoneSettings.is_grant());
        assert!(FixAction::OpenAccessibilitySettings.is_grant());
        assert!(!FixAction::OpenSpeechSettings.is_grant());
        assert!(!FixAction::Relaunch.is_grant());
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
