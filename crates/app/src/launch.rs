//! The launch-at-login setting (M5.3): what the settings screen shows and
//! what saving does to the OS.
//!
//! The settings file keeps the last value applied, but the user can switch
//! the login item off in System Settings, so the screen always shows the OS's
//! own state ([`with_os_state`]). Changing the OS can take a long time
//! (unregistering the login item has taken 40 s, #238), so the settings save
//! never does it: the checkbox asks [`LoginSync`], which changes the OS on a
//! thread of its own, one change at a time, and reports what the OS ended up
//! with.

use std::sync::{Arc, Mutex};

use dictation_core::settings::Settings;
use dictation_ports::{LaunchAtLogin, LaunchAtLoginError, LaunchAtLoginState};
use thiserror::Error;
use tokio::sync::oneshot;

/// Why saving the launch-at-login choice didn't take effect. The text is for
/// the user.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LaunchProblem {
    /// This OS has no login item support yet.
    #[error("Launch at login isn't available on this system yet.")]
    Unsupported,
    /// The OS refused.
    #[error("Couldn't change launch at login: {0}")]
    Os(String),
    /// Registered, but the user has to allow it in the OS settings.
    #[error(
        "Launch at login needs your approval: open System Settings, General, Login Items, and allow dictation-app."
    )]
    NeedsApproval,
    /// The OS said yes, but its state didn't change.
    #[error("The system didn't change launch at login. Try again.")]
    NotApplied,
    /// The OS changed, but the setting couldn't be written to the settings
    /// file (S1 review, R2).
    #[error(
        "Launch at login is {} now, but the setting couldn't be saved ({reason}). Check that dictation-app's settings folder can be written, then try again.",
        if *enabled { "on" } else { "off" }
    )]
    NotStored {
        /// What the OS has now.
        enabled: bool,
        /// Why the write failed.
        reason: String,
    },
}

impl From<LaunchAtLoginError> for LaunchProblem {
    fn from(error: LaunchAtLoginError) -> Self {
        match error {
            LaunchAtLoginError::Unsupported => Self::Unsupported,
            LaunchAtLoginError::Os(message) => Self::Os(message),
        }
    }
}

/// `saved` with `launch_at_login` set to what the OS says. If the OS can't
/// say, the saved value stays.
#[must_use]
pub fn with_os_state(login: &dyn LaunchAtLogin, saved: Settings) -> Settings {
    reconcile(saved, login.state())
}

fn reconcile(
    mut saved: Settings,
    state: Result<LaunchAtLoginState, LaunchAtLoginError>,
) -> Settings {
    match state {
        Ok(state) => {
            if state.is_enabled() != saved.launch_at_login {
                tracing::info!(
                    saved = saved.launch_at_login,
                    os = state.is_enabled(),
                    "launch at login was changed outside the app"
                );
            }
            saved.launch_at_login = state.is_enabled();
        }
        Err(error) => tracing::debug!(%error, "can't read the login item; using the saved value"),
    }
    saved
}

/// Stores an outcome; the error says why it couldn't.
type StoreFn = dyn Fn(&Outcome) -> Result<(), String> + Send + Sync;

/// How long the screen waits for the OS to say if the login item is on.
pub const READ_LIMIT: std::time::Duration = std::time::Duration::from_millis(500);

/// What a change of the login item ended in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Whether the app starts at login now, as the OS says (or, if it can't
    /// say, as it was before).
    pub enabled: bool,
    /// Why the wish didn't come true, if it didn't.
    pub problem: Option<LaunchProblem>,
}

/// Makes the OS match `wanted` and says what it really ended up with, so a
/// stored flag never claims more than is true. Does nothing to the OS if it
/// already matches.
#[must_use]
pub fn change(login: &dyn LaunchAtLogin, wanted: bool) -> Outcome {
    let before = login.state().ok();
    // A registered item the user hasn't approved is cleaned up when they
    // switch the setting off, and tried again when they switch it on.
    let in_sync = match before {
        Some(LaunchAtLoginState::Enabled) => wanted,
        Some(LaunchAtLoginState::Disabled) => !wanted,
        Some(LaunchAtLoginState::RequiresApproval) => false,
        // The OS can't say (no adapter, say): nothing to switch off.
        None => !wanted,
    };
    let was = before.is_some_and(LaunchAtLoginState::is_enabled);
    if in_sync {
        return Outcome {
            enabled: was,
            problem: None,
        };
    }
    if let Err(error) = login.set_enabled(wanted) {
        return Outcome {
            enabled: was,
            problem: Some(error.into()),
        };
    }
    let after = match login.state() {
        Ok(after) => after,
        Err(error) => {
            return Outcome {
                enabled: was,
                problem: Some(error.into()),
            };
        }
    };
    let problem = match after {
        LaunchAtLoginState::RequiresApproval if wanted => Some(LaunchProblem::NeedsApproval),
        // Switching off leaves an unapproved item the user must remove
        // themselves; nothing will start, which is what they asked for.
        LaunchAtLoginState::RequiresApproval => None,
        state if state.is_enabled() == wanted => None,
        _ => Some(LaunchProblem::NotApplied),
    };
    Outcome {
        enabled: after.is_enabled(),
        problem,
    }
}

