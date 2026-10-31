//! M3.3 tester edge cases, black-box through the public `Config` API.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use dictation_core::settings::{Provider, Settings};
use dictation_pipeline::config::Config;
use dictation_platform::fakes::MemSecretStore;
use dictation_ports::SecretStore;

const KEY: &str = "gsk_TESTER-dummy-key-0123456789";

fn dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("m33-tester-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn whitespace_and_control_keys_are_refused_without_echo() {
    let d = dir("ws");
    let store = MemSecretStore::default();
    let config = Config::new(&d, store.clone());
    for bad in [
        "",
        "   ",
        "\n\t ",
        "ab cd-secret",
        "ab\ncd-secret",
        "ab\u{0}cd-secret",
    ] {
        let err = config
            .set_api_key(Provider::Groq, bad)
            .unwrap_err()
            .to_string();
        assert!(!err.contains("secret"), "error echoed the key: {err}");
        assert!(store.get("groq-api-key").unwrap().is_none());
    }
    let long = "k".repeat(513);
    let err = config
        .set_api_key(Provider::Groq, &long)
        .unwrap_err()
        .to_string();
    assert!(!err.contains(&long));
    // exactly the max is accepted
    config
        .set_api_key(Provider::Groq, &"k".repeat(512))
        .unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn key_never_reaches_the_settings_file_or_errors_and_providers_are_separate() {
    let d = dir("leak");
    let store = MemSecretStore::default();
    let config = Config::new(&d, store.clone());
    config
        .set_api_key(Provider::Groq, &format!("  {KEY}\n"))
        .unwrap();
    assert!(config.has_api_key(Provider::Groq).unwrap());
    assert!(!config.has_api_key(Provider::OpenAi).unwrap());
    config.save_settings(&Settings::default()).unwrap();
    let file = std::fs::read_to_string(config.path()).unwrap();
    assert!(!file.contains(KEY) && !file.contains("gsk_"));
    // every file in the config dir is clean
    for e in std::fs::read_dir(&d).unwrap() {
        let text = std::fs::read_to_string(e.unwrap().path()).unwrap_or_default();
        assert!(!text.contains(KEY));
    }
    config.delete_api_key(Provider::Groq).unwrap();
    assert!(!config.has_api_key(Provider::Groq).unwrap());
    config.delete_api_key(Provider::Groq).unwrap(); // idempotent
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn unknown_fields_load_but_are_dropped_on_the_next_save() {
    let d = dir("unk");
    let config = Config::new(&d, MemSecretStore::default());
    std::fs::write(
        config.path(),
        r#"{"version":2,"provider":"openai","model":"whisper-1","language":"de","hotkey":"RightAlt","future":{"x":1}}"#,
    )
    .unwrap();
    let s = config.settings().unwrap();
    assert_eq!(s.provider, Provider::OpenAi);
    assert_eq!(s.language.as_deref(), Some("de"));
    config.save_settings(&s).unwrap();
    let file = std::fs::read_to_string(config.path()).unwrap();
    assert!(!file.contains("future")); // documents the Developer's stated behaviour
    assert_eq!(config.settings().unwrap(), s);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bad_settings_values_are_rejected_and_leave_the_old_file_alone() {
    let d = dir("bad");
    let config = Config::new(&d, MemSecretStore::default());
    config.save_settings(&Settings::default()).unwrap();
    let before = std::fs::read_to_string(config.path()).unwrap();
    for mutate in [
        (|s: &mut Settings| s.model = String::new()) as fn(&mut Settings),
        |s| s.model = "has space".into(),
        |s| s.model = "m".repeat(101),
        |s| s.language = Some("EN".into()),
        |s| s.language = Some("e".into()),
        |s| s.language = Some("english".into()),
        |s| s.language = Some(String::new()),
        |s| s.hotkey = "NotAKey".into(),
        |s| s.version = 1,
    ] {
        let mut s = Settings::default();
        mutate(&mut s);
        let err = config.save_settings(&s).unwrap_err().to_string();
        assert!(!err.is_empty());
    }
    assert_eq!(std::fs::read_to_string(config.path()).unwrap(), before);
    let _ = std::fs::remove_dir_all(&d);
}
