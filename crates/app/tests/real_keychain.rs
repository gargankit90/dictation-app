//! M3.3 criterion 4: the API-key path against the real macOS login
//! keychain. Ignored by default; run it once by hand on a Mac:
//!
//! ```sh
//! cargo test -p dictation-pipeline --test real_keychain -- --ignored --nocapture
//! ```
//!
//! It stores a dummy value (never a real key) as `groq-api-key` under the
//! service `com.dictationapp.desktop`, reads it back, checks
//! `has_api_key`, and deletes it again. With `KEEP_ITEM=1` it leaves the
//! item there so it can be seen in Keychain Access; delete it there (or run
//! again without `KEEP_ITEM`) afterwards. macOS may ask to allow access the
//! first time.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use dictation_core::settings::{Provider, Settings};
use dictation_pipeline::config::Config;
use dictation_platform::secrets::{KeychainSecretStore, SERVICE};
use dictation_ports::SecretStore;

const DUMMY: &str = "dummy-value-for-the-keychain-test";

#[test]
#[ignore = "touches the real keychain; run by hand (M3.3 criterion 4)"]
fn criterion_4_real_keychain_round_trip() {
    let store = KeychainSecretStore::new();
    let name = Provider::Groq.key_name();
    // Never clobber a key the user really saved.
    assert!(
        store.get(name).unwrap().is_none(),
        "a {name} item already exists under {SERVICE}; not touching it"
    );
    let dir = std::env::temp_dir().join(format!("dictation-real-keychain-{}", std::process::id()));
    let config = Config::new(&dir, store.clone());

    assert!(!config.has_api_key(Provider::Groq).unwrap());
    config.set_api_key(Provider::Groq, DUMMY).unwrap();
    assert_eq!(store.get(name).unwrap().unwrap().expose(), DUMMY);
    assert!(config.has_api_key(Provider::Groq).unwrap());
    config.save_settings(&Settings::default()).unwrap();
    let file = std::fs::read_to_string(config.path()).unwrap();
    assert!(!file.contains(DUMMY), "the key leaked into {file}");
    println!("stored a dummy item: service {SERVICE}, account {name}");

    if std::env::var_os("KEEP_ITEM").is_none() {
        config.delete_api_key(Provider::Groq).unwrap();
        assert!(!config.has_api_key(Provider::Groq).unwrap());
        println!("deleted it again");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
