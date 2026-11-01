//! The user's settings: the model, defaults, validation and the on-disk JSON
//! format with its schema version and migrations.
//!
//! Pure: [`from_json`] and [`to_json`] turn text into settings and back; the
//! app reads and writes the file. The API key is never part of the
//! settings: it lives in the OS keychain (`docs/ENGINEERING.md`).
//!
//! Versions:
//!
//! - **v1** (M3.0 drafts): `{"version": 1, "engine": "groq",
//!   "whisper_model": "…", "language": "en" | "", "hotkey": "RightAlt"}`,
//!   with `""` meaning "detect the language".
//! - **v2** (current): `engine` is renamed `provider`, `whisper_model` is
//!   renamed `model`, and "detect" is `null` instead of `""`. M5.3 added
//!   `launch_at_login` (default `false`) without a version bump: a file
//!   without it loads with the default.
//!
//! Fields this version doesn't know (from a newer build, or a hand edit) are
//! ignored, and missing ones take their defaults, so an old or partial file
//! still loads. A file from a *newer* schema version is refused rather than
//! guessed at.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::hotkey_spec::HotkeySpec;

/// The schema version this build reads and writes.
pub const CURRENT_VERSION: u32 = 2;

/// The longest model name accepted.
pub const MAX_MODEL_LEN: usize = 100;

/// The cloud speech-to-text provider.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// Groq's `OpenAI`-compatible Whisper API.
    #[default]
    Groq,
    /// The `OpenAI` Whisper API.
    #[serde(rename = "openai")]
    OpenAi,
}

impl Provider {
    /// Every provider.
    pub const ALL: [Self; 2] = [Self::Groq, Self::OpenAi];

