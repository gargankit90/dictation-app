use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_ports::Clock;

use super::lock;

/// A [`Clock`] that moves only when the test calls [`advance`](Self::advance).
#[derive(Debug, Clone, Default)]
pub struct FakeClock {
    now: Arc<Mutex<Duration>>,
}

impl FakeClock {
    /// A clock at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A clock at `now`.
    #[must_use]
    pub fn at(now: Duration) -> Self {
        Self {
            now: Arc::new(Mutex::new(now)),
        }
    }

    /// Moves the clock forward by `by` (it never goes back).
    pub fn advance(&self, by: Duration) {
        let mut now = lock(&self.now);
        *now = now.saturating_add(by);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        *lock(&self.now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_zero_and_moves_only_when_advanced() {
        let clock = FakeClock::new();
        assert_eq!(clock.now(), Duration::ZERO);
        assert_eq!(clock.now(), Duration::ZERO);
        clock.advance(Duration::from_millis(250));
        assert_eq!(clock.now(), Duration::from_millis(250));
    }

    #[test]
    fn clones_share_time() {
        let clock = FakeClock::at(Duration::from_secs(10));
        let handle = clock.clone();
        handle.advance(Duration::from_secs(1));
        assert_eq!(clock.now(), Duration::from_secs(11));
    }

    #[test]
    fn never_overflows() {
        let clock = FakeClock::at(Duration::MAX);
        clock.advance(Duration::from_secs(1));
        assert_eq!(clock.now(), Duration::MAX);
    }
}
