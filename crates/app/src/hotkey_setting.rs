//! The hotkey setting (T2, P1-03): the saved hotkey is the one that listens,
//! at startup and as soon as Settings is saved, without a restart.
//!
//! [`ConfigurableHotkey`] is the pipeline's [`HotkeySource`]. It builds the
//! real source for a [`HotkeySpec`] with a factory (the shell's
//! `SystemHotkey::new`, a fake in tests) and keeps the pipeline's sink, so
//! [`HotkeyControl::switch`] can put a source for another spec in its place
//! while the pipeline runs: the new source starts first, and only once it
//! listens is the old one stopped. If the new one can't start, the old one
//! keeps listening and the error says why.
//!
//! The handoff is one step under the delivery lock, which every event of
//! every source passes through: before it only the old source's events reach
//! the pipeline, after it only the new one's. There is no moment when
//! neither does (a press of the new key while the old listener is still
//! being stopped and joined arrives) or both do (an old event that was
//! already on its way is dropped, not delivered after the handoff).
//!
//! The hotkey comes back by itself (#88). When the current source dies (a
//! [`HotkeyEvent::Failed`]: Accessibility was turned off, the event tap was
//! removed) or can't start at all, a supervisor thread builds and starts a
//! new one for the current spec, waiting longer after each failure
//! ([`RESTART_POLICY`]), until one listens; then it sends
//! [`HotkeyEvent::Restored`]. It sleeps on a condition variable in between,
//! so it costs nothing while the hotkey works or while it waits, and it ends
//! with [`stop`](HotkeySource::stop) (Quit). There is one supervisor per
//! start, and it replaces the dead source, so there is never a second
//! listener. A [`HotkeyControl::switch`] while the hotkey is down records
//! the new spec, which is the one that comes back.
//!
//! [`startup_spec`] picks the spec to start with from the saved settings, and
//! [`apply`] is what saving Settings does with the hotkey field: switch to
//! it, or keep the old hotkey in both the listener and the settings to be
//! saved, so the file, the form and the keyboard always agree.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use dictation_core::error::DictationError;
use dictation_core::hotkey_spec::HotkeySpec;
use dictation_core::retry::RetryPolicy;
use dictation_core::settings::Settings;
use dictation_ports::{HotkeyError, HotkeyEvent, HotkeySink, HotkeySource, Permission};

use crate::config::ConfigError;
use crate::tray::keycap_label;

/// Builds the real source for a spec; it listens only once started.
pub type HotkeyFactory<H> = Box<dyn Fn(HotkeySpec) -> H + Send + Sync>;

/// How the supervisor waits between tries to bring a dead hotkey back: half
/// a second, then twice as long each time, at most 2 s, and it never gives
/// up. A grant of Accessibility is picked up within about 2 s; a few tries a
/// second at most, each a cheap permission check, don't load the CPU.
pub const RESTART_POLICY: RetryPolicy = RetryPolicy {
    max_attempts: u32::MAX,
    base_delay: Duration::from_millis(500),
    multiplier: 2,
    max_delay: Duration::from_secs(2),
    jitter: 0.0,
};

/// The pipeline's hotkey: a source for the current spec, replaceable through
/// its [`HotkeyControl`] while the pipeline runs.
///
/// A [`HotkeyEvent::Failed`] from the current source is passed on as usual;
/// the pipeline's sink is kept, and the supervisor (see the module docs)
/// restarts the hotkey and sends [`HotkeyEvent::Restored`]. If the first
/// source can't start, [`start`](HotkeySource::start) still succeeds: it
/// sends [`HotkeyEvent::Failed`] and the supervisor keeps trying.
pub struct ConfigurableHotkey<H> {
    shared: Arc<Shared<H>>,
    /// The supervisor thread while started.
    supervisor: Option<JoinHandle<()>>,
}

/// Changes the hotkey of a running [`ConfigurableHotkey`]. Cheap to clone;
/// every clone controls the same hotkey.
#[derive(Clone)]
pub struct HotkeyControl {
    switch: Arc<dyn Switch>,
}

