//! The user's settings: the model, defaults, validation and the on-disk JSON
//! format with its schema version and migrations.
//!
//! Pure: [`from_json`] and [`to_json`] turn text into settings and back; the
//! app reads and writes the file. The API key is never part of the
//! settings: it lives in the OS keychain (`docs/ENGINEERING.md`).
//!
//! The file is `{"version": 2, …}`. There are no migrations: an older (or
//! unversioned) file is read as the current version. The cloud engine is
//! always `OpenRouter` with [`crate::cloud_models::SPEECH_MODEL`], and the
//! on-device one [`crate::local_models::DEFAULT_MODEL`], so neither is a
//! setting.
//!
//! Fields this version doesn't know (from a newer build, or a hand edit) are
//! ignored, and missing ones take their defaults, so an old or partial file
//! still loads. A file from a *newer* schema version is refused rather than
//! guessed at.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::hotkey_spec::HotkeySpec;

/// The schema version this build reads and writes.
pub const CURRENT_VERSION: u32 = 2;

/// The name the user's `OpenRouter` API key is stored under in the
/// keychain.
pub const API_KEY_NAME: &str = "openrouter-api-key";

/// How many recent dictations the user can choose to keep; 0 is Off (U7).
pub const HISTORY_LIMITS: [u32; 5] = [0, 10, 25, 50, 100];

/// How many recent dictations are kept unless the user chooses otherwise.
pub const DEFAULT_HISTORY_LIMIT: u32 = 25;

/// Where speech becomes text (L1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Transcription {
    /// `OpenRouter` with the user's own API key.
    #[default]
    Cloud,
    /// The downloaded model on this Mac; no key.
    Local,
}

/// What the user can set. Never holds the API key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct Settings {
    /// The schema version; always [`CURRENT_VERSION`] once loaded.
    pub version: u32,
    /// The dictation hotkey, as [`HotkeySpec`] writes it (`RightAlt`).
    pub hotkey: String,
    /// Whether the app starts when the user logs in (M5.3). The file keeps
    /// the last value applied; what the settings screen shows is the OS's
    /// own state, which the user can change in System Settings.
    pub launch_at_login: bool,
    /// Cloud provider or on-device model (L1).
    pub transcription: Transcription,
    /// Whether anonymous usage events are shared (A1, `docs/privacy.md`).
    /// On by default; a file without it loads with `true`.
    pub share_usage_events: bool,
    /// How many recent dictations are kept, one of [`HISTORY_LIMITS`] (U7).
    /// A file without it loads with [`DEFAULT_HISTORY_LIMIT`].
    pub history_limit: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            hotkey: HotkeySpec::default().to_string(),
            launch_at_login: false,
            transcription: Transcription::default(),
            share_usage_events: true,
            history_limit: DEFAULT_HISTORY_LIMIT,
        }
    }
}

/// Why settings couldn't be loaded or saved.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SettingsError {
    /// The text isn't a settings object (bad JSON, or a field of the wrong
    /// type); the parser's message says where.
    #[error("the settings file is malformed: {0}")]
    Malformed(String),
    /// The file was written by a newer build.
    #[error(
        "the settings file is from a newer version ({0}); this build reads up to {CURRENT_VERSION}"
    )]
    TooNew(u64),
    /// A field has a value the app can't use.
    #[error("invalid {field}: {reason}")]
    Invalid {
        /// Which field.
        field: &'static str,
        /// What's wrong with it, for the user.
        reason: String,
    },
}

impl Settings {
    /// Checks every field.
    ///
    /// # Errors
    ///
    /// [`SettingsError::Invalid`] for the first field that's wrong:
    /// `version` other than [`CURRENT_VERSION`], a `hotkey` that doesn't
    /// parse, or a `history_limit` not in [`HISTORY_LIMITS`].
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.version != CURRENT_VERSION {
            return Err(invalid(
                "version",
                format!("expected {CURRENT_VERSION}, got {}", self.version),
            ));
        }
        if let Err(error) = self.hotkey.parse::<HotkeySpec>() {
            return Err(invalid("hotkey", error.to_string()));
        }
        if !HISTORY_LIMITS.contains(&self.history_limit) {
            return Err(invalid(
                "history_limit",
                format!("{} is not one of 0, 10, 25, 50, 100", self.history_limit),
            ));
        }
        Ok(())
    }
}

fn invalid(field: &'static str, reason: String) -> SettingsError {
    SettingsError::Invalid { field, reason }
}

