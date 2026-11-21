//! The user's settings: the model, defaults, validation and the on-disk JSON
//! format with its schema version and migrations.
//!
//! Pure: [`from_json`] and [`to_json`] turn text into settings and back; the
//! app reads and writes the file. The API key is never part of the
//! settings: it lives in the OS keychain (`docs/ENGINEERING.md`).
//!
//! The file is `{"version": 2, …}`. M5.3 added `launch_at_login`, L1
//! `transcription` and `local_model`, A1 `share_usage_events`, U7
//! `history_limit`, all without a version bump: a file without one loads
//! with its default. Since Q2 `provider` is `"openrouter"`; an unknown
//! `provider` or `model` loads as its default. There are no migrations: an
//! older (or unversioned) file is read as the current version.
//!
//! Fields this version doesn't know (from a newer build, or a hand edit) are
//! ignored, and missing ones take their defaults, so an old or partial file
//! still loads. A file from a *newer* schema version is refused rather than
//! guessed at.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::cloud_models;
use crate::hotkey_spec::HotkeySpec;
use crate::local_models;

/// The schema version this build reads and writes.
pub const CURRENT_VERSION: u32 = 2;

/// The longest model name accepted.
pub const MAX_MODEL_LEN: usize = 100;

/// How many recent dictations the user can choose to keep; 0 is Off (U7).
pub const HISTORY_LIMITS: [u32; 5] = [0, 10, 25, 50, 100];

/// How many recent dictations are kept unless the user chooses otherwise.
pub const DEFAULT_HISTORY_LIMIT: u32 = 25;

/// The cloud provider: `OpenRouter`, with the user's own key (Q2). A file
/// with any other value loads as `OpenRouter` ([`from_json`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// `OpenRouter`'s API: speech and cleanup models from
    /// [`crate::cloud_models`].
    #[default]
    #[serde(rename = "openrouter")]
    OpenRouter,
}

impl Provider {
    /// Every provider.
    pub const ALL: [Self; 1] = [Self::OpenRouter];

    /// The speech model a new user gets with this provider.
    #[must_use]
    pub fn default_model(self) -> &'static str {
        match self {
            Self::OpenRouter => cloud_models::DEFAULT_SPEECH,
        }
    }

    /// The name this provider's API key is stored under in the keychain.
    #[must_use]
    pub fn key_name(self) -> &'static str {
        match self {
            Self::OpenRouter => "openrouter-api-key",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OpenRouter => "openrouter",
        })
    }
}

/// Where speech becomes text (L1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Transcription {
    /// A cloud provider with the user's own API key ([`Settings::provider`]).
    #[default]
    Cloud,
    /// A downloaded model on this Mac ([`Settings::local_model`]); no key.
    Local,
}

/// What the user can set. Never holds the API key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(default)]
pub struct Settings {
    /// The schema version; always [`CURRENT_VERSION`] once loaded.
    pub version: u32,
    /// Which cloud API transcribes.
    pub provider: Provider,
    /// The speech model's id in [`crate::cloud_models::CATALOG`], e.g.
    /// `openai/gpt-4o-mini-transcribe`. An id the catalog no longer has
    /// (a model shut down, or one from before Q2) loads as the default.
    pub model: String,
    /// The dictation hotkey, as [`HotkeySpec`] writes it (`RightAlt`).
    pub hotkey: String,
    /// Whether the app starts when the user logs in (M5.3). The file keeps
    /// the last value applied; what the settings screen shows is the OS's
    /// own state, which the user can change in System Settings.
    pub launch_at_login: bool,
    /// Cloud provider or on-device model (L1).
    pub transcription: Transcription,
    /// The on-device model's id, used when `transcription` is
    /// [`Transcription::Local`]. An id the catalogue no longer has still
    /// loads; the engine then says to pick a model in Settings.
    pub local_model: String,
    /// Whether anonymous usage events are shared (A1, `docs/privacy.md`).
    /// On by default; a file without it loads with `true`.
    pub share_usage_events: bool,
    /// How many recent dictations are kept, one of [`HISTORY_LIMITS`] (U7).
    /// A file without it loads with [`DEFAULT_HISTORY_LIMIT`].
    pub history_limit: u32,
}

