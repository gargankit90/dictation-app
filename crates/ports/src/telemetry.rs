//! Anonymous usage events (A1, `docs/privacy.md`).
//!
//! [`Event`] is a closed enum: every property is an enum, a bucket or a
//! number, so a transcript, a key, a path or an app name has no field to
//! travel in. Adding an event means adding a variant here, which is also
//! where `docs/privacy.md` is checked against.
//!
//! [`Telemetry::record`] never blocks and never fails: a telemetry problem
//! must not reach dictation.

use dictation_core::error::{DictationError, Permission};
use dictation_core::onboarding::Step;
use dictation_core::settings::Provider;
use serde::Serialize;

/// Records usage events.
///
/// Errors: none; a failure to record or send is dropped silently. Threading:
/// any thread; never blocks (it queues, and sending happens elsewhere).
pub trait Telemetry: Send + Sync {
    /// Records `event`, unless the user turned sharing off.
    fn record(&self, event: Event);
}

/// A [`Telemetry`] that records nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTelemetry;

impl Telemetry for NoTelemetry {
    fn record(&self, _event: Event) {}
}

/// Reports errors to the crash and error service (A2).
///
/// What leaves is the E1 error code and the A1 error type, never the error's
/// text. Like [`Telemetry`], it never blocks and never fails, and it is
/// governed by the same sharing switch.
pub trait ErrorReporter: Send + Sync {
    /// Reports `error`, unless the user turned sharing off.
    fn report(&self, error: &DictationError);
}

/// An [`ErrorReporter`] that reports nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoErrorReporter;

impl ErrorReporter for NoErrorReporter {
    fn report(&self, _error: &DictationError) {}
}

/// The longest [`Version`] accepted.
const MAX_VERSION_LEN: usize = 32;

/// An app version such as `0.1.0` or `1.2.3-beta.1`: ASCII digits, letters,
/// dots, dashes and plus signs only, so it can't hold user text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Version(String);

impl Version {
    /// `text` as a version, or `None` if it isn't one (empty, too long, a
    /// character outside the allowed set, or no digit).
    #[must_use]
    pub fn new(text: &str) -> Option<Self> {
        let shaped = !text.is_empty()
            && text.len() <= MAX_VERSION_LEN
            && text.starts_with(|c: char| c.is_ascii_digit())
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
        shaped.then(|| Self(text.to_owned()))
    }

    /// The version text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The CPU family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    /// Apple silicon (arm64).
    AppleSilicon,
    /// Intel (x86-64).
    Intel,
    /// Anything else.
    Other,
}

impl Arch {
    /// This build's architecture.
    #[must_use]
    pub fn current() -> Self {
        match std::env::consts::ARCH {
            "aarch64" => Self::AppleSilicon,
            "x86_64" => Self::Intel,
            _ => Self::Other,
        }
    }
}

/// Where the transcription runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    /// On this computer.
    Local,
    /// At a cloud provider.
    Cloud,
}

/// The engine the user picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "provider", rename_all = "snake_case")]
pub enum EngineChoice {
    /// A cloud provider.
    Cloud(Provider),
    /// A model on this computer (L1). Only the kind is sent, not which model.
    Local,
}

impl EngineChoice {
    /// Where this engine runs.
    #[must_use]
    pub fn kind(self) -> EngineKind {
        match self {
            Self::Cloud(_) => EngineKind::Cloud,
            Self::Local => EngineKind::Local,
        }
    }
}

/// An onboarding step, by the name `docs/privacy.md` lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStep {
    /// What the app does.
    Welcome,
    /// Choose the language and the engine.
    Speech,
    /// Allow the microphone and Accessibility.
    Permissions,
    /// Try a dictation.
    Try,
    /// The summary, and the sharing switch.
    Ready,
}

impl From<Step> for OnboardingStep {
    fn from(step: Step) -> Self {
        match step {
            Step::Welcome => Self::Welcome,
            Step::Speech => Self::Speech,
            Step::Permissions => Self::Permissions,
            Step::Try => Self::Try,
            Step::Ready => Self::Ready,
        }
    }
}