/// Reads settings from the file's JSON, migrating older versions, filling
/// in missing fields with their defaults and ignoring unknown ones.
///
/// # Errors
///
/// [`SettingsError::Malformed`] if it isn't a JSON object of the right
/// shape, [`SettingsError::TooNew`] for a newer schema version, and
/// [`SettingsError::Invalid`] if a value fails [`Settings::validate`].
pub fn from_json(json: &str) -> Result<Settings, SettingsError> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| SettingsError::Malformed(e.to_string()))?;
    let Value::Object(mut object) = value else {
        return Err(SettingsError::Malformed("not a JSON object".to_owned()));
    };
    let version = match object.get("version") {
        None => u64::from(CURRENT_VERSION),
        Some(version) => version
            .as_u64()
            .ok_or_else(|| SettingsError::Malformed("version is not a number".to_owned()))?,
    };
    if version > u64::from(CURRENT_VERSION) {
        return Err(SettingsError::TooNew(version));
    }
    // An older file is read as this version: there are no migrations.
    object.insert("version".to_owned(), Value::from(CURRENT_VERSION));
    let settings: Settings = serde_json::from_value(Value::Object(object))
        .map_err(|e| SettingsError::Malformed(e.to_string()))?;
    settings.validate()?;
    Ok(settings)
}