impl Default for Settings {
    fn default() -> Self {
        let provider = Provider::default();
        Self {
            version: CURRENT_VERSION,
            provider,
            model: provider.default_model().to_owned(),
            hotkey: HotkeySpec::default().to_string(),
            launch_at_login: false,
            transcription: Transcription::default(),
            local_model: local_models::DEFAULT_MODEL.to_owned(),
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
    /// `version` other than [`CURRENT_VERSION`], an empty, over-long or
    /// whitespace-holding `model` or `local_model`, a `hotkey` that doesn't
    /// parse, or a
    /// `history_limit` not in [`HISTORY_LIMITS`].
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.version != CURRENT_VERSION {
            return Err(invalid(
                "version",
                format!("expected {CURRENT_VERSION}, got {}", self.version),
            ));
        }
        check_model_name("model", &self.model)?;
        check_model_name("local_model", &self.local_model)?;
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

/// A model name or id: not empty, not over-long, no spaces.
fn check_model_name(field: &'static str, name: &str) -> Result<(), SettingsError> {
    if name.is_empty() {
        return Err(invalid(field, "it is empty".to_owned()));
    }
    if name.chars().count() > MAX_MODEL_LEN {
        return Err(invalid(
            field,
            format!("longer than {MAX_MODEL_LEN} characters"),
        ));
    }
    if name.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid(field, "it contains spaces".to_owned()));
    }
    Ok(())
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
    default_unknown_values(&mut object);
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

/// A `provider` this build doesn't know (`OpenRouter` is the only one since
/// Q2; an old file may say `"groq"` or `"openai"`) or a speech `model` the
/// cloud catalog doesn't have loads as that field's default, so an old file
/// still loads with every other field kept (#328).
fn default_unknown_values(object: &mut Map<String, Value>) {
    let provider = Provider::OpenRouter.to_string();
    if object.get("provider").and_then(Value::as_str) != Some(provider.as_str())
        && object.contains_key("provider")
    {
        object.insert("provider".to_owned(), Value::from(provider));
    }
    // A model of the wrong type stays, so the file is reported malformed.
    let unknown = object
        .get("model")
        .and_then(Value::as_str)
        .is_some_and(|id| cloud_models::speech(id).is_none());
    if unknown {
        object.insert(
            "model".to_owned(),
            Value::from(Provider::OpenRouter.default_model()),
        );
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn criterion_1_defaults_are_valid_openrouter() {
        let settings = Settings::default();
        assert_eq!(settings.version, CURRENT_VERSION);
        assert_eq!(settings.provider, Provider::OpenRouter);
        assert_eq!(settings.model, "openai/gpt-4o-mini-transcribe");
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
            provider: Provider::OpenRouter,
            model: cloud_models::DEFAULT_SPEECH.into(),
            hotkey: "Ctrl+Shift+Space".into(),
            ..Settings::default()
        };
        let json = to_json(&settings).unwrap();
        assert!(json.contains(r#""provider": "openrouter""#), "{json}");
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
    fn l1_criterion_4_transcription_defaults_to_cloud_with_the_default_model() {
        let settings = Settings::default();
        assert_eq!(settings.transcription, Transcription::Cloud);
        assert_eq!(settings.local_model, local_models::DEFAULT_MODEL);
        // A file from before L1 has neither field.
        let old = from_json(r#"{"version": 2, "provider": "openai", "model": "whisper-1"}"#);
        let old = old.unwrap();
        assert_eq!(old.transcription, Transcription::Cloud);
        assert_eq!(old.local_model, local_models::DEFAULT_MODEL);
    }

    #[test]
    fn l1_criterion_4_the_local_choice_round_trips() {
        let local = Settings {
            transcription: Transcription::Local,
            local_model: "qwen3-asr-0.6b-q4_k_m".into(),
            ..Settings::default()
        };
        let json = to_json(&local).unwrap();
        assert!(json.contains(r#""transcription": "local""#), "{json}");
        assert!(json.contains(r#""local_model": "qwen3-asr-0.6b-q4_k_m""#));
        assert_eq!(from_json(&json).unwrap(), local);
        // An id a later catalogue dropped still loads.
        let gone = from_json(r#"{"version": 2, "local_model": "retired-model"}"#).unwrap();
        assert_eq!(gone.local_model, "retired-model");
        assert!(matches!(
            from_json(r#"{"transcription": "onnx"}"#),
            Err(SettingsError::Malformed(_))
        ));
        for bad in ["", "two words"] {
            let settings = Settings {
                local_model: bad.into(),
                ..Settings::default()
            };
            assert!(
                matches!(
                    settings.validate(),
                    Err(SettingsError::Invalid {
                        field: "local_model",
                        ..
                    })
                ),
                "{bad:?}"
            );
        }
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
            r#"{"model": 7}"#,
        ] {
            assert!(
                matches!(from_json(json), Err(SettingsError::Malformed(_))),
                "{json:?}"
            );
        }
    }

    #[test]
    fn criterion_1_validation_rejects_each_bad_field() {
        let cases: [(&str, Settings); 5] = [
            (
                "version",
                Settings {
                    version: 1,
                    ..Settings::default()
                },
            ),
            (
                "model",
                Settings {
                    model: String::new(),
                    ..Settings::default()
                },
            ),
            (
                "model",
                Settings {
                    model: "x".repeat(MAX_MODEL_LEN + 1),
                    ..Settings::default()
                },
            ),
            (
                "model",
                Settings {
                    model: "whisper large".into(),
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
        // The longest model is fine.
        Settings {
            model: "x".repeat(MAX_MODEL_LEN),
            ..Settings::default()
        }
        .validate()
        .unwrap();
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
                field: "model",
                reason: "it is empty".into()
            }
            .to_string(),
            "invalid model: it is empty"
        );
    }

    /// Q2 criterion 1 (#328): an old file whose provider or model this
    /// build doesn't know still loads: those fields take their defaults and
    /// every other preference survives.
    #[test]
    fn q2_criterion_1_unknown_provider_and_model_values_load_as_defaults() {
        for provider in [
            r#""groq""#,
            r#""openai""#,
            r#""whisper.cpp""#,
            r#""""#,
            "7",
            "null",
            r#"{"name": "groq"}"#,
        ] {
            let json = format!(
                r#"{{
                    "version": 2,
                    "provider": {provider},
                    "model": "whisper-large-v3-turbo",
                    "language": "de",
                    "hotkey": "Ctrl+Shift+Space",
                    "launch_at_login": true,
                    "transcription": "local",
                    "local_model": "qwen3-asr-0.6b-q4_k_m",
                    "share_usage_events": false,
                    "history_limit": 50
                }}"#
            );
            let settings = from_json(&json).unwrap_or_else(|e| panic!("{provider}: {e}"));
            assert_eq!(
                settings,
                Settings {
                    version: 2,
                    provider: Provider::OpenRouter,
                    model: cloud_models::DEFAULT_SPEECH.into(),
                    hotkey: "Ctrl+Shift+Space".into(),
                    launch_at_login: true,
                    transcription: Transcription::Local,
                    local_model: "qwen3-asr-0.6b-q4_k_m".into(),
                    share_usage_events: false,
                    history_limit: 50,
                },
                "{provider}"
            );
            // Saved back, it is an OpenRouter file.
            let saved = to_json(&settings).unwrap();
            assert!(saved.contains(r#""provider": "openrouter""#), "{saved}");
            assert!(!saved.contains("groq") && !saved.contains("whisper-large"));
        }
        // A model of the wrong type is still a malformed file.
        assert!(matches!(
            from_json(r#"{"model": 7}"#),
            Err(SettingsError::Malformed(_))
        ));
    }

    #[test]
    fn openrouter_has_the_default_model_and_its_own_key_name() {
        assert_eq!(
            Provider::OpenRouter.default_model(),
            cloud_models::DEFAULT_SPEECH
        );
        assert_eq!(Provider::OpenRouter.key_name(), "openrouter-api-key");
        assert_eq!(Provider::OpenRouter.to_string(), "openrouter");
        for provider in Provider::ALL {
            let json = serde_json::to_string(&provider).unwrap();
            assert_eq!(json, format!("\"{provider}\""));
        }
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
            let model = cloud_models::DEFAULT_SPEECH;
            let settings = Settings {
                provider: Provider::OpenRouter,
                model: model.to_owned(),
                share_usage_events,
                launch_at_login,
                ..Settings::default()
            };
            prop_assert_eq!(from_json(&to_json(&settings).unwrap()).unwrap(), settings);
        }
    }
}
