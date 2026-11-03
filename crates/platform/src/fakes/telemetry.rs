//! A [`Telemetry`] that keeps what it was given.

use std::sync::{Arc, Mutex};

use dictation_ports::{Event, Telemetry};

use super::lock;

/// Records events in memory. Clones share the list.
#[derive(Debug, Clone, Default)]
pub struct FakeTelemetry {
    events: Arc<Mutex<Vec<Event>>>,
}

impl FakeTelemetry {
    /// An empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything recorded so far, in order.
    #[must_use]
    pub fn events(&self) -> Vec<Event> {
        lock(&self.events).clone()
    }
}

impl Telemetry for FakeTelemetry {
    fn record(&self, event: Event) {
        lock(&self.events).push(event);
    }
}