/// [`change`] for `settings.launch_at_login`, which then holds what the OS
/// ended up with.
///
/// # Errors
///
/// A [`LaunchProblem`] saying why the wish didn't come true; `settings` holds
/// the OS's actual state anyway.
pub fn apply(login: &dyn LaunchAtLogin, settings: &mut Settings) -> Result<(), LaunchProblem> {
    let outcome = change(login, settings.launch_at_login);
    settings.launch_at_login = outcome.enabled;
    outcome.problem.map_or(Ok(()), Err)
}

#[derive(Default)]
struct Pending {
    /// The latest choice not yet applied.
    wanted: Option<bool>,
    /// The choice the OS is being asked for right now.
    applying: Option<bool>,
    /// Whether the worker thread is running.
    running: bool,
    /// Everyone waiting for the final outcome.
    waiters: Vec<oneshot::Sender<Outcome>>,
}

/// Changes the login item off the caller's thread, one change at a time, the
/// latest choice winning (S1, #238).
///
/// Every [`request`](Self::request) returns at once with a receiver. If
/// the user toggles again while a change runs, the older answers are
/// skipped: after the running OS call, only the newest wish is applied, and
/// every waiting receiver gets that last outcome, so nothing arrives out of
/// order and the final state is the last choice.
#[derive(Clone)]
pub struct LoginSync {
    login: Arc<dyn LaunchAtLogin>,
    /// What the OS last said, whether the app asked or the screen did.
    known: Arc<Mutex<Option<LaunchAtLoginState>>>,
    /// Called with each final outcome before the receivers are told, with
    /// no other change running or starting: stores the result.
    on_done: Arc<StoreFn>,
    pending: Arc<Mutex<Pending>>,
}

impl LoginSync {
    /// A sync over `login`; `on_done` is called with every final outcome.
    pub fn new(
        login: Arc<dyn LaunchAtLogin>,
        on_done: impl Fn(&Outcome) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            login,
            known: Arc::new(Mutex::new(None)),
            on_done: Arc::new(on_done),
            pending: Arc::new(Mutex::new(Pending::default())),
        }
    }

    /// `saved` with `launch_at_login` as the screen should show it, without
    /// waiting long for the OS: reading the state has taken 8 s (just after
    /// a boot) and blocks while the OS unregisters, which would hold up the
    /// settings window (S1, #238). While a change is pending the screen shows
    /// that choice; otherwise the OS's state if it answers within
    /// [`READ_LIMIT`], else the last state it gave (the late answer is kept
    /// for the next call), else `saved`.
    #[must_use]
    pub fn shown(&self, mut saved: Settings) -> Settings {
        if let Some(wanted) = self.pending() {
            saved.launch_at_login = wanted;
            return saved;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let (login, known) = (Arc::clone(&self.login), Arc::clone(&self.known));
        let spawned = std::thread::Builder::new()
            .name("login-item-state".into())
            .spawn(move || {
                let state = login.state();
                if let Ok(state) = state {
                    *known
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(state);
                }
                let _ = sender.send(state);
            });
        let state = match spawned {
            Ok(_) => receiver.recv_timeout(READ_LIMIT).ok(),
            Err(_) => None,
        };
        let state = state.unwrap_or_else(|| {
            tracing::info!(
                "the OS is slow to say if the login item is on; using what it said last"
            );
            let known = *self
                .known
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            known.ok_or(LaunchAtLoginError::Unsupported)
        });
        reconcile(saved, state)
    }

    /// Asks for the login item to be `enabled`. Returns without waiting for
    /// the OS; the receiver gets the outcome of the last choice made by the
    /// time the OS is idle. It yields nothing if the worker couldn't start.
    pub fn request(&self, enabled: bool) -> oneshot::Receiver<Outcome> {
        let (sender, receiver) = oneshot::channel();
        let start = {
            let mut pending = lock(&self.pending);
            pending.wanted = Some(enabled);
            pending.waiters.push(sender);
            !std::mem::replace(&mut pending.running, true)
        };
        if start {
            let (login, on_done, pending, known) = (
                Arc::clone(&self.login),
                Arc::clone(&self.on_done),
                Arc::clone(&self.pending),
                Arc::clone(&self.known),
            );
            let spawned = std::thread::Builder::new()
                .name("login-item".into())
                .spawn(move || work(&*login, &*on_done, &pending, &known));
            if let Err(error) = spawned {
                tracing::warn!(%error, "could not start the login item thread");
                let mut pending = lock(&self.pending);
                pending.running = false;
                pending.wanted = None;
                pending.waiters.clear();
            }
        }
        receiver
    }

    /// The choice still waiting to be applied or being applied, if any: what
    /// the checkbox should show while the OS works.
    #[must_use]
    pub fn pending(&self) -> Option<bool> {
        let pending = lock(&self.pending);
        pending.wanted.or(pending.applying)
    }
}

