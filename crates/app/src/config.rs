//! The user's settings file and API keys, behind the Tauri commands.
//!
//! [`Config`] reads and writes the settings JSON (via
//! `dictation_core::settings`) in the app's config dir and keeps API keys
//! in a [`SecretStore`] (the OS keychain), never in the file. Keys can be
//! stored and their presence checked, but nothing here hands one back to
//! the UI: [`Config::set_api_key`] is write-only.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use dictation_core::settings::{self, Provider, Settings, SettingsError};
use dictation_ports::{Secret, SecretError, SecretStore};
use thiserror::Error;

/// The settings file's name in the app's config dir.
pub const SETTINGS_FILE: &str = "settings.json";

/// The longest API key accepted.
pub const MAX_KEY_LEN: usize = 512;

/// Why a [`Config`] call failed. Never contains an API key.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    /// The settings are malformed, too new or invalid.
    #[error(transparent)]
    Settings(#[from] SettingsError),
    /// The settings file couldn't be read or written.
    #[error("settings file {path}: {reason}")]
    Io {
        /// The file.
        path: String,
        /// The OS error.
        reason: String,
    },
    /// The keychain failed.
    #[error(transparent)]
    Secret(#[from] SecretError),
    /// The key given to [`Config::set_api_key`] can't be a key; says why,
    /// without the key.
    #[error("that API key {0}")]
    InvalidKey(&'static str),
}

/// The settings file and the API keys.
pub struct Config<S> {
    path: PathBuf,
    secrets: S,
}

impl<S: SecretStore> Config<S> {
    /// Settings in `dir`/[`SETTINGS_FILE`] (created on first save), keys in
    /// `secrets`.
    pub fn new(dir: impl AsRef<Path>, secrets: S) -> Self {
        Self {
            path: dir.as_ref().join(SETTINGS_FILE),
            secrets,
        }
    }

    /// The settings file's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The saved settings, migrated to the current version; the defaults
    /// if nothing was saved yet.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Io`] if the file can't be read, and
    /// [`ConfigError::Settings`] if it's malformed, too new or invalid.
    pub fn settings(&self) -> Result<Settings, ConfigError> {
        match std::fs::read_to_string(&self.path) {
            Ok(json) => Ok(settings::from_json(&json)?),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Settings::default()),
            Err(error) => Err(self.io_error(&error)),
        }
    }

    /// Validates and saves `settings`, replacing the file in one step (a
    /// temporary file, then a rename), so a crash never leaves half a file.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Settings`] if they're invalid (nothing is written),
    /// and [`ConfigError::Io`] if the file can't be written.
    pub fn save_settings(&self, settings: &Settings) -> Result<(), ConfigError> {
        let json = settings::to_json(settings)?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| self.io_error(&e))?;
        }
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, json).map_err(|e| self.io_error(&e))?;
        std::fs::rename(&temporary, &self.path).map_err(|e| self.io_error(&e))?;
        tracing::info!(path = %self.path.display(), "settings saved");
        Ok(())
    }

    /// Stores `key` as `provider`'s API key, replacing any earlier one.
    /// Surrounding whitespace (from a paste) is trimmed.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidKey`] if it's empty, too long or holds spaces,
    /// and [`ConfigError::Secret`] if the keychain refuses.
    pub fn set_api_key(&self, provider: Provider, key: &str) -> Result<(), ConfigError> {
        let key = key.trim();
        if key.is_empty() {
            return Err(ConfigError::InvalidKey("is empty"));
        }
        if key.chars().count() > MAX_KEY_LEN {
            return Err(ConfigError::InvalidKey("is too long"));
        }
        if key.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ConfigError::InvalidKey("contains spaces"));
        }
        self.secrets
            .set(provider.key_name(), &Secret::new(key.to_owned()))?;
        tracing::info!(%provider, "API key saved");
        Ok(())
    }

    /// Whether `provider` has an API key stored.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Secret`] if the keychain refuses.
    pub fn has_api_key(&self, provider: Provider) -> Result<bool, ConfigError> {
        Ok(self.secrets.get(provider.key_name())?.is_some())
    }

    /// Removes `provider`'s API key, if any.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Secret`] if the keychain refuses.
    pub fn delete_api_key(&self, provider: Provider) -> Result<(), ConfigError> {
        self.secrets.delete(provider.key_name())?;
        Ok(())
    }

    fn io_error(&self, error: &std::io::Error) -> ConfigError {
        ConfigError::Io {
            path: self.path.display().to_string(),
            reason: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use dictation_platform::fakes::MemSecretStore;

    use super::*;

    /// A fresh directory in the temp dir, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            Self(std::env::temp_dir().join(format!(
                "dictation-config-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const DUMMY_KEY: &str = "gsk_dummy_not_a_real_key_0123456789";

    fn config(dir: &TempDir) -> (Config<MemSecretStore>, MemSecretStore) {
        let secrets = MemSecretStore::new();
        (Config::new(dir.0.join("config"), secrets.clone()), secrets)
    }

    #[test]
    fn without_a_file_the_settings_are_the_defaults() {
        let dir = TempDir::new();
        let (config, _) = config(&dir);
        assert_eq!(config.settings().unwrap(), Settings::default());
        assert!(!config.path().exists(), "loading writes nothing");
    }

    #[test]
    fn saved_settings_load_back() {
        let dir = TempDir::new();
        let (config, _) = config(&dir);
        let settings = Settings {
            provider: Provider::OpenAi,
            model: "whisper-1".into(),
            language: Some("en".into()),
            ..Settings::default()
        };
        config.save_settings(&settings).unwrap();
        assert_eq!(config.settings().unwrap(), settings);
        assert!(!config.path().with_extension("json.tmp").exists());
    }

    #[test]
    fn invalid_settings_are_not_saved() {
        let dir = TempDir::new();
        let (config, _) = config(&dir);
        let invalid = Settings {
            model: String::new(),
            ..Settings::default()
        };
        assert!(matches!(
            config.save_settings(&invalid),
            Err(ConfigError::Settings(SettingsError::Invalid {
                field: "model",
                ..
            }))
        ));
        assert!(!config.path().exists());
    }

    #[test]
    fn a_v1_file_is_migrated_on_load() {
        let dir = TempDir::new();
        let (config, _) = config(&dir);
        std::fs::create_dir_all(config.path().parent().unwrap()).unwrap();
        std::fs::write(
            config.path(),
            r#"{"version": 1, "engine": "openai", "whisper_model": "whisper-1", "language": ""}"#,
        )
        .unwrap();
        let settings = config.settings().unwrap();
        assert_eq!(settings.provider, Provider::OpenAi);
        assert_eq!(settings.language, None);
    }

    #[test]
    fn a_corrupt_or_unreadable_file_is_an_error() {
        let dir = TempDir::new();
        let (config, _) = config(&dir);
        std::fs::create_dir_all(config.path()).unwrap(); // a directory, not a file
        assert!(matches!(config.settings(), Err(ConfigError::Io { .. })));
        std::fs::remove_dir(config.path()).unwrap();
        std::fs::write(config.path(), "{ not json").unwrap();
        assert!(matches!(
            config.settings(),
            Err(ConfigError::Settings(SettingsError::Malformed(_)))
        ));
    }

    #[test]
    fn criterion_3_the_key_goes_to_the_secret_store_only() {
        let dir = TempDir::new();
        let (config, secrets) = config(&dir);
        assert!(!config.has_api_key(Provider::Groq).unwrap());

        config
            .set_api_key(Provider::Groq, &format!("  {DUMMY_KEY}\n"))
            .unwrap();
        config.save_settings(&Settings::default()).unwrap();

        assert!(config.has_api_key(Provider::Groq).unwrap());
        assert!(
            !config.has_api_key(Provider::OpenAi).unwrap(),
            "per provider"
        );
        assert_eq!(
            secrets.get("groq-api-key").unwrap().unwrap().expose(),
            DUMMY_KEY,
            "trimmed"
        );
        // The settings file never holds the key.
        let file = std::fs::read_to_string(config.path()).unwrap();
        assert!(!file.contains(DUMMY_KEY), "{file}");
        assert!(!file.contains("gsk_"), "{file}");

        config.delete_api_key(Provider::Groq).unwrap();
        assert!(!config.has_api_key(Provider::Groq).unwrap());
    }

    #[test]
    fn criterion_3_keys_that_cant_be_keys_are_refused_without_echoing_them() {
        let dir = TempDir::new();
        let (config, secrets) = config(&dir);
        let long = "k".repeat(MAX_KEY_LEN + 1);
        for (key, why) in [
            ("", "is empty"),
            ("   ", "is empty"),
            (long.as_str(), "is too long"),
            ("gsk_dummy with_space", "contains spaces"),
        ] {
            let error = config.set_api_key(Provider::Groq, key).unwrap_err();
            assert_eq!(error, ConfigError::InvalidKey(why));
            if !key.trim().is_empty() {
                assert!(!error.to_string().contains(key.trim()));
            }
        }
        assert_eq!(secrets.get("groq-api-key").unwrap(), None);
    }

    #[test]
    fn criterion_3_a_denied_keychain_is_an_error_not_a_missing_key() {
        let dir = TempDir::new();
        let (config, secrets) = config(&dir);
        secrets.set_access_denied(true);
        assert_eq!(
            config.has_api_key(Provider::Groq),
            Err(ConfigError::Secret(SecretError::AccessDenied))
        );
        assert_eq!(
            config.set_api_key(Provider::Groq, DUMMY_KEY),
            Err(ConfigError::Secret(SecretError::AccessDenied))
        );
        assert!(config.delete_api_key(Provider::Groq).is_err());
        assert_eq!(
            ConfigError::Secret(SecretError::AccessDenied).to_string(),
            "keychain access was denied"
        );
    }
}
