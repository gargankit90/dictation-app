use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use dictation_ports::{Secret, SecretError, SecretStore};

use super::lock;

/// A [`SecretStore`] held in memory. Tests use made-up keys only, never a
/// real one (`AGENTS.md`).
#[derive(Debug, Clone, Default)]
pub struct MemSecretStore {
    state: Arc<Mutex<State>>,
    /// While `true`, every call waits (see [`MemSecretStore::set_blocked`]).
    blocked: Arc<(Mutex<bool>, Condvar)>,
}

#[derive(Debug, Default)]
struct State {
    secrets: HashMap<String, Secret>,
    denied: bool,
    /// Makes every call fail with [`SecretError::Store`] and this text.
    failing: Option<String>,
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

    /// Makes every call fail with [`SecretError::Store`] saying `reason`
    /// (a locked keychain, say) until set back to `None`.
    pub fn set_failing(&self, reason: Option<&str>) {
        lock(&self.state).failing = reason.map(str::to_owned);
    }

    /// While `blocked`, every call waits, like a real keychain that is
    /// waiting for a person to unlock it or allow access. Setting it back
    /// to `false` lets the waiting calls go on.
    pub fn set_blocked(&self, blocked: bool) {
        let (flag, wake) = &*self.blocked;
        *lock(flag) = blocked;
        wake.notify_all();
    }

    fn with_access<T>(&self, f: impl FnOnce(&mut State) -> T) -> Result<T, SecretError> {
        {
            let (flag, wake) = &*self.blocked;
            let mut blocked = lock(flag);
            while *blocked {
                blocked = wake.wait(blocked).unwrap_or_else(PoisonError::into_inner);
            }
        }
        let mut state = lock(&self.state);
        if state.denied {
            return Err(SecretError::AccessDenied);
        }
        if let Some(reason) = &state.failing {
            return Err(SecretError::Store(reason.clone()));
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

    #[test]
    fn a_failing_store_fails_every_call_with_its_reason_until_cleared() {
        let store = MemSecretStore::new();
        store.set("groq", &Secret::new("kept")).unwrap();
        store.set_failing(Some("locked"));
        let locked = Err(SecretError::Store("locked".to_owned()));
        assert_eq!(store.get("groq"), locked);
        assert_eq!(store.set("groq", &Secret::new("lost")), locked.map(|_| ()));
        assert!(store.delete("groq").is_err());

        store.set_failing(None);
        assert_eq!(store.get("groq").unwrap(), Some(Secret::new("kept")));
    }

    #[test]
    fn a_blocked_store_waits_until_released() {
        let store = MemSecretStore::new();
        store.set("groq", &Secret::new("kept")).unwrap();
        store.set_blocked(true);
        let (done, finished) = std::sync::mpsc::channel();
        let reader = store.clone();
        let thread = std::thread::spawn(move || done.send(reader.get("groq")).unwrap());
        assert!(
            finished
                .recv_timeout(std::time::Duration::from_millis(200))
                .is_err(),
            "returned while blocked"
        );
        store.set_blocked(false);
        assert_eq!(
            finished
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap(),
            Ok(Some(Secret::new("kept")))
        );
        thread.join().unwrap();
    }
}
