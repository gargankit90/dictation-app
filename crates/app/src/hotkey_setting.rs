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
//! [`startup_spec`] picks the spec to start with from the saved settings, and
//! [`apply`] is what saving Settings does with the hotkey field: switch to
//! it, or keep the old hotkey in both the listener and the settings to be
//! saved, so the file, the form and the keyboard always agree.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dictation_core::hotkey_spec::HotkeySpec;
use dictation_core::settings::Settings;
use dictation_ports::{HotkeyError, HotkeyEvent, HotkeySink, HotkeySource};

use crate::config::ConfigError;

/// Builds the real source for a spec; it listens only once started.
pub type HotkeyFactory<H> = Box<dyn Fn(HotkeySpec) -> H + Send + Sync>;

/// The pipeline's hotkey: a source for the current spec, replaceable through
/// its [`HotkeyControl`] while the pipeline runs.
///
/// A [`HotkeyEvent::Failed`] from the current source is passed on as usual;
/// the pipeline's sink is kept, so a later successful
/// [`switch`](HotkeyControl::switch) listens again.
pub struct ConfigurableHotkey<H> {
    shared: Arc<Shared<H>>,
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
    /// Nothing is listening (the pipeline hasn't started, or the hotkey
    /// couldn't start, for the reason given). The spec is kept: it is the one
    /// used the next time the hotkey starts.
    NotListening(Option<HotkeyError>),
}

struct Shared<H> {
    factory: HotkeyFactory<H>,
    state: Mutex<State<H>>,
    /// Where every source's events go. Never held while a source starts or
    /// stops, so a source's `stop` can wait for its own event in flight.
    delivery: Arc<Mutex<Delivery>>,
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
}

impl Delivery {
    fn send(&mut self, event: HotkeyEvent) {
        if let Some(sink) = self.sink.as_mut() {
            sink(event);
        }
    }

    /// An event from the source of `generation`.
    fn deliver(&mut self, generation: u64, event: HotkeyEvent) {
        if generation == self.active {
            match event {
                HotkeyEvent::Pressed => self.held = true,
                HotkeyEvent::Released | HotkeyEvent::Interrupted | HotkeyEvent::Failed => {
                    self.held = false;
                }
                // Esc says nothing about the hotkey.
                HotkeyEvent::CancelPressed => {}
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
                // Esc before the switch is the old hotkey's business.
                HotkeyEvent::CancelPressed => {}
            }
        }
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
    /// be dropped); a hold of the new key that began while it was pending
    /// starts, so its release isn't the first the pipeline hears of it.
    fn hand_over(&mut self) {
        self.active = self.pending;
        let pressed = self.pending_held;
        let failed = self.pending_failed;
        self.abandon();
        if std::mem::take(&mut self.held) {
            self.send(HotkeyEvent::Interrupted);
        }
        if failed {
            self.send(HotkeyEvent::Failed);
        } else if pressed {
            self.held = true;
            self.send(HotkeyEvent::Pressed);
        }
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
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl<H: HotkeySource + 'static> ConfigurableHotkey<H> {
    /// A hotkey for `spec`, built with `factory`; returns it and its
    /// control. Nothing listens until the pipeline starts it.
    pub fn new(
        spec: HotkeySpec,
        factory: impl Fn(HotkeySpec) -> H + Send + Sync + 'static,
    ) -> (Self, HotkeyControl) {
        let shared = Arc::new(Shared {
            factory: Box::new(factory),
            state: Mutex::new(State {
                spec,
                source: None,
                started: false,
                last_error: None,
                generation: 0,
            }),
            delivery: Arc::new(Mutex::new(Delivery::default())),
        });
        let control = HotkeyControl {
            switch: Arc::clone(&shared) as Arc<dyn Switch>,
        };
        (Self { shared }, control)
    }
}

impl<H: HotkeySource> Shared<H> {
    /// A sink for the source of `generation`: passes its events on while it
    /// is the current one, and tracks whether its key is held.
    fn forwarder(&self, generation: u64) -> HotkeySink {
        let delivery = Arc::clone(&self.delivery);
        Box::new(move |event| lock(&delivery).deliver(generation, event))
    }

    /// Builds and starts a source for `spec` that sends as `generation`.
    fn start_source(&self, spec: HotkeySpec, generation: u64) -> Result<H, HotkeyError> {
        let mut source = (self.factory)(spec);
        source.start(self.forwarder(generation))?;
        Ok(source)
    }
}

impl<H: HotkeySource + 'static> HotkeySource for ConfigurableHotkey<H> {
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
        match shared.start_source(state.spec, generation) {
            Ok(source) => {
                state.source = Some(source);
                state.started = true;
                state.last_error = None;
                Ok(())
            }
            Err(error) => {
                // As the trait says: on error the sink is dropped.
                lock(&shared.delivery).sink = None;
                state.last_error = Some(error.clone());
                Err(error)
            }
        }
    }

    fn stop(&mut self) {
        let shared = &self.shared;
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
    }
}

impl<H: HotkeySource + 'static> Switch for Shared<H> {
    fn spec(&self) -> HotkeySpec {
        lock(&self.state).spec
    }

    fn switch(&self, spec: HotkeySpec) -> Result<Switched, HotkeyError> {
        let mut state = lock(&self.state);
        if state.started && state.source.is_some() && state.spec == spec {
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
            Err(error) => {
                lock(&self.delivery).abandon();
                return Err(error);
            }
        };
        // The handoff, atomic with delivery: the new source's events reach
        // the pipeline from here on, the old one's never again.
        lock(&self.delivery).hand_over();
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
    /// # Errors
    ///
    /// The new source's error if it can't start; the old hotkey keeps
    /// listening and stays current.
    pub fn switch(&self, spec: HotkeySpec) -> Result<Switched, HotkeyError> {
        self.switch.switch(spec)
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
            Self::NotListening { wanted, reason } => Some(match reason {
                Some(HotkeyError::PermissionDenied) => format!(
                    "The hotkey {wanted} is saved, but dictation-app can't listen to the keyboard: allow it in System Settings, Privacy & Security, Accessibility, then restart dictation-app."
                ),
                Some(error) => format!(
                    "The hotkey {wanted} is saved, but no hotkey is listening right now ({error}). Restart dictation-app to use it."
                ),
                None => format!(
                    "The hotkey {wanted} is saved, but no hotkey is listening right now. Restart dictation-app to use it."
                ),
            }),
            Self::Kept {
                kept,
                wanted,
                error,
            } => Some(format!(
                "Couldn't switch the hotkey to {wanted} ({error}). {kept} still works, and your other settings were saved."
            )),
        }
    }

    /// Puts the previous hotkey back after a save that switched it failed,
    /// so the listener matches the file again. Best effort: a failure is
    /// logged.
    pub fn undo(&self, control: &HotkeyControl) {
        if let Self::Switched { previous } = self {
            match control.switch(*previous) {
                Ok(_) => tracing::info!(hotkey = %previous, "hotkey restored after a failed save"),
                Err(error) => tracing::warn!(
                    %error,
                    hotkey = %previous,
                    "could not restore the hotkey after a failed save"
                ),
            }
        }
    }
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
