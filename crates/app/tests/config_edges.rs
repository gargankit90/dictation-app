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

fn leftovers(d: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(d)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

/// Run 2 stress: 16 threads x 100 saves of wildly different lengths through one
/// `Arc<Config>`, while a reader loads continuously. No save errors, every read
/// parses to one of the saved values, and no temp files remain.
#[test]
fn stress_concurrent_saves_with_a_concurrent_reader() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    const THREADS: usize = 16;
    let d = dir("stress");
    let config = Arc::new(Config::new(&d, MemSecretStore::default()));
    let variant = |t: usize| Settings {
        model: "m".repeat(1 + t * 37 % 100),
        ..Settings::default()
    };
    config.save_settings(&variant(0)).unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let reader = {
        let (config, done) = (config.clone(), done.clone());
        std::thread::spawn(move || {
            let mut reads = 0u32;
            while !done.load(Ordering::Relaxed) {
                let s = config.settings().expect("a read never sees a torn file");
                assert!((0..THREADS).any(|t| s == variant(t)));
                reads += 1;
            }
            reads
        })
    };
    let writers: Vec<_> = (0..THREADS)
        .map(|t| {
            let config = config.clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    config.save_settings(&variant(t)).unwrap();
                }
            })
        })
        .collect();
    for w in writers {
        w.join().unwrap();
    }
    done.store(true, Ordering::Relaxed);
    assert!(reader.join().unwrap() > 0);
    let s = config.settings().unwrap();
    assert!((0..THREADS).any(|t| s == variant(t)));
    assert_eq!(leftovers(&d).len(), 1);
    let _ = std::fs::remove_dir_all(&d);
}

/// A save into a read-only directory fails with an error and leaves nothing behind.
#[cfg(unix)]
#[test]
fn save_into_a_read_only_directory_fails_cleanly() {
    use std::os::unix::fs::PermissionsExt;
    let d = dir("ro");
    let config = Config::new(&d, MemSecretStore::default());
    config.save_settings(&Settings::default()).unwrap();
    let before = std::fs::read_to_string(config.path()).unwrap();
    let names = leftovers(&d);
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = std::fs::File::create(d.join("probe")).is_ok(); // root ignores modes
    let result = config.save_settings(&Settings {
        model: "other-model".into(),
        ..Settings::default()
    });
    std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
    if !probe {
        assert!(result.is_err(), "save into a read-only dir must error");
        assert_eq!(std::fs::read_to_string(config.path()).unwrap(), before);
        assert_eq!(leftovers(&d), names);
    }
    let _ = std::fs::remove_dir_all(&d);
}
