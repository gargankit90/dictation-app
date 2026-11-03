//! T1 (P2-03): oracles for the settings file's version handling. A v2 file
//! must never be read through the v1 aliases, and a v1 file must keep every
//! field it has. The expected values come from the generated inputs, not
//! from the migration code.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use dictation_core::settings::{CURRENT_VERSION, Provider, Settings, SettingsError, from_json};
use proptest::prelude::*;
use serde_json::{Map, Value, json};

#[test]
fn v2_does_not_apply_v1_alias_migration() {
    // The v1 names mean nothing in v2: they are unknown fields and are
    // ignored, so the v2 defaults stay.
    let settings = from_json(
        r#"{"version": 2, "engine": "openai", "whisper_model": "whisper-1", "hotkey": "Ctrl+Space"}"#,
    )
    .unwrap();
    assert_eq!(settings.provider, Settings::default().provider);
    assert_eq!(settings.model, Settings::default().model);
    assert_eq!(settings.hotkey, "Ctrl+Space");

    // v1 spelled "detect the language" as "". In v2 that is just a bad code,
    // and is reported instead of being quietly turned into "detect".
    assert!(matches!(
        from_json(r#"{"version": 2, "language": ""}"#),
        Err(SettingsError::Invalid {
            field: "language",
            ..
        })
    ));
    // The same text in v1 is "detect".
    assert_eq!(
        from_json(r#"{"version": 1, "language": ""}"#)
            .unwrap()
            .language,
        None
    );
}

#[test]
fn versions_below_two_are_migrated_and_newer_ones_refused() {
    // 0 is older than 2, so it is treated like v1 and comes out as v2.
    let settings = from_json(r#"{"version": 0, "engine": "openai"}"#).unwrap();
    assert_eq!(settings.version, CURRENT_VERSION);
    assert_eq!(settings.provider, Provider::OpenAi);
    for version in [3_u64, 4, u64::from(u32::MAX), u64::MAX] {
        assert_eq!(
            from_json(&format!(r#"{{"version": {version}}}"#)),
            Err(SettingsError::TooNew(version))
        );
    }
}

/// A JSON value no settings field would accept as a sensible one.
fn junk() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        Just(json!(7)),
        Just(json!(["x"])),
        Just(json!({"a": {"b": [1, 2]}})),
        "[ -~]{0,12}".prop_map(Value::from),
    ]
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Groq => "groq",
        Provider::OpenAi => "openai",
    }
}

#[derive(Debug, Clone)]
struct Known {
    provider: Provider,
    model: String,
    language: Option<String>,
    hotkey: &'static str,
}

fn known() -> impl Strategy<Value = Known> {
    (
        prop_oneof![Just(Provider::Groq), Just(Provider::OpenAi)],
        "[a-z0-9.-]{1,40}",
        proptest::option::of("[a-z]{2,3}"),
        prop_oneof![Just("RightAlt"), Just("Ctrl+Space"), Just("F5")],
    )
        .prop_map(|(provider, model, language, hotkey)| Known {
            provider,
            model,
            language,
            hotkey,
        })
}

fn expected(known: &Known) -> Settings {
    Settings {
        version: CURRENT_VERSION,
        provider: known.provider,
        model: known.model.clone(),
        language: known.language.clone(),
        hotkey: known.hotkey.to_owned(),
        launch_at_login: false,
        share_usage_events: true,
    }
}

proptest! {
    // v1 files, with the legacy names, with and without the current names
    // next to them: a current name wins, every other field survives, and
    // unknown fields (any JSON) change nothing.
    #[test]
    fn v1_v2_migration_preserves_known_fields(
        legacy in known(),
        current in known(),
        both_names in any::<bool>(),
        versioned in any::<bool>(),
        extras in proptest::collection::vec(("[a-z_]{3,12}", junk()), 0..4),
    ) {
        let mut object = Map::new();
        if versioned {
            object.insert("version".into(), json!(1));
        }
        object.insert("engine".into(), json!(provider_name(legacy.provider)));
        object.insert("whisper_model".into(), json!(legacy.model));
        object.insert("language".into(), json!(legacy.language.clone().unwrap_or_default()));
        object.insert("hotkey".into(), json!(legacy.hotkey));
        let mut want = expected(&legacy);
        if both_names {
            object.insert("provider".into(), json!(provider_name(current.provider)));
            object.insert("model".into(), json!(current.model));
            want.provider = current.provider;
            want.model = current.model.clone();
        }
        for (key, value) in extras {
            // Unknown means unknown: skip names the format uses.
            if !["version", "engine", "whisper_model", "provider", "model", "language", "hotkey"]
                .contains(&key.as_str())
            {
                object.insert(key, value);
            }
        }
        prop_assert_eq!(from_json(&Value::Object(object).to_string()).unwrap(), want);
    }

    // v2 files: the legacy names are noise, whatever they hold.
    #[test]
    fn v2_files_ignore_legacy_names_whatever_they_hold(
        v2 in known(),
        engine in proptest::option::of(junk()),
        whisper_model in proptest::option::of(junk()),
    ) {
        let mut object = Map::new();
        object.insert("version".into(), json!(2));
        object.insert("provider".into(), json!(provider_name(v2.provider)));
        object.insert("model".into(), json!(v2.model));
        object.insert("hotkey".into(), json!(v2.hotkey));
        match &v2.language {
            Some(language) => object.insert("language".into(), json!(language)),
            None => object.insert("language".into(), Value::Null),
        };
        if let Some(engine) = engine {
            object.insert("engine".into(), engine);
        }
        if let Some(whisper_model) = whisper_model {
            object.insert("whisper_model".into(), whisper_model);
        }
        prop_assert_eq!(from_json(&Value::Object(object).to_string()).unwrap(), expected(&v2));
    }

    // A wrong-typed known field is an error, never a silent default.
    #[test]
    fn wrong_typed_known_fields_are_malformed(
        field in prop_oneof![Just("provider"), Just("model"), Just("hotkey")],
        version in 1_u32..=2,
        wrong in prop_oneof![Just(json!(7)), Just(json!(["x"])), Just(json!({"a": 1})), Just(json!(true))],
    ) {
        let mut object = Map::new();
        object.insert("version".into(), json!(version));
        object.insert(field.into(), wrong);
        prop_assert!(matches!(
            from_json(&Value::Object(object).to_string()),
            Err(SettingsError::Malformed(_))
        ));
    }
}
