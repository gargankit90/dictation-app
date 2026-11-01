//! Bounded keychain writes (T3, #113).
//!
//! Saving an API key writes the keychain, which can block like a read: a
//! locked keychain, or an access prompt nobody answers. [`KeyWrites`] runs
//! each write on a thread of its own and stops waiting at a deadline, so
//! Settings gets an error it can show and its form unlocks.
//!
//! A write that timed out can't be cancelled; it may still land later. So
//! there is at most one write in flight, app-wide, and that is what a retry
//! means:
//!
//! - a save while an earlier write is still in flight waits for it (within
//!   the save's own deadline) and only then writes, so the newest key is
//!   always written last and a late write can't overwrite it;
//! - if the earlier write still hasn't finished by then, the save fails
//!   without starting anything, so a stuck keychain costs one parked thread
//!   however often the user tries again.
//!
//! The error says the key may not be saved: the stuck write isn't cancelled,
//! and if it lands it rebuilds the engine like any other save
//! ([`crate::engine::changed`]), so dictation uses what the keychain holds.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use dictation_pipeline::config::ConfigError;
use dictation_ports::SecretError;

type Outcome = Result<(), ConfigError>;

/// Keychain writes with at most one in flight. Clones share it.
#[derive(Clone, Default)]
pub struct KeyWrites {
    in_flight: Arc<Mutex<Option<Arc<Write>>>>,
    /// How many saves have started waiting for an earlier write, so a test
    /// can tell a save is queued without sleeping.
    queued: Arc<(Mutex<usize>, Condvar)>,
}

/// One write; its outcome once it finishes.
#[derive(Default)]
struct Write {
    outcome: Mutex<Option<Outcome>>,
    finished: Condvar,
}

impl Write {
    /// The outcome, or `None` if `deadline` passes first.
    fn wait(&self, deadline: Instant) -> Option<Outcome> {
        let mut outcome = lock(&self.outcome);
        loop {
            if let Some(done) = &*outcome {
                return Some(done.clone());
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            outcome = self
                .finished
                .wait_timeout(outcome, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The error for a write that didn't finish in time. Never holds the key.
fn no_answer(timeout: Duration) -> ConfigError {
    ConfigError::Secret(SecretError::Store(format!(
        "no answer from the keychain after {} s, so the key may not be saved (locked, or waiting for access to be allowed). Unlock it or allow access, then save again",
        timeout.as_secs()
    )))
}

impl KeyWrites {
    /// Runs `write` on a thread of its own, after any write still in flight,
    /// and waits at most `timeout` (from now) for it. Blocks: call it from a
    /// blocking task.
    ///
    /// # Errors
    ///
    /// `write`'s own error, or that the keychain didn't answer in time (then
    /// `write` may still be running, or may never have started).
    pub fn write(
        &self,
        timeout: Duration,
        write: impl FnOnce() -> Outcome + Send + 'static,
    ) -> Outcome {
        let deadline = Instant::now() + timeout;
        loop {
            let mut slot = lock(&self.in_flight);
            if let Some(earlier) = slot.clone() {
                drop(slot);
                self.note_queued();
                // Its outcome, success or not, isn't ours: only whether it
                // finished in time to start ours.
                if earlier.wait(deadline).is_none() {
                    return Err(no_answer(timeout));
                }
                continue;
            }
            let ours = Arc::new(Write::default());
            let (shared, task) = (Arc::clone(&self.in_flight), Arc::clone(&ours));
            std::thread::Builder::new()
                .name("keychain-write".to_owned())
                .spawn(move || {
                    let outcome = catch_unwind(AssertUnwindSafe(write)).unwrap_or_else(|_| {
                        Err(ConfigError::Secret(SecretError::Store(
                            "the keychain write stopped unexpectedly".to_owned(),
                        )))
                    });
                    // Finished and no longer in flight in one step, so a
                    // queued save never waits on a finished write.
                    let mut slot = lock(&shared);
                    *slot = None;
                    *lock(&task.outcome) = Some(outcome);
                    task.finished.notify_all();
                })
                .map_err(|error| {
                    ConfigError::Secret(SecretError::Store(format!(
                        "could not start the keychain write: {error}"
                    )))
                })?;
            *slot = Some(Arc::clone(&ours));
            drop(slot);
            return ours
                .wait(deadline)
                .unwrap_or_else(|| Err(no_answer(timeout)));
        }
    }

    fn note_queued(&self) {
        *lock(&self.queued.0) += 1;
        self.queued.1.notify_all();
    }

    /// Waits until `count` saves have queued behind an earlier write, or
    /// `limit` passes; whether they did.
    #[cfg(test)]
    pub(crate) fn wait_queued(&self, count: usize, limit: Duration) -> bool {
        let (queued, changed) = &*self.queued;
        let (queued, _) = changed
            .wait_timeout_while(lock(queued), limit, |queued| *queued < count)
            .unwrap_or_else(PoisonError::into_inner);
        *queued >= count
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    const LIMIT: Duration = Duration::from_secs(20);

    /// A write that waits for its release, saying when it starts.
    fn held(entered: mpsc::Sender<u32>, id: u32) -> (impl FnOnce() -> Outcome, mpsc::Sender<()>) {
        let (release, gate) = mpsc::channel::<()>();
        let write = move || {
            let _ = entered.send(id);
            let _ = gate.recv();
            Ok(())
        };
        (write, release)
    }

    #[test]
    fn a_stuck_write_times_out_and_a_retry_starts_nothing_until_it_finishes() {
        let writes = KeyWrites::default();
        let (entered, started) = mpsc::channel();
        let (write, release) = held(entered.clone(), 1);
        let error = writes.write(Duration::from_millis(50), write).unwrap_err();
        assert!(error.to_string().contains("may not be saved"), "{error}");
        assert_eq!(started.recv_timeout(LIMIT), Ok(1));

        let ran = Arc::new(Mutex::new(false));
        let flag = Arc::clone(&ran);
        let retry = writes.write(Duration::from_millis(50), move || {
            *lock(&flag) = true;
            Ok(())
        });
        assert!(retry.is_err());
        assert!(!*lock(&ran), "nothing started behind the stuck write");

        release.send(()).unwrap();
        let (write, release) = held(entered, 2);
        release.send(()).unwrap();
        assert_eq!(writes.write(LIMIT, write), Ok(()));
        assert_eq!(started.recv_timeout(LIMIT), Ok(2));
    }

    #[test]
    fn a_panicking_write_is_an_error_and_frees_the_slot() {
        let writes = KeyWrites::default();
        let error = writes.write(LIMIT, || panic!("boom")).unwrap_err();
        assert!(error.to_string().contains("stopped unexpectedly"));
        assert_eq!(writes.write(LIMIT, || Ok(())), Ok(()));
    }
}