/// How long a recording was, in coarse buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DurationBucket {
    /// Under 2 seconds.
    Under2s,
    /// 2 to under 5 seconds.
    From2To5s,
    /// 5 to under 15 seconds.
    From5To15s,
    /// 15 to under 60 seconds.
    From15To60s,
    /// 60 seconds or more.
    Over60s,
}

impl DurationBucket {
    /// The bucket for `duration`.
    #[must_use]
    pub fn of(duration: std::time::Duration) -> Self {
        match duration.as_secs() {
            0..=1 => Self::Under2s,
            2..=4 => Self::From2To5s,
            5..=14 => Self::From5To15s,
            15..=59 => Self::From15To60s,
            _ => Self::Over60s,
        }
    }
}

/// How long the transcription took, in coarse buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LatencyBucket {
    /// Under half a second.
    Under500ms,
    /// Half a second to under 1 second.
    From500To1000ms,
    /// 1 to under 2 seconds.
    From1To2s,
    /// 2 to under 5 seconds.
    From2To5s,
    /// 5 seconds or more.
    Over5s,
}

impl LatencyBucket {
    /// The bucket for `latency`.
    #[must_use]
    pub fn of(latency: std::time::Duration) -> Self {
        match latency.as_millis() {
            0..=499 => Self::Under500ms,
            500..=999 => Self::From500To1000ms,
            1000..=1999 => Self::From1To2s,
            2000..=4999 => Self::From2To5s,
            _ => Self::Over5s,
        }
    }
}

/// Why a dictation failed, without the reason's text.
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

impl From<&DictationError> for ErrorType {
    fn from(error: &DictationError) -> Self {
        match error {
            DictationError::TranscriptionFailed { .. } => Self::Transcription,
            DictationError::EmptyTranscript => Self::EmptyTranscript,
            DictationError::InsertFailed { .. } => Self::Insert,
            DictationError::SecureInput => Self::SecureInput,
            DictationError::PermissionMissing(Permission::Microphone) => Self::MicrophonePermission,
            DictationError::PermissionMissing(Permission::Accessibility) => {
                Self::AccessibilityPermission
            }
            DictationError::HotkeyFailed => Self::Hotkey,
        }
    }
}

/// A setting, by name. Never its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingName {
    /// The cloud provider.
    Provider,
    /// The model name.
    Model,
    /// The language.
    Language,
    /// The hotkey.
    Hotkey,
    /// Launch at login.
    LaunchAtLogin,
    /// Sharing usage events.
    ShareUsageEvents,
    /// How many recent dictations are kept (never how many that is).
    HistoryLimit,
}

/// What went wrong during first setup, or what the user chose instead of
/// going on (U10). Fixed values only: no text from the OS, a provider, a path
/// or the user has a field to travel in. Each is listed in `docs/privacy.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupProblem {
    /// The model download stopped: not enough free disk space.
    DownloadFailedNoSpace,
    /// The model download stopped: the network or the server failed.
    DownloadFailedNetwork,
    /// The model download stopped: the file failed its checksum.
    DownloadFailedChecksum,
    /// The model download stopped: the file couldn't be written.
    DownloadFailedDisk,
    /// The user cancelled the model download.
    DownloadCancelled,
    /// The downloaded model couldn't be loaded.
    ModelLoadFailed,
    /// The key test found no key to test.
    KeyTestFailedNoKey,
    /// The key test failed: the provider rejected the key, or it can't be one.
    KeyTestFailedRejected,
    /// The key test failed: the provider is rate limiting.
    KeyTestFailedRateLimited,
    /// The key test failed: the provider couldn't be reached.
    KeyTestFailedNetwork,
    /// The key test failed: the provider failed or gave an unusable answer.
    KeyTestFailedServer,
    /// The key test failed: the saved key couldn't be read from the keychain.
    KeyTestFailedKeychain,
    /// macOS reports the microphone as refused.
    MicrophoneDenied,
    /// Accessibility was asked for and still wasn't on when setup was closed.
    /// macOS doesn't report a refusal of this one.
    AccessibilityDenied,
    /// macOS's microphone prompt or its settings page couldn't be opened.
    MicrophoneRequestFailed,
    /// macOS's Accessibility prompt or its settings page couldn't be opened.
    AccessibilityRequestFailed,
    /// The user relaunched the app because a permission hadn't taken effect.
    RelaunchNeeded,
    /// The practice dictation's words didn't reach the practice box.
    PasteMissed,
    /// The user skipped the practice dictation.
    PracticeSkipped,
}