/// The settings as pretty-printed JSON for the file.
///
/// # Errors
///
/// [`SettingsError::Invalid`] if they fail [`Settings::validate`], so an
/// invalid file is never written.
pub fn to_json(settings: &Settings) -> Result<String, SettingsError> {
    settings.validate()?;
    serde_json::to_string_pretty(settings).map_err(|e| SettingsError::Malformed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn criterion_1_defaults_are_valid() {
        let settings = Settings::default();
        assert_eq!(settings.version, CURRENT_VERSION);
        assert_eq!(settings.hotkey, "RightAlt");
        settings.validate().unwrap();
    }

    #[test]
    fn criterion_1_an_empty_object_loads_as_the_defaults() {
        // No version is the current one; every field is missing, so all default.
        assert_eq!(from_json("{}").unwrap(), Settings::default());
        assert_eq!(from_json(r#"{"version": 2}"#).unwrap(), Settings::default());
    }

    #[test]
    fn criterion_1_round_trips_through_json() {
        let settings = Settings {
            hotkey: "Ctrl+Shift+Space".into(),
            ..Settings::default()
        };
        let json = to_json(&settings).unwrap();
        assert_eq!(from_json(&json).unwrap(), settings);
    }

    #[test]
    fn sharing_usage_events_defaults_to_on_and_round_trips() {
        assert!(Settings::default().share_usage_events);
        let old = from_json(r#"{"version": 2, "provider": "groq"}"#).unwrap();
        assert!(old.share_usage_events, "a file without it shares");
        let off = Settings {
            share_usage_events: false,
            ..Settings::default()
        };
        assert_eq!(from_json(&to_json(&off).unwrap()).unwrap(), off);
    }

    /// U7 criterion 1: new installs and files from before U7 get 25.
    #[test]
    fn u7_criterion_1_the_history_limit_defaults_to_25_for_old_files() {
        assert_eq!(DEFAULT_HISTORY_LIMIT, 25);
        assert_eq!(Settings::default().history_limit, 25);
        let old = r#"{"version": 2, "provider": "openai", "model": "whisper-1"}"#;
        assert_eq!(from_json(old).unwrap().history_limit, 25);
    }

    #[test]
    fn u7_criterion_1_every_allowed_limit_round_trips_and_others_are_rejected() {
        for limit in HISTORY_LIMITS {
            let settings = Settings {
                history_limit: limit,
                ..Settings::default()
            };
            let json = to_json(&settings).unwrap();
            assert_eq!(from_json(&json).unwrap(), settings);
        }
        for limit in [1, 9, 11, 24, 26, 101, u32::MAX] {
            let settings = Settings {
                history_limit: limit,
                ..Settings::default()
            };
            assert!(
                matches!(
                    settings.validate(),
                    Err(SettingsError::Invalid {
                        field: "history_limit",
                        ..
                    })
                ),
                "{limit}"
            );
            assert!(to_json(&settings).is_err(), "{limit}: saved anyway");
        }
        assert!(matches!(
            from_json(r#"{"version": 2, "history_limit": 7}"#),
            Err(SettingsError::Invalid {
                field: "history_limit",
                ..
            })
        ));
        assert!(matches!(
            from_json(r#"{"version": 2, "history_limit": "25"}"#),
            Err(SettingsError::Malformed(_))
        ));
    }

    #[test]
    fn launch_at_login_defaults_to_off_and_round_trips() {
        assert!(!Settings::default().launch_at_login);
        // A file from before M5.3 has no such field.
        assert!(!from_json(r#"{"version": 2}"#).unwrap().launch_at_login);
        let on = Settings {
            launch_at_login: true,
            ..Settings::default()
        };
        let json = to_json(&on).unwrap();
        assert!(json.contains(r#""launch_at_login": true"#), "{json}");
        assert_eq!(from_json(&json).unwrap(), on);
        assert!(matches!(
            from_json(r#"{"launch_at_login": "yes"}"#),
            Err(SettingsError::Malformed(_))
        ));
    }

    #[test]
    fn l1_criterion_4_transcription_defaults_to_cloud() {
        assert_eq!(Settings::default().transcription, Transcription::Cloud);
        let old = from_json(r#"{"version": 2, "provider": "openai", "model": "whisper-1"}"#);
        assert_eq!(old.unwrap().transcription, Transcription::Cloud);
    }

    #[test]
    fn l1_criterion_4_the_local_choice_round_trips() {
        let local = Settings {
            transcription: Transcription::Local,
            ..Settings::default()
        };
        let json = to_json(&local).unwrap();
        assert!(json.contains(r#""transcription": "local""#), "{json}");
        assert_eq!(from_json(&json).unwrap(), local);
        assert!(matches!(
            from_json(r#"{"transcription": "onnx"}"#),
            Err(SettingsError::Malformed(_))
        ));
    }

    #[test]
    fn criterion_1_unknown_fields_are_tolerated() {
        let json = r#"{
            "version": 2,
            "provider": "openrouter",
            "model": "openai/gpt-4o-mini-transcribe",
            "theme": "dark",
            "future": {"nested": [1, 2, 3]}
        }"#;
        assert_eq!(from_json(json).unwrap(), Settings::default());
    }

    #[test]
    fn criterion_1_a_newer_version_is_refused() {
        assert_eq!(
            from_json(r#"{"version": 3, "provider": "groq"}"#),
            Err(SettingsError::TooNew(3))
        );
        assert!(
            SettingsError::TooNew(3)
                .to_string()
                .contains("newer version (3)")
        );
    }

    #[test]
    fn criterion_1_malformed_files_are_errors() {
        for json in [
            "",
            "not json",
            "[]",
            "42",
            r#"{"version": "two"}"#,
            r#"{"version": -1}"#,
            r#"{"hotkey": 7}"#,
        ] {
            assert!(
                matches!(from_json(json), Err(SettingsError::Malformed(_))),
                "{json:?}"
            );
        }
    }

    #[test]
    fn criterion_1_validation_rejects_each_bad_field() {
        let cases = [
            (
                "version",
                Settings {
                    version: 1,
                    ..Settings::default()
                },
            ),
            (
                "hotkey",
                Settings {
                    hotkey: "Ctrl+Banana".into(),
                    ..Settings::default()
                },
            ),
        ];
        for (field, settings) in cases {
            match settings.validate() {
                Err(SettingsError::Invalid { field: got, .. }) => assert_eq!(got, field),
                other => panic!("{field}: {other:?}"),
            }
            assert!(to_json(&settings).is_err(), "{field}: saved anyway");
        }
    }

    #[test]
    fn criterion_1_invalid_values_in_a_file_are_reported() {
        assert_eq!(
            from_json(r#"{"version": 2, "hotkey": "Ctrl+Banana"}"#).map_err(|e| match e {
                SettingsError::Invalid { field, .. } => field,
                other => panic!("{other:?}"),
            }),
            Err("hotkey")
        );
        assert_eq!(
            SettingsError::Invalid {
                field: "hotkey",
                reason: "it is empty".into()
            }
            .to_string(),
            "invalid hotkey: it is empty"
        );
    }

    /// An old file's fields this build no longer has (`provider`, `model`,
    /// `local_model`, `language`) are ignored; every other preference
    /// survives, and saving drops them.
    #[test]
    fn fields_this_build_no_longer_has_are_ignored() {
        let json = r#"{
            "version": 2,
            "provider": "groq",
            "model": "whisper-large-v3-turbo",
            "language": "de",
            "hotkey": "Ctrl+Shift+Space",
            "launch_at_login": true,
            "transcription": "local",
            "local_model": "qwen3-asr-0.6b-q4_k_m",
            "share_usage_events": false,
            "history_limit": 50
        }"#;
        let settings = from_json(json).unwrap();
        assert_eq!(
            settings,
            Settings {
                version: 2,
                hotkey: "Ctrl+Shift+Space".into(),
                launch_at_login: true,
                transcription: Transcription::Local,
                share_usage_events: false,
                history_limit: 50,
            }
        );
        let saved = to_json(&settings).unwrap();
        assert!(
            !saved.contains("provider") && !saved.contains("model"),
            "{saved}"
        );
    }

    proptest! {
        #[test]
        fn any_json_text_never_panics(json in ".{0,200}") {
            let _ = from_json(&json);
        }

        #[test]
        fn valid_settings_round_trip(
            share_usage_events in any::<bool>(),
            launch_at_login in any::<bool>(),
        ) {
            let settings = Settings {
                launch_at_login,
                share_usage_events,
                ..Settings::default()
            };
            prop_assert_eq!(from_json(&to_json(&settings).unwrap()).unwrap(), settings);
        }
    }
}
