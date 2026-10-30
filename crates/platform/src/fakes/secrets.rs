use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use dictation_ports::{Secret, SecretError, SecretStore};

use super::lock;

/// A [`SecretStore`] held in memory. Tests use made-up keys only, never a
/// real one (`AGENTS.md`).
#[derive(Debug, Clone, Default)]
pub struct MemSecretStore {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    secrets: HashMap<String, Secret>,
    denied: bool,
}

impl MemSecretStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes every call fail with [`SecretError::AccessDenied`] (the user
    /// refused the keychain prompt) until set back to `false`.
    pub fn set_access_denied(&self, denied: bool) {
        lock(&self.state).denied = denied;
    }

    fn with_access<T>(&self, f: impl FnOnce(&mut State) -> T) -> Result<T, SecretError> {
        let mut state = lock(&self.state);
        if state.denied {
            return Err(SecretError::AccessDenied);
        }
        Ok(f(&mut state))
    }
}

impl SecretStore for MemSecretStore {
    fn get(&self, name: &str) -> Result<Option<Secret>, SecretError> {
        self.with_access(|state| state.secrets.get(name).cloned())
    }

    fn set(&self, name: &str, secret: &Secret) -> Result<(), SecretError> {
        self.with_access(|state| {
            state.secrets.insert(name.to_owned(), secret.clone());
        })
    }

    fn delete(&self, name: &str) -> Result<(), SecretError> {
        self.with_access(|state| {
            state.secrets.remove(name);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_replace_delete() {
        let store = MemSecretStore::new();
        assert_eq!(store.get("groq").unwrap(), None);

        store.set("groq", &Secret::new("fake-1")).unwrap();
        store.set("groq", &Secret::new("fake-2")).unwrap();
        store.set("openai", &Secret::new("fake-3")).unwrap();
        assert_eq!(store.get("groq").unwrap(), Some(Secret::new("fake-2")));

        store.delete("groq").unwrap();
        assert_eq!(store.get("groq").unwrap(), None);
        assert_eq!(store.get("openai").unwrap(), Some(Secret::new("fake-3")));
    }

    #[test]
    fn deleting_a_missing_secret_succeeds() {
        assert_eq!(MemSecretStore::new().delete("nothing"), Ok(()));
    }

    #[test]
    fn denied_access_fails_every_call_and_keeps_the_previous_value() {
        let store = MemSecretStore::new();
        store.set("groq", &Secret::new("kept")).unwrap();
        store.set_access_denied(true);
        assert_eq!(store.get("groq"), Err(SecretError::AccessDenied));
        assert_eq!(
            store.set("groq", &Secret::new("lost")),
            Err(SecretError::AccessDenied)
        );
        assert_eq!(store.delete("groq"), Err(SecretError::AccessDenied));

        store.set_access_denied(false);
        assert_eq!(store.get("groq").unwrap(), Some(Secret::new("kept")));
    }
}