impl SetupProblem {
    /// Every value, in the order `docs/privacy.md` lists them.
    pub const ALL: [Self; 19] = [
        Self::DownloadFailedNoSpace,
        Self::DownloadFailedNetwork,
        Self::DownloadFailedChecksum,
        Self::DownloadFailedDisk,
        Self::DownloadCancelled,
        Self::ModelLoadFailed,
        Self::KeyTestFailedNoKey,
        Self::KeyTestFailedRejected,
        Self::KeyTestFailedRateLimited,
        Self::KeyTestFailedNetwork,
        Self::KeyTestFailedServer,
        Self::KeyTestFailedKeychain,
        Self::MicrophoneDenied,
        Self::AccessibilityDenied,
        Self::MicrophoneRequestFailed,
        Self::AccessibilityRequestFailed,
        Self::RelaunchNeeded,
        Self::PasteMissed,
        Self::PracticeSkipped,
    ];
}

/// What can be recorded. Serialises as `{"event": name, "properties": {…}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", content = "properties", rename_all = "snake_case")]
pub enum Event {
    /// The app started.
    AppOpened {
        /// The app version.
        version: Version,
        /// The macOS major version, e.g. 15.
        macos_major: u16,
        /// The CPU family.
        arch: Arch,
    },
    /// The user got to an onboarding step.
    OnboardingStepReached {
        /// The step.
        step: OnboardingStep,
    },
    /// The user finished onboarding.
    OnboardingCompleted {
        /// The last step they were on.
        step: OnboardingStep,
    },
    /// Setup's window was closed, or the app quit, with setup not finished.
    OnboardingLeft {
        /// The step they stopped on.
        step: OnboardingStep,
    },
    /// Something got in the way during setup (U10).
    SetupProblem {
        /// Which fixed reason.
        reason: SetupProblem,
    },
    /// The user picked or changed the engine.
    EngineSelected {
        /// Local or cloud, and which provider.
        #[serde(flatten)]
        engine: EngineChoice,
    },
    /// A dictation was pasted.
    DictationFinished {
        /// Where it ran.
        engine_kind: EngineKind,
        /// How long the recording was.
        duration_bucket: DurationBucket,
        /// How long the transcription took.
        latency_bucket: LatencyBucket,
    },
    /// A dictation failed.
    DictationFailed {
        /// What kind of failure.
        error_type: ErrorType,
    },
    /// A setting changed.
    SettingChanged {
        /// Which one (not its value).
        setting: SettingName,
    },
    /// The app was updated.
    UpdateInstalled {
        /// The version before.
        from_version: Version,
        /// The version now.
        to_version: Version,
    },
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;