/// What [`HotkeyControl::switch`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Switched {
    /// The new hotkey listens now; the old one doesn't.
    Now,
    /// It was already the hotkey; nothing was re-registered.
    Unchanged,
    /// Nothing is listening (the pipeline hasn't started, or the hotkey is
    /// down and couldn't start, for the reason given). The spec is kept: it
    /// is the one used the next time the hotkey starts, or comes back.
    NotListening(Option<HotkeyError>),
}

struct Shared<H> {
    factory: HotkeyFactory<H>,
    /// How long the supervisor waits between tries.
    restart: RetryPolicy,
    state: Mutex<State<H>>,
    /// Where every source's events go. Never held while a source starts or
    /// stops, so a source's `stop` can wait for its own event in flight.
    delivery: Arc<Mutex<Delivery>>,
    /// Wakes the supervisor (with `delivery`): the hotkey died, or stopped.
    wake: Arc<Condvar>,
}

/// Which source's events reach the pipeline. Each event is checked and
/// passed on under this lock, so the handoff is atomic with delivery.
#[derive(Default)]
struct Delivery {
    /// The pipeline's sink while started. Every source this hotkey starts
    /// sends through it.
    sink: Option<HotkeySink>,
    /// The generation whose events reach the sink; a source's events are
    /// dropped unless it is the current one.
    active: u64,
    /// Whether the current source's last event was a press, so a switch in
    /// the middle of a hold can end it (the release would come from the old
    /// source, which no longer reaches the pipeline).
    held: bool,
    /// The generation being started by a switch, if any (0: none). Its
    /// events are not passed on yet, only remembered for the handoff.
    pending: u64,
    /// Whether the pending source's key is held now.
    pending_held: bool,
    /// Whether the pending source has failed since it started.
    pending_failed: bool,
    /// The hotkey is down: the current source failed, or none could start.
    /// The supervisor brings it back.
    dead: bool,
    /// The hotkey was stopped; the supervisor ends.
    closed: bool,
    /// While set and in the future, presses, releases, Esc and shortcuts
    /// don't reach the pipeline: the user is choosing a new key (K1). It
    /// lapses by itself, so a window that never says it is done (closed,
    /// crashed) can't leave the hotkey off.
    paused_until: Option<Instant>,
    /// A press was dropped (or the hold interrupted) while paused, so its
    /// release, which may come after the pause ends, must not reach the
    /// pipeline alone.
    swallow_release: bool,
}

impl Delivery {
    fn send(&mut self, event: HotkeyEvent) {
        if let Some(sink) = self.sink.as_mut() {
            sink(event);
        }
    }