    /// The model a new user gets with this provider.
    #[must_use]
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Groq => "whisper-large-v3-turbo",
            Self::OpenAi => "whisper-1",
        }
    }

    /// The name this provider's API key is stored under in the keychain
    /// (one key per provider).
    #[must_use]
    pub fn key_name(self) -> &'static str {
        match self {
            Self::Groq => "groq-api-key",
            Self::OpenAi => "openai-api-key",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Groq => "groq",
            Self::OpenAi => "openai",
        })
    }
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
    /// The provider's model name, e.g. `whisper-large-v3-turbo`.
    pub model: String,
    /// An ISO 639-1 (or 639-3) language code such as `en`, or `None` to let
    /// the engine detect the language.
    pub language: Option<String>,
    /// The dictation hotkey, as [`HotkeySpec`] writes it (`RightAlt`).
    pub hotkey: String,
    /// Whether the app starts when the user logs in (M5.3). The file keeps
    /// the last value applied; what the settings screen shows is the OS's
    /// own state, which the user can change in System Settings.
    pub launch_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let provider = Provider::default();
        Self {
            version: CURRENT_VERSION,
            provider,
            model: provider.default_model().to_owned(),
            language: None,
            hotkey: HotkeySpec::default().to_string(),
            launch_at_login: false,
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
    /// whitespace-holding `model`, a `language` that isn't 2–3 lowercase
    /// letters, or a `hotkey` that doesn't parse.
    pub fn validate(&self) -> Result<(), SettingsError> {
        if self.version != CURRENT_VERSION {
            return Err(invalid(
                "version",
                format!("expected {CURRENT_VERSION}, got {}", self.version),
            ));
        }
        if self.model.is_empty() {
            return Err(invalid("model", "it is empty".to_owned()));
        }
        if self.model.chars().count() > MAX_MODEL_LEN {
            return Err(invalid(
                "model",
                format!("longer than {MAX_MODEL_LEN} characters"),
            ));
        }
        if self
            .model
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(invalid("model", "it contains spaces".to_owned()));
        }
        if let Some(language) = &self.language {
            let letters = (2..=3).contains(&language.len())
                && language.chars().all(|c| c.is_ascii_lowercase());
            if !letters {
                return Err(invalid(
                    "language",
                    format!("{language:?} is not a language code like \"en\""),
                ));
            }
        }
        if let Err(error) = self.hotkey.parse::<HotkeySpec>() {
            return Err(invalid("hotkey", error.to_string()));
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
    // No version: the first format, before it had one.
    let version = match object.get("version") {
        None => 1,
        Some(version) => version
            .as_u64()
            .ok_or_else(|| SettingsError::Malformed("version is not a number".to_owned()))?,
    };
    if version > u64::from(CURRENT_VERSION) {
        return Err(SettingsError::TooNew(version));
    }
    if version < 2 {
        migrate_v1_to_v2(&mut object);
    }
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

/// v1 → v2: `engine` → `provider`, `whisper_model` → `model`, and an empty
/// `language` (detect) → `null`. Fields v2 already has win over renamed
/// ones; anything else is left alone.
fn migrate_v1_to_v2(object: &mut Map<String, Value>) {
    for (old, new) in [("engine", "provider"), ("whisper_model", "model")] {
        if let Some(value) = object.remove(old) {
            object.entry(new).or_insert(value);
        }
    }
    if object.get("language").and_then(Value::as_str) == Some("") {
        object.insert("language".to_owned(), Value::Null);
    }
    object.insert("version".to_owned(), Value::from(2));
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn criterion_1_defaults_are_valid_groq_with_language_detection() {
        let settings = Settings::default();
        assert_eq!(settings.version, CURRENT_VERSION);
        assert_eq!(settings.provider, Provider::Groq);
        assert_eq!(settings.model, "whisper-large-v3-turbo");
        assert_eq!(settings.language, None);
        assert_eq!(settings.hotkey, "RightAlt");
        settings.validate().unwrap();
    }

    #[test]
    fn criterion_1_an_empty_object_loads_as_the_defaults() {
        // No version means v1; every field is missing, so all default.
        assert_eq!(from_json("{}").unwrap(), Settings::default());
        assert_eq!(from_json(r#"{"version": 2}"#).unwrap(), Settings::default());
    }

    #[test]
    fn criterion_1_round_trips_through_json() {
        let settings = Settings {
            provider: Provider::OpenAi,
            model: "whisper-1".into(),
            language: Some("de".into()),
            hotkey: "Ctrl+Shift+Space".into(),
            ..Settings::default()
        };
        let json = to_json(&settings).unwrap();
        assert!(json.contains(r#""provider": "openai""#), "{json}");
        assert_eq!(from_json(&json).unwrap(), settings);
    }

    #[test]
    fn criterion_1_v1_is_migrated_to_v2() {
        let v1 = r#"{
            "version": 1,
            "engine": "openai",
            "whisper_model": "whisper-1",
            "language": "",
            "hotkey": "Ctrl+Space"
        }"#;
        assert_eq!(
            from_json(v1).unwrap(),
            Settings {
                version: 2,
                provider: Provider::OpenAi,
                model: "whisper-1".into(),
                language: None,
                hotkey: "Ctrl+Space".into(),
                launch_at_login: false,
            }
        );
        // A language stays a language; no version at all is v1 too.
        let v1 = r#"{"engine": "groq", "whisper_model": "whisper-large-v3", "language": "fr"}"#;
        let settings = from_json(v1).unwrap();
        assert_eq!(settings.model, "whisper-large-v3");
        assert_eq!(settings.language.as_deref(), Some("fr"));
        assert_eq!(settings.hotkey, "RightAlt");
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
    fn criterion_1_a_v2_field_wins_over_its_v1_name() {
        let mixed = r#"{"version": 1, "engine": "openai", "provider": "groq"}"#;
        assert_eq!(from_json(mixed).unwrap().provider, Provider::Groq);
    }

    #[test]
    fn criterion_1_unknown_fields_are_tolerated() {
        let json = r#"{
            "version": 2,
            "provider": "groq",
            "model": "whisper-large-v3-turbo",
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
            r#"{"provider": "whisper.cpp"}"#,
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
        let cases: [(&str, Settings); 8] = [
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
                "language",
                Settings {
                    language: Some("English".into()),
                    ..Settings::default()
                },
            ),
            (
                "language",
                Settings {
                    language: Some(String::new()),
                    ..Settings::default()
                },
            ),
            (
                "language",
                Settings {
                    language: Some("EN".into()),
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
        // The longest model and a three-letter code are fine.
        Settings {
            model: "x".repeat(MAX_MODEL_LEN),
            language: Some("yue".into()),
            ..Settings::default()
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn criterion_1_invalid_values_in_a_file_are_reported() {
        assert_eq!(
            from_json(r#"{"version": 2, "language": "english"}"#),
            Err(SettingsError::Invalid {
                field: "language",
                reason: r#""english" is not a language code like "en""#.into()
            })
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

    #[test]
    fn providers_have_a_default_model_and_their_own_key_name() {
        assert_eq!(Provider::OpenAi.default_model(), "whisper-1");
        assert_eq!(Provider::Groq.key_name(), "groq-api-key");
        assert_eq!(Provider::OpenAi.key_name(), "openai-api-key");
        assert_eq!(Provider::OpenAi.to_string(), "openai");
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
            openai in any::<bool>(),
            model in "[a-z0-9.-]{1,40}",
            language in proptest::option::of("[a-z]{2,3}"),
        ) {
            let settings = Settings {
                provider: if openai { Provider::OpenAi } else { Provider::Groq },
                model,
                language,
                ..Settings::default()
            };
            prop_assert_eq!(from_json(&to_json(&settings).unwrap()).unwrap(), settings);
        }
    }
}
