//! Time, injected so tests never sleep.

use std::time::Duration;

/// A monotonic clock. The coordinator never reads time itself; the pipeline
/// reads it here and puts it into events.
///
/// Threading: callable from any thread; never blocks.
pub trait Clock: Send + Sync {
    /// Time since this clock's origin; never goes backwards.
    ///
    /// Errors: none. Threading: any thread; never blocks.
    fn now(&self) -> Duration;
}