    /// Whether delivery is paused now; clears a lapsed pause.
    fn paused(&mut self) -> bool {
        match self.paused_until {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                self.paused_until = None;
                false
            }
            None => false,
        }
    }

    /// Pauses delivery for `lease` (renewing a pause), or ends the pause
    /// (`None`). A hold in progress ends with [`HotkeyEvent::Interrupted`];
    /// its release is dropped whenever it comes.
    fn pause(&mut self, lease: Option<Duration>) {
        match lease {
            Some(lease) => {
                self.paused_until = Some(Instant::now() + lease);
                if std::mem::take(&mut self.held) {
                    self.swallow_release = true;
                    self.send(HotkeyEvent::Interrupted);
                }
            }
            None => self.paused_until = None,
        }
    }

    /// Whether a pause (or a hold it cut short) drops `event`.
    fn dropped(&mut self, event: HotkeyEvent) -> bool {
        let paused = self.paused();
        match event {
            HotkeyEvent::Pressed if paused => {
                self.swallow_release = true;
                true
            }
            HotkeyEvent::Pressed => {
                self.swallow_release = false;
                false
            }
            HotkeyEvent::Released | HotkeyEvent::Interrupted if paused || self.swallow_release => {
                self.swallow_release = false;
                true
            }
            HotkeyEvent::CancelPressed | HotkeyEvent::ShortcutPressed => paused,
            _ => false,
        }
    }

    /// An event from the source of `generation`. Returns whether it made
    /// the hotkey [`dead`](Self::dead), so the supervisor must be woken.
    fn deliver(&mut self, generation: u64, event: HotkeyEvent) -> bool {
        let mut died = false;
        if generation == self.active {
            if self.dropped(event) {
                return false;
            }
            match event {
                HotkeyEvent::Pressed => self.held = true,
                HotkeyEvent::Released | HotkeyEvent::Interrupted => self.held = false,
                HotkeyEvent::Failed => {
                    self.held = false;
                    died = !self.dead;
                    self.dead = true;
                }
                // Esc and a shortcut say nothing about whether the hotkey
                // is down (its release still comes), and a source doesn't
                // restore itself.
                HotkeyEvent::CancelPressed
                | HotkeyEvent::ShortcutPressed
                | HotkeyEvent::Restored => {}
            }
            self.send(event);
        } else if generation == self.pending {
            match event {
                HotkeyEvent::Pressed => self.pending_held = true,
                HotkeyEvent::Released | HotkeyEvent::Interrupted => self.pending_held = false,
                HotkeyEvent::Failed => {
                    self.pending_held = false;
                    self.pending_failed = true;
                }
                // Esc or a shortcut before the switch is the old hotkey's
                // business.
                HotkeyEvent::CancelPressed
                | HotkeyEvent::ShortcutPressed
                | HotkeyEvent::Restored => {}
            }
        }
        died
    }

    /// A switch is starting the source of `generation`.
    fn begin(&mut self, generation: u64) {
        self.pending = generation;
        self.pending_held = false;
        self.pending_failed = false;
    }

    /// The pending source couldn't start.
    fn abandon(&mut self) {
        self.begin(0);
    }

    /// Makes the pending source the current one: from here on only its
    /// events reach the sink. A hold of the old key ends (its release would
    /// be dropped); if the hotkey was down, [`HotkeyEvent::Restored`] says
    /// it is back; a hold of the new key that began while it was pending
    /// starts, so its release isn't the first the pipeline hears of it.
    ///
    /// Returns whether the new source has already failed, which leaves the
    /// hotkey dead (and the supervisor must be woken).
    fn hand_over(&mut self) -> bool {
        self.active = self.pending;
        let pressed = self.pending_held;
        let failed = self.pending_failed;
        self.abandon();
        if std::mem::take(&mut self.held) {
            self.send(HotkeyEvent::Interrupted);
        }
        if failed {
            self.dead = true;
            self.send(HotkeyEvent::Failed);
            return true;
        }
        if std::mem::take(&mut self.dead) {
            self.send(HotkeyEvent::Restored);
        }
        if pressed {
            if self.paused() {
                self.swallow_release = true;
            } else {
                self.held = true;
                self.send(HotkeyEvent::Pressed);
            }
        }
        false
    }
}

struct State<H> {
    spec: HotkeySpec,
    /// The listening source, if any.
    source: Option<H>,
    /// Whether the pipeline started this hotkey (and its sink is kept).
    started: bool,
    /// Why the hotkey couldn't start at the last try, if it couldn't.
    last_error: Option<HotkeyError>,
    /// The last generation handed out.
    generation: u64,
}

trait Switch: Send + Sync {
    fn spec(&self) -> HotkeySpec;
    fn switch(&self, spec: HotkeySpec) -> Result<Switched, HotkeyError>;
    fn pause(&self, lease: Option<Duration>);
    fn waiting_for(&self) -> Option<HotkeyError>;
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl<H: HotkeySource + 'static> ConfigurableHotkey<H> {
    /// A hotkey for `spec`, built with `factory`; returns it and its
    /// control. Nothing listens until the pipeline starts it. A dead hotkey
    /// is restarted under [`RESTART_POLICY`].
    pub fn new(
        spec: HotkeySpec,
        factory: impl Fn(HotkeySpec) -> H + Send + Sync + 'static,
    ) -> (Self, HotkeyControl) {
        Self::with_restart_policy(spec, factory, RESTART_POLICY)
    }

