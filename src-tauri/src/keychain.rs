//! Bounded keychain calls (T3, #113, #204).
//!
//! The keychain can block: a locked keychain, or an access prompt nobody
//! answers. Each read or write runs on a thread of its own and the caller
//! stops waiting at a deadline, so Settings gets an error it can show. A
//! call that timed out can't be cancelled, so there is at most one read and
//! one write in flight, app-wide ([`InFlight`]), and a stuck keychain costs
//! one parked thread however often the user tries again.
//!
//! - [`KeyReads`]: a read joins the one in flight; one started before the
//!   last save ([`KeyReads::invalidate`]) may hold the old key, so it is
//!   waited out and a new read starts.
//! - [`KeyWrites`]: a save waits for an earlier write still in flight
//!   (within its own deadline) and only then writes, so the newest key is
//!   written last. A stuck write isn't cancelled: if it lands it rebuilds
//!   the engine like any other save ([`crate::engine::changed`]).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use dictation_pipeline::config::{Config, ConfigError};
use dictation_ports::{Secret, SecretError, SecretStore};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One call on a thread of its own: its tag and, once it finishes, its
/// outcome.
struct Call<T> {
    tag: u64,
    outcome: Mutex<Option<T>>,
    finished: Condvar,
}

impl<T: Clone> Call<T> {
    /// The outcome, or `None` if `deadline` passes first.
    fn wait(&self, deadline: Instant) -> Option<T> {
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

/// At most one call in flight. Clones share it.
struct InFlight<T>(Arc<Mutex<Option<Arc<Call<T>>>>>);

impl<T> Clone for InFlight<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Default for InFlight<T> {
    fn default() -> Self {
        Self(Arc::default())
    }
}

/// What [`InFlight::start`] did.
enum Started<T, F> {
    /// It started this call.
    Ours(Arc<Call<T>>),
    /// Another call is in flight; `run` comes back unused.
    Busy(Arc<Call<T>>, F),
}

impl<T: Clone + Send + 'static> InFlight<T> {
    /// Starts `run` on a thread named `name`, tagged `tag`, unless a call is
    /// in flight. A panic in `run` finishes the call with `crashed`.
    fn start<F: FnOnce() -> T + Send + 'static>(
        &self,
        tag: u64,
        name: &str,
        run: F,
        crashed: fn() -> T,
    ) -> std::io::Result<Started<T, F>> {
        let mut slot = lock(&self.0);
        if let Some(call) = &*slot {
            return Ok(Started::Busy(Arc::clone(call), run));
        }
        let call = Arc::new(Call {
            tag,
            outcome: Mutex::new(None),
            finished: Condvar::new(),
        });
        let (shared, task) = (Arc::clone(&self.0), Arc::clone(&call));
        std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|_| crashed());
                // Finished and no longer in flight in one step, so nobody
                // waits on a finished call.
                let mut slot = lock(&shared);
                *slot = None;
                *lock(&task.outcome) = Some(outcome);
                task.finished.notify_all();
            })?;
        *slot = Some(Arc::clone(&call));
        Ok(Started::Ours(call))
    }
}

type ReadOutcome = Result<Option<Secret>, SecretError>;

/// Keychain reads of the API key, at most one in flight.
#[derive(Clone, Default)]
pub struct KeyReads {
    /// Bumped by every save; a read started before it is stale after it.
    generation: Arc<Mutex<u64>>,
    read: InFlight<ReadOutcome>,
}

impl KeyReads {
    /// Marks every read in flight as stale: call it after a save, before the
    /// rebuild that should see what was saved.
    pub fn invalidate(&self) {
        *lock(&self.generation) += 1;
    }

    /// The key from the keychain, waiting at most `timeout` (from now) for
    /// it. Blocks.
    pub(crate) fn read<S: SecretStore + 'static>(
        &self,
        config: &Arc<Config<S>>,
        timeout: Duration,
    ) -> ReadOutcome {
        let deadline = Instant::now() + timeout;
        let wanted = *lock(&self.generation);
        loop {
            let reader = Arc::clone(config);
            let started = self
                .read
                .start(
                    wanted,
                    "keychain-read",
                    move || match reader.api_key() {
                        Ok(key) => Ok(key),
                        Err(ConfigError::Secret(error)) => Err(error),
                        // `api_key` only fails in the keychain; ConfigError
                        // never holds a key.
                        Err(other) => Err(SecretError::Store(other.to_string())),
                    },
                    || {
                        Err(SecretError::Store(
                            "the keychain read stopped unexpectedly".to_owned(),
                        ))
                    },
                )
                .map_err(|error| {
                    SecretError::Store(format!("could not start the keychain read: {error}"))
                })?;
            let (Started::Ours(read) | Started::Busy(read, _)) = started;
            let Some(answer) = read.wait(deadline) else {
                return Err(SecretError::Store(format!(
                    "no answer from the keychain after {} s (locked, or waiting for access to be allowed)",
                    timeout.as_secs()
                )));
            };
            if read.tag >= wanted {
                return answer;
            }
            // A stale answer: it's finished now, so the next pass starts a
            // read of its own (the only one in flight).
        }
    }
}

type WriteOutcome = Result<(), ConfigError>;

/// Keychain writes, at most one in flight. Clones share it.
#[derive(Clone, Default)]
pub struct KeyWrites {
    write: InFlight<WriteOutcome>,
    /// How many saves have started waiting for an earlier write, so a test
    /// can tell a save is queued without sleeping.
    queued: Arc<(Mutex<usize>, Condvar)>,
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
    /// and waits at most `timeout` (from now) for it. Blocks.
    ///
    /// # Errors
    ///
    /// `write`'s own error, or that the keychain didn't answer in time (then
    /// `write` may still be running, or may never have started).
    pub fn write(
        &self,
        timeout: Duration,
        write: impl FnOnce() -> WriteOutcome + Send + 'static,
    ) -> WriteOutcome {
        let deadline = Instant::now() + timeout;
        let mut write = write;
        loop {
            let crashed = || {
                Err(ConfigError::Secret(SecretError::Store(
                    "the keychain write stopped unexpectedly".to_owned(),
                )))
            };
            let started = self
                .write
                .start(0, "keychain-write", write, crashed)
                .map_err(|error| {
                    ConfigError::Secret(SecretError::Store(format!(
                        "could not start the keychain write: {error}"
                    )))
                })?;
            match started {
                Started::Ours(call) => {
                    return call
                        .wait(deadline)
                        .unwrap_or_else(|| Err(no_answer(timeout)));
                }
                Started::Busy(earlier, unused) => {
                    self.note_queued();
                    // Its outcome, success or not, isn't ours: only whether
                    // it finished in time to start ours.
                    if earlier.wait(deadline).is_none() {
                        return Err(no_answer(timeout));
                    }
                    write = unused;
                }
            }
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
    fn held(
        entered: mpsc::Sender<u32>,
        id: u32,
    ) -> (impl FnOnce() -> WriteOutcome, mpsc::Sender<()>) {
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