    /// E1: an error code refines A1's `ErrorType`; it isn't a second
    /// taxonomy. Every code sits under exactly one type, and an error's
    /// code and type agree.
    #[test]
    fn error_codes_refine_error_types() {
        use dictation_core::diagnostics::ErrorCode;

        fn coarse(code: ErrorCode) -> ErrorType {
            match code {
                ErrorCode::MicDenied => ErrorType::MicrophonePermission,
                ErrorCode::AccessibilityMissing => ErrorType::AccessibilityPermission,
                ErrorCode::HotkeyUnregistered => ErrorType::Hotkey,
                ErrorCode::NoSpeech => ErrorType::EmptyTranscript,
                ErrorCode::PasteFailed => ErrorType::Insert,
                ErrorCode::SecureInput => ErrorType::SecureInput,
                ErrorCode::MicFailed
                | ErrorCode::KeyMissing
                | ErrorCode::KeyRejected
                | ErrorCode::KeyUnreadable
                | ErrorCode::NetworkUnreachable
                | ErrorCode::ProviderQuota
                | ErrorCode::ProviderRedirect
                | ErrorCode::ModelMissing
                | ErrorCode::ModelLoading
                | ErrorCode::ModelLoadingFailed
                | ErrorCode::EngineFailed => ErrorType::Transcription,
            }
        }
        let errors = [
            DictationError::TranscriptionFailed {
                reason: "The provider rejected your API key. Check it in Settings.".into(),
            },
            DictationError::TranscriptionFailed {
                reason: "the microphone failed (no microphone found)".into(),
            },
            DictationError::EmptyTranscript,
            DictationError::InsertFailed { reason: "x".into() },
            DictationError::SecureInput,
            DictationError::PermissionMissing(Permission::Microphone),
            DictationError::PermissionMissing(Permission::Accessibility),
            DictationError::HotkeyFailed,
        ];
        for error in errors {
            assert_eq!(
                coarse(ErrorCode::of(&error)),
                ErrorType::from(&error),
                "{error:?}"
            );
        }
    }

    fn wire(event: &Event) -> serde_json::Value {
        serde_json::to_value(event).expect("events serialise")
    }