    /// Like [`new`](Self::new), restarting a dead hotkey under `restart`:
    /// the wait after the `n`th failed try is its
    /// [`backoff`](RetryPolicy::backoff)`(n)`; its attempt limit and jitter
    /// are not used (the supervisor never gives up).
    pub fn with_restart_policy(
        spec: HotkeySpec,
        factory: impl Fn(HotkeySpec) -> H + Send + Sync + 'static,
        restart: RetryPolicy,
    ) -> (Self, HotkeyControl) {
        let shared = Arc::new(Shared {
            factory: Box::new(factory),
            restart,
            state: Mutex::new(State {
                spec,
                source: None,
                started: false,
                last_error: None,
                generation: 0,
            }),
            delivery: Arc::new(Mutex::new(Delivery::default())),
            wake: Arc::new(Condvar::new()),
        });
        let control = HotkeyControl {
            switch: Arc::clone(&shared) as Arc<dyn Switch>,
        };
        (
            Self {
                shared,
                supervisor: None,
            },
            control,
        )
    }
}

/// Says, at warn, that the hotkey for `spec` can't listen and will come
/// back by itself. A missing Accessibility grant is logged as
/// `PermissionMissing`, which is what the e2e scenarios look for.
fn log_down(error: &HotkeyError, spec: HotkeySpec) {
    if *error == HotkeyError::PermissionDenied {
        tracing::warn!(
            error = %DictationError::PermissionMissing(Permission::Accessibility),
            hotkey = %spec,
            "PermissionMissing: the hotkey is off until Accessibility is granted; it starts by itself then"
        );
    } else {
        tracing::warn!(%error, hotkey = %spec, "the hotkey can't listen yet; it starts as soon as it can");
    }
}

/// What one try of the supervisor's came to.
enum Restart {
    /// The hotkey listens (again, or a switch had already brought it back).
    Listening,
    /// It is still down.
    Failed,
    /// The hotkey was stopped meanwhile.
    Stopped,
}

/// The supervisor thread: waits until the hotkey is dead, then tries to
/// bring it back after each backoff until it listens, and again whenever it
/// dies; ends once the hotkey is stopped.
fn supervise<H: HotkeySource>(shared: &Shared<H>) {
    let mut failures = 0_u32;
    loop {
        if !shared.wait_until_dead() {
            return;
        }
        failures = failures.saturating_add(1);
        if !shared.sleep(shared.restart.backoff(failures)) {
            return;
        }
        match shared.restart(failures) {
            Restart::Listening => failures = 0,
            Restart::Failed => {}
            Restart::Stopped => return,
        }
    }
}

impl<H: HotkeySource> Shared<H> {
    /// A sink for the source of `generation`: passes its events on while it
    /// is the current one, tracks whether its key is held, and wakes the
    /// supervisor when it dies.
    fn forwarder(&self, generation: u64) -> HotkeySink {
        let delivery = Arc::clone(&self.delivery);
        let wake = Arc::clone(&self.wake);
        Box::new(move |event| {
            if lock(&delivery).deliver(generation, event) {
                wake.notify_all();
            }
        })
    }

    /// [`Delivery::hand_over`], waking the supervisor if the new source has
    /// already died.
    fn hand_over(&self) -> bool {
        let died = lock(&self.delivery).hand_over();
        if died {
            self.wake.notify_all();
        }
        died
    }

