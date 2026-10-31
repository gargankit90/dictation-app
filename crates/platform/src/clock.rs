//! [`SystemClock`]: the real [`Clock`].

use std::time::{Duration, Instant};

use dictation_ports::Clock;

/// Monotonic time since the clock was created.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// A clock whose origin is now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_near_zero_and_never_goes_back() {
        let clock = SystemClock::new();
        let first = clock.now();
        assert!(first < Duration::from_secs(5));
        assert!(clock.now() >= first);
    }
}