    #[test]
    fn versions_hold_no_free_text() {
        assert!(Version::new("0.1.0").is_some());
        assert!(Version::new("1.2.3-beta.1+7").is_some());
        for bad in ["", "v1", "1 0", "1.0/etc", "1.0\n", &"1".repeat(33), "ß1"] {
            assert!(Version::new(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn every_event_has_its_documented_shape() {
        let v = |t| Version::new(t).expect("version");
        let cases = [
            (
                Event::AppOpened {
                    version: v("0.1.0"),
                    macos_major: 15,
                    arch: Arch::AppleSilicon,
                },
                json!({"event":"app_opened","properties":{"version":"0.1.0","macos_major":15,"arch":"apple_silicon"}}),
            ),
            (
                Event::OnboardingStepReached {
                    step: OnboardingStep::Speech,
                },
                json!({"event":"onboarding_step_reached","properties":{"step":"speech"}}),
            ),
            (
                Event::OnboardingCompleted {
                    step: OnboardingStep::Ready,
                },
                json!({"event":"onboarding_completed","properties":{"step":"ready"}}),
            ),
            (
                Event::OnboardingLeft {
                    step: OnboardingStep::Permissions,
                },
                json!({"event":"onboarding_left","properties":{"step":"permissions"}}),
            ),
            (
                Event::SetupProblem {
                    reason: SetupProblem::MicrophoneDenied,
                },
                json!({"event":"setup_problem","properties":{"reason":"microphone_denied"}}),
            ),
            (
                Event::EngineSelected {
                    engine: EngineChoice::Cloud(Provider::OpenAi),
                },
                json!({"event":"engine_selected","properties":{"kind":"cloud","provider":"openai"}}),
            ),
            (
                Event::EngineSelected {
                    engine: EngineChoice::Local,
                },
                json!({"event":"engine_selected","properties":{"kind":"local"}}),
            ),
            (
                Event::DictationFinished {
                    engine_kind: EngineKind::Cloud,
                    duration_bucket: DurationBucket::From2To5s,
                    latency_bucket: LatencyBucket::Under500ms,
                },
                json!({"event":"dictation_finished","properties":{"engine_kind":"cloud","duration_bucket":"from2_to5s","latency_bucket":"under500ms"}}),
            ),
            (
                Event::DictationFailed {
                    error_type: ErrorType::SecureInput,
                },
                json!({"event":"dictation_failed","properties":{"error_type":"secure_input"}}),
            ),
            (
                Event::SettingChanged {
                    setting: SettingName::LaunchAtLogin,
                },
                json!({"event":"setting_changed","properties":{"setting":"launch_at_login"}}),
            ),
            (
                Event::UpdateInstalled {
                    from_version: v("0.1.0"),
                    to_version: v("0.2.0"),
                },
                json!({"event":"update_installed","properties":{"from_version":"0.1.0","to_version":"0.2.0"}}),
            ),
        ];
        for (event, expected) in cases {
            assert_eq!(wire(&event), expected);
        }
    }

    /// U10 criterion 4: the page lists every value, and none can hold free text.
    #[test]
    fn every_setup_problem_is_a_plain_word_and_on_the_privacy_page() {
        let page = include_str!("../../../docs/privacy.md");
        let table = page
            .split("## Setup problems")
            .nth(1)
            .expect("the page has the section");
        let mut seen = Vec::new();
        for problem in SetupProblem::ALL {
            let event = wire(&Event::SetupProblem { reason: problem });
            let properties = event["properties"].as_object().expect("properties");
            assert_eq!(properties.len(), 1, "only the reason: {event}");
            let reason = properties["reason"].as_str().expect("a word").to_owned();
            assert!(
                reason.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{reason}"
            );
            assert!(
                table.contains(&format!("`{reason}`")),
                "{reason} isn't listed"
            );
            assert!(!seen.contains(&reason), "{reason} twice");
            seen.push(reason);
        }
    }

    /// Adding a value breaks this match, which says to add it to `ALL`.
    #[test]
    fn the_list_of_setup_problems_is_complete() {
        for problem in SetupProblem::ALL {
            match problem {
                SetupProblem::DownloadFailedNoSpace
                | SetupProblem::DownloadFailedNetwork
                | SetupProblem::DownloadFailedChecksum
                | SetupProblem::DownloadFailedDisk
                | SetupProblem::DownloadCancelled
                | SetupProblem::ModelLoadFailed
                | SetupProblem::KeyTestFailedNoKey
                | SetupProblem::KeyTestFailedRejected
                | SetupProblem::KeyTestFailedRateLimited
                | SetupProblem::KeyTestFailedNetwork
                | SetupProblem::KeyTestFailedServer
                | SetupProblem::KeyTestFailedKeychain
                | SetupProblem::MicrophoneDenied
                | SetupProblem::AccessibilityDenied
                | SetupProblem::MicrophoneRequestFailed
                | SetupProblem::AccessibilityRequestFailed
                | SetupProblem::RelaunchNeeded
                | SetupProblem::PasteMissed
                | SetupProblem::PracticeSkipped => {}
            }
        }
    }

    #[test]
    fn buckets_split_at_their_edges() {
        let secs = Duration::from_secs;
        let ms = Duration::from_millis;
        assert_eq!(DurationBucket::of(ms(1999)), DurationBucket::Under2s);
        assert_eq!(DurationBucket::of(secs(2)), DurationBucket::From2To5s);
        assert_eq!(DurationBucket::of(secs(5)), DurationBucket::From5To15s);
        assert_eq!(DurationBucket::of(secs(15)), DurationBucket::From15To60s);
        assert_eq!(DurationBucket::of(secs(60)), DurationBucket::Over60s);
        assert_eq!(LatencyBucket::of(ms(499)), LatencyBucket::Under500ms);
        assert_eq!(LatencyBucket::of(ms(500)), LatencyBucket::From500To1000ms);
        assert_eq!(LatencyBucket::of(ms(1000)), LatencyBucket::From1To2s);
        assert_eq!(LatencyBucket::of(ms(2000)), LatencyBucket::From2To5s);
        assert_eq!(LatencyBucket::of(ms(5000)), LatencyBucket::Over5s);
    }

    #[test]
    fn an_error_type_ignores_the_reason() {
        let error = DictationError::TranscriptionFailed {
            reason: "my secret words".into(),
        };
        assert_eq!(ErrorType::from(&error), ErrorType::Transcription);
        assert_eq!(
            ErrorType::from(&DictationError::PermissionMissing(
                Permission::Accessibility
            )),
            ErrorType::AccessibilityPermission
        );
    }
}