    /// Blocks until the hotkey is dead (`true`) or stopped (`false`).
    fn wait_until_dead(&self) -> bool {
        let mut delivery = lock(&self.delivery);
        loop {
            if delivery.closed {
                return false;
            }
            if delivery.dead {
                return true;
            }
            delivery = self
                .wake
                .wait(delivery)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Waits `delay`; `false` if the hotkey is stopped first, which ends
    /// the wait at once.
    fn sleep(&self, delay: Duration) -> bool {
        let deadline = Instant::now() + delay;
        let mut delivery = lock(&self.delivery);
        loop {
            if delivery.closed {
                return false;
            }
            let now = Instant::now();
            if now >= deadline {
                return true;
            }
            delivery = self
                .wake
                .wait_timeout(delivery, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// One try to bring the dead hotkey back, the `failures`th in a row:
    /// stops what is left of the dead source, then starts a new one for the
    /// current spec, which takes over if it listens.
    fn restart(&self, failures: u32) -> Restart {
        let mut state = lock(&self.state);
        if !state.started {
            return Restart::Stopped;
        }
        if !lock(&self.delivery).dead {
            // A switch brought it back meanwhile.
            return Restart::Listening;
        }
        if let Some(mut dead) = state.source.take() {
            // Its events no longer count; its thread (if any) ends.
            dead.stop();
        }
        state.generation += 1;
        let generation = state.generation;
        lock(&self.delivery).begin(generation);
        match self.start_source(state.spec, generation) {
            Ok(source) => {
                state.source = Some(source);
                if self.hand_over() {
                    return Restart::Failed;
                }
                state.last_error = None;
                tracing::info!(hotkey = %state.spec, failures, "the hotkey listens again");
                Restart::Listening
            }
            Err(error) => {
                lock(&self.delivery).abandon();
                if failures == 1 {
                    log_down(&error, state.spec);
                } else {
                    tracing::debug!(%error, failures, "the hotkey still can't listen");
                }
                state.last_error = Some(error);
                Restart::Failed
            }
        }
    }

    /// Builds and starts a source for `spec` that sends as `generation`.
    fn start_source(&self, spec: HotkeySpec, generation: u64) -> Result<H, HotkeyError> {
        let mut source = (self.factory)(spec);
        source.start(self.forwarder(generation))?;
        Ok(source)
    }
}

impl<H: HotkeySource + 'static> HotkeySource for ConfigurableHotkey<H> {
    /// Starts the source for the current spec and the supervisor. If the
    /// source can't start, sends [`HotkeyEvent::Failed`] and still returns
    /// `Ok`: the supervisor brings the hotkey up as soon as it can.
    ///
    /// # Errors
    ///
    /// [`HotkeyError::AlreadyStarted`], or [`HotkeyError::Unavailable`] if
    /// the supervisor thread can't be created (then nothing listens).
    fn start(&mut self, sink: HotkeySink) -> Result<(), HotkeyError> {
        let shared = &self.shared;
        let mut state = lock(&shared.state);
        if state.started {
            return Err(HotkeyError::AlreadyStarted);
        }
        state.generation += 1;
        let generation = state.generation;
        *lock(&shared.delivery) = Delivery {
            sink: Some(sink),
            active: generation,
            ..Delivery::default()
        };
        // First, so a supervisor that can't run means nothing started. It
        // waits for the state lock or for the hotkey to die.
        let supervised = Arc::clone(shared);
        let supervisor = thread::Builder::new()
            .name("hotkey-supervisor".into())
            .spawn(move || supervise(&supervised));
        let supervisor = match supervisor {
            Ok(handle) => handle,
            Err(error) => {
                // As the trait says: on error the sink is dropped.
                lock(&shared.delivery).sink = None;
                return Err(HotkeyError::Unavailable(error.to_string()));
            }
        };
        self.supervisor = Some(supervisor);
        state.started = true;
        match shared.start_source(state.spec, generation) {
            Ok(source) => {
                state.source = Some(source);
                state.last_error = None;
            }
            Err(error) => {
                log_down(&error, state.spec);
                state.last_error = Some(error);
                let mut delivery = lock(&shared.delivery);
                delivery.dead = true;
                delivery.send(HotkeyEvent::Failed);
                shared.wake.notify_all();
            }
        }
        Ok(())
    }

    fn stop(&mut self) {
        let shared = &self.shared;
        {
            let mut state = lock(&shared.state);
            state.started = false;
            if let Some(mut source) = state.source.take() {
                // Its stop may wait for a sink call, which takes only the
                // delivery lock, never the state's.
                source.stop();
            }
            let mut delivery = lock(&shared.delivery);
            delivery.sink = None;
            delivery.held = false;
            delivery.dead = false;
            delivery.closed = true;
            shared.wake.notify_all();
        }
        // Without the locks, which it may be waiting for.
        if let Some(supervisor) = self.supervisor.take() {
            // A panic in it already ended it; nothing to undo.
            let _ = supervisor.join();
        }
    }
}

impl<H> Drop for ConfigurableHotkey<H> {
    fn drop(&mut self) {
        if let Some(supervisor) = self.supervisor.take() {
            {
                let mut delivery = lock(&self.shared.delivery);
                delivery.closed = true;
                self.shared.wake.notify_all();
            }
            let _ = supervisor.join();
        }
    }
}

impl<H: HotkeySource + 'static> Switch for Shared<H> {
    fn spec(&self) -> HotkeySpec {
        lock(&self.state).spec
    }

    fn pause(&self, lease: Option<Duration>) {
        lock(&self.delivery).pause(lease);
    }

    fn waiting_for(&self) -> Option<HotkeyError> {
        let state = lock(&self.state);
        let dead = lock(&self.delivery).dead;
        // Before the first failed try there's nothing to say yet: the
        // supervisor tries within half a second.
        (state.started && dead)
            .then(|| state.last_error.clone())
            .flatten()
    }

    fn switch(&self, spec: HotkeySpec) -> Result<Switched, HotkeyError> {
        let mut state = lock(&self.state);
        let dead = lock(&self.delivery).dead;
        if state.started && !dead && state.source.is_some() && state.spec == spec {
            return Ok(Switched::Unchanged);
        }
        if !state.started {
            state.spec = spec;
            return Ok(Switched::NotListening(state.last_error.clone()));
        }
        state.generation += 1;
        let generation = state.generation;
        // The new source first: if it can't start, the old one keeps
        // listening as if nothing happened.
        lock(&self.delivery).begin(generation);
        let source = match self.start_source(spec, generation) {
            Ok(source) => source,
            Err(error) if dead => {
                // Nothing listens anyway: `spec` is the hotkey the
                // supervisor brings back.
                lock(&self.delivery).abandon();
                state.spec = spec;
                state.last_error = Some(error.clone());
                return Ok(Switched::NotListening(Some(error)));
            }
            Err(error) => {
                lock(&self.delivery).abandon();
                return Err(error);
            }
        };
        // The handoff, atomic with delivery: the new source's events reach
        // the pipeline from here on, the old one's never again.
        self.hand_over();
        if let Some(mut old) = state.source.replace(source) {
            // Outside the delivery lock: the old source's stop may wait for
            // an event of its own in flight, which is then dropped. The new
            // key is heard meanwhile.
            old.stop();
        }
        state.spec = spec;
        state.last_error = None;
        Ok(Switched::Now)
    }
}

impl HotkeyControl {
    /// The current spec: the one listening, or the one the hotkey will
    /// start with.
    #[must_use]
    pub fn spec(&self) -> HotkeySpec {
        self.switch.spec()
    }

    /// Makes `spec` the hotkey. While the pipeline runs, a source for `spec`
    /// is started and, once it listens, takes over in one step; then the old
    /// one is stopped. A hold of the old key ends with
    /// [`HotkeyEvent::Interrupted`]. Blocks while the old source stops (it
    /// may wait for its thread); the new key is already heard meanwhile.
    ///
    /// While the hotkey is down, a source for `spec` is tried at once; if it
    /// can't start either, `spec` is kept as the one to bring back
    /// ([`Switched::NotListening`]).
    ///
    /// # Errors
    ///
    /// The new source's error if it can't start while the old one listens;
    /// the old hotkey keeps listening and stays current.
    pub fn switch(&self, spec: HotkeySpec) -> Result<Switched, HotkeyError> {
        self.switch.switch(spec)
    }

    /// Stops the hotkey from reaching the pipeline for `lease` (K1: while
    /// the user is choosing a new key in Settings, pressing the current one
    /// must not dictate). A hold in progress is interrupted. Call again to
    /// renew; it ends by itself after `lease`, so it can't stay off.
    pub fn pause(&self, lease: Duration) {
        self.switch.pause(Some(lease));
    }

    /// Ends [`pause`](Self::pause) now.
    pub fn resume(&self) {
        self.switch.pause(None);
    }

    /// What to tell the user while the started hotkey is down and waiting to
    /// come back, such as for Accessibility; `None` while it listens. Saving
    /// a setting doesn't fail for this, so Settings asks here (U9).
    #[must_use]
    pub fn notice(&self) -> Option<String> {
        let reason = self.switch.waiting_for()?;
        HotkeyChange::NotListening {
            wanted: self.spec(),
            reason: Some(reason),
        }
        .problem()
    }
}

/// The hotkey to start with: the saved one, or the default if the settings
/// can't be read (that is logged; the settings screen reports it too).
#[must_use]
pub fn startup_spec(saved: Result<Settings, ConfigError>) -> HotkeySpec {
    let parsed = saved
        .map_err(|error| error.to_string())
        .and_then(|settings| {
            settings
                .hotkey
                .parse::<HotkeySpec>()
                .map_err(|error| error.to_string())
        });
    parsed.unwrap_or_else(|error| {
        let spec = HotkeySpec::default();
        tracing::warn!(%error, hotkey = %spec, "can't use the saved hotkey; using the default");
        spec
    })
}

/// What saving Settings did to the hotkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyChange {
    /// The saved hotkey was already listening.
    Unchanged,
    /// The new hotkey listens; `previous` doesn't.
    Switched {
        /// The hotkey before the save, for [`undo`](HotkeyChange::undo).
        previous: HotkeySpec,
    },
    /// The new hotkey is saved but nothing listens now; it is used the next
    /// time the hotkey starts.
    NotListening {
        /// The saved hotkey.
        wanted: HotkeySpec,
        /// Why nothing listens, if known.
        reason: Option<HotkeyError>,
    },
    /// The new hotkey couldn't start: `kept` still listens, and the settings
    /// to be saved hold `kept` again.
    Kept {
        /// The hotkey still listening.
        kept: HotkeySpec,
        /// The hotkey the user asked for.
        wanted: HotkeySpec,
        /// Why it couldn't start.
        error: HotkeyError,
    },
    /// The hotkey field doesn't parse; saving reports that, so nothing was
    /// touched.
    Invalid,
}

impl HotkeyChange {
    /// What to tell the user, if the save didn't simply take effect.
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::Unchanged | Self::Switched { .. } | Self::Invalid => None,
            Self::NotListening { wanted, reason } => {
                let key = shown(wanted);
                Some(match reason {
                    Some(HotkeyError::PermissionDenied) => format!(
                        "The dictation key {key} is saved, but Vocet can't listen to the keyboard yet: allow it in System Settings → Privacy & Security → Accessibility, and {key} starts working by itself."
                    ),
                    Some(error) => format!(
                        "The dictation key {key} is saved, but no dictation key is listening right now ({error}). {key} starts working as soon as it can."
                    ),
                    None => format!(
                        "The dictation key {key} is saved, but no dictation key is listening right now. Restart Vocet to use it."
                    ),
                })
            }
            Self::Kept {
                kept,
                wanted,
                error,
            } => Some(format!(
                "Couldn't switch the dictation key to {} ({error}). {} still works, and your other settings were saved.",
                shown(wanted),
                shown(kept),
            )),
        }
    }

