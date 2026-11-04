//! The first launch after the app was renamed from dictation-app to Vocet
//! (U1): macOS may then ask once whether the app can use the API key saved
//! in the Keychain, so Settings says so for that launch only.
//!
//! The app remembers the name it last ran under in a file in the config
//! folder. A build from before the rename wrote no such file, so a settings
//! file with no name beside it means the previous run was under the old name.

use std::path::Path;

/// The file in the config folder holding the name the app last ran under.
pub const APP_NAME_MARKER: &str = "last-run-app-name";

/// What Settings says on the first launch after the rename.
pub const KEYCHAIN_NOTICE: &str = "After this update, macOS may ask once whether Vocet can use the key saved in your Keychain. Choose Always Allow. Your key stays in your Keychain and is sent only to your speech provider (Groq or OpenAI).";

/// Whether this is the first launch under `app_name` of an install that ran
/// under another name (or, with no name remembered, one that has settings:
/// the build before the rename wrote none). Remembers `app_name`, so only
/// one launch says yes. A fresh install says no.
#[must_use]
pub fn first_launch_after_rename(config_dir: &Path, app_name: &str) -> bool {
    let marker = config_dir.join(APP_NAME_MARKER);
    let before = std::fs::read_to_string(&marker).ok();
    let renamed = match before.as_deref() {
        Some(name) => name.trim() != app_name,
        None => config_dir.join(crate::config::SETTINGS_FILE).exists(),
    };
    if before.as_deref().map(str::trim) != Some(app_name) {
        // Not remembering means asking again next launch, which is the safe
        // side: the notice shows once more.
        if std::fs::create_dir_all(config_dir)
            .and_then(|()| std::fs::write(&marker, app_name))
            .is_err()
        {
            tracing::warn!("could not remember the app's name");
        }
    }
    renamed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("u1-rename-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_fresh_install_is_not_a_rename() {
        let d = dir("fresh");
        assert!(!first_launch_after_rename(&d, "Vocet"));
        assert!(!first_launch_after_rename(&d, "Vocet"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_build_from_before_the_rename_left_settings_and_no_name() {
        let d = dir("old");
        std::fs::write(d.join(crate::config::SETTINGS_FILE), "{}").unwrap();
        // The first launch as Vocet says yes, and only that one.
        assert!(first_launch_after_rename(&d, "Vocet"));
        assert!(!first_launch_after_rename(&d, "Vocet"));
        assert!(!first_launch_after_rename(&d, "Vocet"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_remembered_other_name_is_a_rename() {
        let d = dir("named");
        std::fs::write(d.join(APP_NAME_MARKER), "dictation-app").unwrap();
        assert!(first_launch_after_rename(&d, "Vocet"));
        assert!(!first_launch_after_rename(&d, "Vocet"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_notice_says_what_to_do_and_that_the_key_stays() {
        assert!(KEYCHAIN_NOTICE.contains("Always Allow"));
        assert!(KEYCHAIN_NOTICE.contains("sent only to your speech provider (Groq or OpenAI)"));
        assert!(!KEYCHAIN_NOTICE.contains("never leaves"));
    }
}