fn lock(pending: &Mutex<Pending>) -> std::sync::MutexGuard<'_, Pending> {
    pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn work(
    login: &dyn LaunchAtLogin,
    on_done: &StoreFn,
    shared: &Mutex<Pending>,
    known: &Mutex<Option<LaunchAtLoginState>>,
) {
    loop {
        let wanted = {
            let mut pending = lock(shared);
            let Some(wanted) = pending.wanted.take() else {
                // Decided under the lock a request takes, so a request can't
                // slip in between "nothing left" and "not running".
                pending.running = false;
                pending.applying = None;
                return;
            };
            pending.applying = Some(wanted);
            wanted
        };
        let started = std::time::Instant::now();
        let mut outcome = change(login, wanted);
        tracing::info!(
            wanted,
            enabled = outcome.enabled,
            elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "login item changed"
        );
        // What the change read last; no second read of the OS, and never one
        // under the lock (S1 review, R1: it takes seconds).
        *known
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(if outcome.enabled {
            LaunchAtLoginState::Enabled
        } else {
            LaunchAtLoginState::Disabled
        });
        let waiters = {
            let mut pending = lock(shared);
            if pending.wanted.is_some() {
                // Toggled again meanwhile: this outcome is stale.
                continue;
            }
            std::mem::take(&mut pending.waiters)
        };
        // Stored outside the lock, with `running` still set: a newer request
        // only queues, so it can't overtake the store, and nobody waits on
        // the settings file here.
        if let Err(reason) = on_done(&outcome) {
            tracing::warn!(%reason, "could not store the launch-at-login result");
            if outcome.problem.is_none() {
                outcome.problem = Some(LaunchProblem::NotStored {
                    enabled: outcome.enabled,
                    reason,
                });
            }
        }
        for waiter in waiters {
            let _ = waiter.send(outcome.clone());
        }
        let mut pending = lock(shared);
        if pending.wanted.is_none() {
            pending.running = false;
            pending.applying = None;
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use dictation_platform::fakes::FakeLaunchAtLogin;

    use super::*;

    fn settings(launch_at_login: bool) -> Settings {
        Settings {
            launch_at_login,
            ..Settings::default()
        }
    }

    #[test]
    fn the_screen_shows_the_os_state_not_the_saved_flag() {
        let login = FakeLaunchAtLogin::new();
        // Saved on, but the user removed it in System Settings.
        assert!(!with_os_state(&login, settings(true)).launch_at_login);
        login.set_state(LaunchAtLoginState::Enabled);
        assert!(with_os_state(&login, settings(false)).launch_at_login);
        // Awaiting approval is not on.
        login.set_state(LaunchAtLoginState::RequiresApproval);
        assert!(!with_os_state(&login, settings(true)).launch_at_login);
    }

    #[test]
    fn when_the_os_cannot_say_the_saved_flag_stays() {
        let login = FakeLaunchAtLogin::new();
        login.fail_with(LaunchAtLoginError::Unsupported);
        assert!(with_os_state(&login, settings(true)).launch_at_login);
        assert!(!with_os_state(&login, settings(false)).launch_at_login);
    }

    #[test]
    fn turning_it_on_and_off_registers_and_unregisters() {
        let login = FakeLaunchAtLogin::new();
        let mut on = settings(true);
        apply(&login, &mut on).unwrap();
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Enabled));
        assert!(on.launch_at_login);

        let mut off = settings(false);
        apply(&login, &mut off).unwrap();
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Disabled));
        assert!(!off.launch_at_login);
        assert_eq!(login.calls(), [true, false]);
    }

    #[test]
    fn an_unchanged_choice_does_not_touch_the_os() {
        let login = FakeLaunchAtLogin::new();
        apply(&login, &mut settings(false)).unwrap();
        login.set_state(LaunchAtLoginState::Enabled);
        apply(&login, &mut settings(true)).unwrap();
        assert!(login.calls().is_empty());
    }

    #[test]
    fn an_os_refusal_is_reported_and_the_flag_keeps_the_real_state() {
        let login = FakeLaunchAtLogin::new();
        login.set_state(LaunchAtLoginState::Enabled);
        let mut off = settings(false);
        // set_enabled fails but state() still answers: script that with a
        // failing fake that is cleared for the read.
        let broken = RefusingLogin(login.clone());
        let error = apply(&broken, &mut off).unwrap_err();
        assert_eq!(error, LaunchProblem::Os("no bundle".into()));
        assert!(error.to_string().contains("no bundle"));
        assert!(off.launch_at_login, "still on in the OS");
    }

    /// Reads fine, refuses every change.
    struct RefusingLogin(FakeLaunchAtLogin);

    impl LaunchAtLogin for RefusingLogin {
        fn state(&self) -> Result<LaunchAtLoginState, LaunchAtLoginError> {
            self.0.state()
        }

        fn set_enabled(&self, _: bool) -> Result<(), LaunchAtLoginError> {
            Err(LaunchAtLoginError::Os("no bundle".into()))
        }
    }

    #[test]
    fn needing_approval_is_an_error_and_not_on() {
        let login = FakeLaunchAtLogin::new();
        login.enabling_gives(LaunchAtLoginState::RequiresApproval);
        let mut on = settings(true);
        assert_eq!(apply(&login, &mut on), Err(LaunchProblem::NeedsApproval));
        assert!(!on.launch_at_login);
        assert!(
            LaunchProblem::NeedsApproval
                .to_string()
                .contains("Login Items")
        );
    }

    #[test]
    fn switching_off_an_unapproved_item_removes_it() {
        let login = FakeLaunchAtLogin::new();
        login.set_state(LaunchAtLoginState::RequiresApproval);
        let mut off = settings(false);
        apply(&login, &mut off).unwrap();
        assert_eq!(login.calls(), [false]);
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Disabled));
    }

    #[test]
    fn no_adapter_means_unsupported() {
        let login = FakeLaunchAtLogin::new();
        login.fail_with(LaunchAtLoginError::Unsupported);
        let mut on = settings(true);
        assert_eq!(apply(&login, &mut on), Err(LaunchProblem::Unsupported));
        assert!(!on.launch_at_login);
        // Off needs nothing from an OS that can't say, so saving other
        // settings there works.
        let mut off = settings(false);
        apply(&login, &mut off).unwrap();
        assert_eq!(
            login.calls(),
            [true],
            "only the failed attempt to turn it on"
        );
    }

    #[test]
    fn a_state_that_does_not_change_is_reported() {
        let login = FakeLaunchAtLogin::new();
        login.enabling_gives(LaunchAtLoginState::Disabled);
        let mut on = settings(true);
        assert_eq!(apply(&login, &mut on), Err(LaunchProblem::NotApplied));
        assert!(!on.launch_at_login);
    }

    use std::time::Duration;

    fn synced(login: &FakeLaunchAtLogin) -> (LoginSync, Arc<Mutex<Vec<Outcome>>>) {
        let stored = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&stored);
        let sync = LoginSync::new(Arc::new(login.clone()), move |outcome| {
            sink.lock().unwrap().push(outcome.clone());
            Ok(())
        });
        (sync, stored)
    }

    #[tokio::test]
    async fn a_request_returns_before_the_slow_os_call_ends() {
        let login = FakeLaunchAtLogin::new();
        login.set_state(LaunchAtLoginState::Enabled);
        login.delay_changes(Duration::from_millis(400));
        let (sync, stored) = synced(&login);
        let started = std::time::Instant::now();
        let outcome = sync.request(false);
        assert!(started.elapsed() < Duration::from_millis(200));
        assert_eq!(sync.pending(), Some(false));
        assert!(stored.lock().unwrap().is_empty());
        let outcome = outcome.await.unwrap();
        assert_eq!(
            outcome,
            Outcome {
                enabled: false,
                problem: None
            }
        );
        assert_eq!(sync.pending(), None);
        assert_eq!(stored.lock().unwrap().as_slice(), [outcome]);
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Disabled));
    }

    #[tokio::test]
    async fn toggling_again_while_pending_ends_on_the_last_choice() {
        let login = FakeLaunchAtLogin::new();
        login.delay_changes(Duration::from_millis(300));
        let (sync, stored) = synced(&login);
        let first = sync.request(true);
        // The first change is running in the OS; now off, on, off, on.
        std::thread::sleep(Duration::from_millis(50));
        let rest = [sync.request(false), sync.request(true), sync.request(false)];
        let last = sync.request(true);
        let wanted = Outcome {
            enabled: true,
            problem: None,
        };
        assert_eq!(first.await.unwrap(), wanted);
        for outcome in rest {
            assert_eq!(outcome.await.unwrap(), wanted);
        }
        assert_eq!(last.await.unwrap(), wanted);
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Enabled));
        // One answer, not one per click; the OS only saw the running change
        // (the later choices end where it already is).
        assert_eq!(stored.lock().unwrap().as_slice(), [wanted]);
        assert_eq!(login.calls(), [true]);
    }

    #[tokio::test]
    async fn the_last_choice_wins_when_it_differs_from_the_running_change() {
        let login = FakeLaunchAtLogin::new();
        login.delay_changes(Duration::from_millis(300));
        let (sync, _) = synced(&login);
        let first = sync.request(true);
        std::thread::sleep(Duration::from_millis(50));
        let second = sync.request(false);
        let done = second.await.unwrap();
        assert_eq!(first.await.unwrap(), done);
        assert!(!done.enabled);
        assert_eq!(login.state(), Ok(LaunchAtLoginState::Disabled));
        assert_eq!(login.calls(), [true, false]);
    }

    #[tokio::test]
    async fn a_refusal_comes_back_as_the_problem_with_the_real_state() {
        let login = FakeLaunchAtLogin::new();
        login.enabling_gives(LaunchAtLoginState::RequiresApproval);
        let (sync, stored) = synced(&login);
        let outcome = sync.request(true).await.unwrap();
        assert!(!outcome.enabled);
        let problem = outcome.problem.unwrap();
        assert_eq!(problem, LaunchProblem::NeedsApproval);
        assert!(problem.to_string().contains("General, Login Items"));
        assert_eq!(stored.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn the_next_request_after_an_answer_starts_a_new_change() {
        let login = FakeLaunchAtLogin::new();
        let (sync, stored) = synced(&login);
        assert!(sync.request(true).await.unwrap().enabled);
        // Changed behind the app's back, then the user asks again.
        login.set_state(LaunchAtLoginState::Disabled);
        assert!(sync.request(true).await.unwrap().enabled);
        assert!(!sync.request(false).await.unwrap().enabled);
        assert_eq!(stored.lock().unwrap().len(), 3);
        assert_eq!(login.calls(), [true, true, false]);
    }

    /// A login item whose state reads are slow.
    struct SlowRead(FakeLaunchAtLogin, Duration);

    impl LaunchAtLogin for SlowRead {
        fn state(&self) -> Result<LaunchAtLoginState, LaunchAtLoginError> {
            std::thread::sleep(self.1);
            self.0.state()
        }

        fn set_enabled(&self, enabled: bool) -> Result<(), LaunchAtLoginError> {
            self.0.set_enabled(enabled)
        }
    }

    #[test]
    fn the_screen_does_not_wait_for_a_slow_state_read() {
        let fake = FakeLaunchAtLogin::new();
        fake.set_state(LaunchAtLoginState::Enabled);
        let sync = LoginSync::new(Arc::new(SlowRead(fake, READ_LIMIT * 4)), |_: &Outcome| {
            Ok(())
        });
        let started = std::time::Instant::now();
        // Nothing known yet: the saved value shows, after the limit.
        let shown = sync.shown(settings(false));
        assert!(
            started.elapsed() < READ_LIMIT * 3,
            "{:?}",
            started.elapsed()
        );
        assert!(!shown.launch_at_login);
        // The late answer was kept: the next look has it.
        std::thread::sleep(READ_LIMIT * 5);
        assert!(sync.shown(settings(false)).launch_at_login);
    }

    #[test]
    fn the_screen_shows_the_pending_choice_without_asking_the_os() {
        let fake = FakeLaunchAtLogin::new();
        fake.set_state(LaunchAtLoginState::Enabled);
        fake.delay_changes(Duration::from_millis(400));
        let (sync, _) = synced(&fake);
        let answer = sync.request(false);
        let started = std::time::Instant::now();
        assert!(!sync.shown(settings(true)).launch_at_login);
        assert!(started.elapsed() < Duration::from_millis(100));
        drop(answer);
    }

    #[test]
    fn a_quick_read_is_the_os_state() {
        let fake = FakeLaunchAtLogin::new();
        let (sync, _) = synced(&fake);
        assert!(!sync.shown(settings(true)).launch_at_login);
        fake.set_state(LaunchAtLoginState::Enabled);
        assert!(sync.shown(settings(false)).launch_at_login);
    }

    /// Blocks the third `state()` call (the one a change used to make under
    /// the lock, after its own two reads) until released.
    struct GatedRead {
        inner: FakeLaunchAtLogin,
        reads: std::sync::atomic::AtomicUsize,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl LaunchAtLogin for GatedRead {
        fn state(&self) -> Result<LaunchAtLoginState, LaunchAtLoginError> {
            let n = self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if n >= 3 {
                let _ = self
                    .release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5));
            }
            self.inner.state()
        }

        fn set_enabled(&self, enabled: bool) -> Result<(), LaunchAtLoginError> {
            self.inner.set_enabled(enabled)
        }
    }

    /// S1 review, R1: a slow read of the OS never holds the lock a new
    /// request and the screen need.
    #[test]
    fn a_slow_os_read_after_a_change_does_not_block_requests_or_the_screen() {
        let (release, gate) = std::sync::mpsc::channel();
        let login = Arc::new(GatedRead {
            inner: FakeLaunchAtLogin::new(),
            reads: std::sync::atomic::AtomicUsize::new(0),
            release: Mutex::new(gate),
        });
        let sync = LoginSync::new(
            Arc::clone(&login) as Arc<dyn LaunchAtLogin>,
            |_: &Outcome| Ok(()),
        );
        let first = sync.request(true);
        // The change has made its two reads and its change.
        for _ in 0..200 {
            if login.reads.load(std::sync::atomic::Ordering::SeqCst) >= 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(100));
        let (done, finished) = std::sync::mpsc::channel();
        let worker = sync.clone();
        std::thread::spawn(move || {
            let again = worker.request(true);
            let shown = worker.shown(settings(false));
            let _ = done.send(shown.launch_at_login);
            drop(again);
        });
        let answered = finished.recv_timeout(Duration::from_secs(2));
        let _ = release.send(());
        let _ = release.send(());
        assert!(
            answered.is_ok(),
            "a request or the screen waited for the OS read"
        );
        drop(first);
    }

    /// S1 review, R2: the OS changed but the file couldn't be written.
    #[tokio::test]
    async fn a_failed_store_is_reported_with_the_real_state() {
        let fake = FakeLaunchAtLogin::new();
        let sync = LoginSync::new(Arc::new(fake.clone()), |_: &Outcome| {
            Err("disk full".to_owned())
        });
        let outcome = sync.request(true).await.unwrap();
        assert!(outcome.enabled, "the OS result is kept");
        assert_eq!(fake.state(), Ok(LaunchAtLoginState::Enabled));
        let problem = outcome.problem.unwrap();
        let text = problem.to_string();
        assert!(
            text.contains("on now") && text.contains("disk full"),
            "{text}"
        );
        assert!(text.contains("try again"), "{text}");
        // And an OS refusal is not hidden by it.
        let refused = FakeLaunchAtLogin::new();
        refused.enabling_gives(LaunchAtLoginState::RequiresApproval);
        let sync = LoginSync::new(Arc::new(refused), |_: &Outcome| Err("disk full".to_owned()));
        assert_eq!(
            sync.request(true).await.unwrap().problem,
            Some(LaunchProblem::NeedsApproval)
        );
    }
}
