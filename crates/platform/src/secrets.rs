//! [`KeychainSecretStore`]: API keys in the OS keychain through the
//! `keyring` crate (MIT OR Apache-2.0). macOS: the login keychain, as
//! generic passwords with service [`SERVICE`] and the secret's name as the
//! account (`groq-api-key`, `openai-api-key`; see
//! `dictation_core::settings::Provider::key_name`).
//!
//! Never logs a secret, and no error carries one.

use dictation_ports::{Secret, SecretError, SecretStore};

/// The keychain service every secret is stored under; it's the app's bundle
/// identifier, so Keychain Access lists the items under the app's name.
pub const SERVICE: &str = "com.dictationapp.desktop";

/// The OS keychain. Where there's no adapter yet (Windows, Linux), every
/// call fails with [`SecretError::Store`].
#[derive(Debug, Clone)]
pub struct KeychainSecretStore {
    service: String,
    /// Refuses every call without asking the OS (see [`Self::disabled`]).
    disabled: bool,
}

impl Default for KeychainSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeychainSecretStore {
    /// Secrets under [`SERVICE`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_service(SERVICE)
    }

    /// Secrets under another service name, so a test can't touch the app's
    /// own items.
    #[must_use]
    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            disabled: false,
        }
    }

    /// A store that fails every call without touching the keychain, as on an
    /// OS with no adapter. Test fixtures use it where the type must be the
    /// real one (F1): no Keychain prompt, no read of the owner's items.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            disabled: true,
            ..Self::new()
        }
    }

    /// Why this store must not reach the OS, if it mustn't: it was made
    /// [`disabled`](Self::disabled), or the test harness set
    /// [`NO_LAUNCH_VAR`](crate::reveal::NO_LAUNCH_VAR), or this is the
    /// platform crate's own unit tests.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn blocked(&self) -> Result<(), SecretError> {
        if self.disabled || crate::reveal::launching_disabled() {
            return Err(SecretError::Store(
                "the keychain is disabled in tests (DICTATION_NO_LAUNCH)".into(),
            ));
        }
        Ok(())
    }

    /// The service name items are stored under.
    #[must_use]
    pub fn service(&self) -> &str {
        &self.service
    }
}

#[cfg(target_os = "macos")]
impl SecretStore for KeychainSecretStore {
    fn get(&self, name: &str) -> Result<Option<Secret>, SecretError> {
        self.blocked()?;
        match entry(&self.service, name)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(secret_error(&error)),
        }
    }

    fn set(&self, name: &str, secret: &Secret) -> Result<(), SecretError> {
        self.blocked()?;
        entry(&self.service, name)?
            .set_password(secret.expose())
            .map_err(|error| secret_error(&error))
    }

    fn delete(&self, name: &str) -> Result<(), SecretError> {
        self.blocked()?;
        match entry(&self.service, name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(secret_error(&error)),
        }
    }
}

#[cfg(target_os = "macos")]
fn entry(service: &str, name: &str) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(service, name).map_err(|error| secret_error(&error))
}

/// Maps a keyring error without ever including a secret: `BadEncoding`
/// holds the stored bytes and `Invalid` / `TooLong` echo arguments, so
/// they get fixed messages.
#[cfg(target_os = "macos")]
fn secret_error(error: &keyring::Error) -> SecretError {
    match error {
        keyring::Error::NoStorageAccess(_) => SecretError::AccessDenied,
        keyring::Error::BadEncoding(_) => SecretError::Store("the stored item isn't text".into()),
        keyring::Error::TooLong(attribute, _) => {
            SecretError::Store(format!("the {attribute} is too long"))
        }
        keyring::Error::Invalid(attribute, _) => {
            SecretError::Store(format!("the {attribute} is invalid"))
        }
        keyring::Error::PlatformFailure(source) => SecretError::Store(source.to_string()),
        _ => SecretError::Store("the keychain failed".into()),
    }
}

#[cfg(not(target_os = "macos"))]
impl SecretStore for KeychainSecretStore {
    fn get(&self, _name: &str) -> Result<Option<Secret>, SecretError> {
        Err(unavailable())
    }

    fn set(&self, _name: &str, _secret: &Secret) -> Result<(), SecretError> {
        Err(unavailable())
    }

    fn delete(&self, _name: &str) -> Result<(), SecretError> {
        Err(unavailable())
    }
}

#[cfg(not(target_os = "macos"))]
fn unavailable() -> SecretError {
    SecretError::Store("no keychain adapter for this OS yet".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_service_is_its_bundle_identifier() {
        assert_eq!(SERVICE, "com.dictationapp.desktop");
        assert_eq!(KeychainSecretStore::new().service(), SERVICE);
        assert_eq!(KeychainSecretStore::default().service(), SERVICE);
    }

    #[test]
    fn a_disabled_store_and_test_builds_refuse_without_asking_the_os() {
        let store = KeychainSecretStore::disabled();
        assert!(store.get("groq-api-key").is_err());
        assert!(store.set("groq-api-key", &Secret::new("x")).is_err());
        assert!(store.delete("groq-api-key").is_err());
        // cfg(test) refuses even a store that wasn't disabled.
        assert!(KeychainSecretStore::new().get("groq-api-key").is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn errors_never_carry_the_value() {
        let leaked = "dummy-not-a-real-key";
        let errors = [
            keyring::Error::BadEncoding(leaked.as_bytes().to_vec()),
            keyring::Error::Invalid("service".into(), leaked.into()),
            keyring::Error::TooLong("user".into(), 255),
            keyring::Error::NoEntry,
        ];
        for error in &errors {
            let mapped = secret_error(error);
            assert!(
                !format!("{mapped} {mapped:?}").contains(leaked),
                "{mapped:?}"
            );
        }
        assert_eq!(
            secret_error(&keyring::Error::NoStorageAccess("locked".into())),
            SecretError::AccessDenied
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unavailable_without_an_adapter() {
        let store = KeychainSecretStore::new();
        assert!(store.get("x").is_err());
        assert!(store.set("x", &Secret::new("y")).is_err());
        assert!(store.delete("x").is_err());
    }
}