    /// Puts the previous hotkey back after a save that switched it failed,
    /// so the listener matches the file again.
    ///
    /// # Errors
    ///
    /// [`UndoFailed`] if the previous hotkey can't start again: the new one
    /// then still listens, and the error names both, so the caller can tell
    /// the user which hotkey works (T3 round 2).
    pub fn undo(&self, control: &HotkeyControl) -> Result<(), UndoFailed> {
        let Self::Switched { previous } = self else {
            return Ok(());
        };
        match control.switch(*previous) {
            Ok(_) => {
                tracing::info!(hotkey = %previous, "hotkey restored after a failed save");
                Ok(())
            }
            Err(error) => {
                let active = control.spec();
                tracing::warn!(
                    %error,
                    hotkey = %previous,
                    active = %active,
                    "could not restore the hotkey after a failed save"
                );
                Err(UndoFailed {
                    previous: *previous,
                    active,
                    error,
                })
            }
        }
    }
}

/// A hotkey as the user sees it: its keycap label, never its stored name.
fn shown(spec: &HotkeySpec) -> String {
    keycap_label(&spec.to_string())
}

/// Why [`HotkeyChange::undo`] couldn't put the previous hotkey back. The
/// text is for the user and says which hotkey works now.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "Switching the dictation key back to {} failed ({error}), so {} is the dictation key that works now; Settings shows it, and saving again keeps it.",
    shown(.previous),
    shown(.active)
)]
pub struct UndoFailed {
    /// The hotkey from before the save, which couldn't start again.
    pub previous: HotkeySpec,
    /// The hotkey still listening (or to start with).
    pub active: HotkeySpec,
    /// Why `previous` couldn't start.
    pub error: HotkeyError,
}

/// `saved` with its hotkey set to `active`, the one that listens now (or
/// starts with the pipeline), if they differ: the file can hold another
/// hotkey after a save whose write and undo both failed ([`UndoFailed`]), or
/// one that doesn't parse, for which the default listens. Settings then
/// shows the hotkey that really works, as it shows the OS's login item.
#[must_use]
pub fn with_active(active: HotkeySpec, mut saved: Settings) -> Settings {
    if saved.hotkey.parse::<HotkeySpec>().ok() != Some(active) {
        tracing::info!(
            saved = %saved.hotkey,
            active = %active,
            "the hotkey listening isn't the saved one; showing the one listening"
        );
        saved.hotkey = active.to_string();
    }
    saved
}

/// What saving Settings does with its hotkey field: write it in canonical
/// form and switch the listener to it, or, if that fails, put the hotkey that
/// still listens back into `settings`, so the file never claims a hotkey that
/// doesn't work. Call before writing the file; if writing fails,
/// [`HotkeyChange::undo`].
pub fn apply(control: &HotkeyControl, settings: &mut Settings) -> HotkeyChange {
    let Ok(wanted) = settings.hotkey.parse::<HotkeySpec>() else {
        return HotkeyChange::Invalid;
    };
    // Saved the way the app writes it (`leftalt` becomes `LeftAlt`), so the
    // file and the form match what the log and errors say.
    settings.hotkey = wanted.to_string();
    let previous = control.spec();
    match control.switch(wanted) {
        Ok(Switched::Unchanged) => HotkeyChange::Unchanged,
        Ok(Switched::Now) => {
            tracing::info!(hotkey = %wanted, previous = %previous, "hotkey applied");
            HotkeyChange::Switched { previous }
        }
        Ok(Switched::NotListening(reason)) => {
            tracing::info!(hotkey = %wanted, "hotkey saved; it is used when the hotkey starts");
            HotkeyChange::NotListening { wanted, reason }
        }
        Err(error) => {
            tracing::warn!(
                %error,
                hotkey = %wanted,
                kept = %previous,
                "could not switch the hotkey; keeping the old one"
            );
            settings.hotkey = previous.to_string();
            HotkeyChange::Kept {
                kept: previous,
                wanted,
                error,
            }
        }
    }
}
