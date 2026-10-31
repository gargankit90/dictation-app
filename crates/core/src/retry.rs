//! Retry rules: exponential backoff with jitter and a maximum number of
//! attempts.
//!
//! Pure: no clock and no sleeping. [`Retry::failed`] takes the current time
//! (from the caller's `Clock`) and says when to try again, and jitter comes
//! from an injected [`RandomSource`], so tests use plain durations and a
//! seeded [`SplitMix64`].

use std::time::Duration;

/// A source of uniform random numbers in `0.0..1.0`.
pub trait RandomSource {
    /// The next number, `0.0 <= x < 1.0`.
    fn next_unit(&mut self) -> f64;
}

/// A small, fast, seedable generator (`SplitMix64`). Good enough for jitter;
/// not for anything secret.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A generator that always produces the same sequence for `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl RandomSource for SplitMix64 {
    fn next_unit(&mut self) -> f64 {
        // The top 53 bits fill an f64 mantissa exactly: 0 <= x < 1.
        #[allow(clippy::cast_precision_loss, reason = "53 bits fit an f64 exactly")]
        let x = (self.next_u64() >> 11) as f64;
        x / 9_007_199_254_740_992.0 // 2^53
    }
}

/// How to retry a failing operation.
///
/// The retry after the `n`th failure (`n` from 1) waits
/// `min(base_delay * multiplier^(n-1), max_delay)`, shortened by up to
/// `jitter` of itself at random, so many clients don't retry in step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryPolicy {
    /// Attempts in all, the first included. At least 1 (0 counts as 1).
    pub max_attempts: u32,
    /// The wait before the first retry, before jitter.
    pub base_delay: Duration,
    /// How much each wait grows over the last. At least 1.
    pub multiplier: u32,
    /// The longest wait, before jitter.
    pub max_delay: Duration,
    /// The fraction of a wait that may be taken off at random, `0.0..=1.0`
    /// (clamped). 0 means no jitter.
    pub jitter: f64,
}

impl Default for RetryPolicy {
    /// Three attempts, waiting about 0.5 s then 1 s, each up to 20% shorter.
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            multiplier: 2,
            max_delay: Duration::from_secs(8),
            jitter: 0.2,
        }
    }
}

impl RetryPolicy {
    /// The wait before the retry that follows failure number `failure`
    /// (from 1), without jitter. Saturates at [`max_delay`](Self::max_delay).
    #[must_use]
    pub fn backoff(&self, failure: u32) -> Duration {
        let exponent = failure.saturating_sub(1);
        let growth = self.multiplier.max(1).checked_pow(exponent);
        growth
            .and_then(|g| self.base_delay.checked_mul(g))
            .map_or(self.max_delay, |d| d.min(self.max_delay))
    }

    /// The wait after failure number `failure` with jitter applied, or
    /// `None` if that failure used up the last attempt.
    pub fn delay_after(&self, failure: u32, rng: &mut impl RandomSource) -> Option<Duration> {
        if failure >= self.max_attempts.max(1) {
            return None;
        }
        let backoff = self.backoff(failure);
        let cut = self.jitter.clamp(0.0, 1.0) * rng.next_unit().clamp(0.0, 1.0);
        Some(backoff.mul_f64(1.0 - cut))
    }
}

/// What to do after a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Try again at this time (the caller's clock).
    RetryAt(Duration),
    /// Out of attempts; report the last error.
    GiveUp,
}

/// Tracks the attempts of one operation under a [`RetryPolicy`].
#[derive(Debug, Clone)]
pub struct Retry {
    policy: RetryPolicy,
    failures: u32,
}

impl Retry {
    /// A fresh operation: nothing has failed yet.
    #[must_use]
    pub fn new(policy: RetryPolicy) -> Self {
        Self {
            policy,
            failures: 0,
        }
    }

    /// Failures so far.
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Records a failure at `now` and says whether and when to try again.
    pub fn failed(&mut self, now: Duration, rng: &mut impl RandomSource) -> RetryDecision {
        self.failures = self.failures.saturating_add(1);
        match self.policy.delay_after(self.failures, rng) {
            Some(delay) => RetryDecision::RetryAt(now.saturating_add(delay)),
            None => RetryDecision::GiveUp,
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// Always returns the same number.
    struct Fixed(f64);

    impl RandomSource for Fixed {
        fn next_unit(&mut self) -> f64 {
            self.0
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn policy() -> RetryPolicy {
        RetryPolicy {
            max_attempts: 6,
            base_delay: ms(100),
            multiplier: 2,
            max_delay: ms(1_000),
            jitter: 0.5,
        }
    }

    #[test]
    fn backoff_doubles_up_to_the_cap() {
        let p = policy();
        let waits: Vec<_> = (1..=6).map(|n| p.backoff(n)).collect();
        assert_eq!(
            waits,
            [ms(100), ms(200), ms(400), ms(800), ms(1_000), ms(1_000)]
        );
        // Failure 0 is treated like the first.
        assert_eq!(p.backoff(0), ms(100));
    }

    #[test]
    fn backoff_saturates_instead_of_overflowing() {
        let p = RetryPolicy {
            multiplier: u32::MAX,
            max_delay: Duration::MAX,
            ..policy()
        };
        assert_eq!(p.backoff(40), Duration::MAX);
        let p = RetryPolicy {
            base_delay: Duration::MAX,
            ..policy()
        };
        assert_eq!(p.backoff(3), ms(1_000));
    }

    #[test]
    fn a_multiplier_of_zero_counts_as_one() {
        let p = RetryPolicy {
            multiplier: 0,
            ..policy()
        };
        assert_eq!(p.backoff(5), ms(100));
    }

    #[test]
    fn jitter_takes_off_up_to_its_fraction() {
        let p = policy();
        assert_eq!(p.delay_after(2, &mut Fixed(0.0)), Some(ms(200)));
        assert_eq!(p.delay_after(2, &mut Fixed(0.5)), Some(ms(150)));
        // Just under 1.0 is the shortest wait the source can give.
        let shortest = p.delay_after(2, &mut Fixed(0.999_999)).unwrap();
        assert!(shortest > ms(100) && shortest < ms(101), "{shortest:?}");
        // Out-of-range sources and jitter are clamped.
        assert_eq!(p.delay_after(2, &mut Fixed(7.0)), Some(ms(100)));
        let none = RetryPolicy {
            jitter: -1.0,
            ..policy()
        };
        assert_eq!(none.delay_after(2, &mut Fixed(0.9)), Some(ms(200)));
        let all = RetryPolicy {
            jitter: 3.0,
            ..policy()
        };
        assert_eq!(all.delay_after(2, &mut Fixed(1.0)), Some(Duration::ZERO));
    }

    #[test]
    fn retries_at_now_plus_the_wait_until_attempts_run_out() {
        let p = RetryPolicy {
            jitter: 0.0,
            max_attempts: 4,
            ..policy()
        };
        let mut retry = Retry::new(p);
        let mut rng = SplitMix64::new(1);
        // A fake clock: each retry happens exactly when it was scheduled.
        let mut now = ms(5_000);
        let mut decisions = Vec::new();
        loop {
            let decision = retry.failed(now, &mut rng);
            decisions.push(decision);
            match decision {
                RetryDecision::RetryAt(at) => now = at,
                RetryDecision::GiveUp => break,
            }
        }
        assert_eq!(
            decisions,
            [
                RetryDecision::RetryAt(ms(5_100)),
                RetryDecision::RetryAt(ms(5_300)),
                RetryDecision::RetryAt(ms(5_700)),
                RetryDecision::GiveUp,
            ]
        );
        assert_eq!(retry.failures(), 4);
        // Still out of attempts afterwards.
        assert_eq!(retry.failed(now, &mut rng), RetryDecision::GiveUp);
    }

    #[test]
    fn one_attempt_never_retries_and_zero_counts_as_one() {
        for max_attempts in [0, 1] {
            let mut retry = Retry::new(RetryPolicy {
                max_attempts,
                ..policy()
            });
            assert_eq!(
                retry.failed(ms(0), &mut SplitMix64::new(0)),
                RetryDecision::GiveUp
            );
        }
    }

    #[test]
    fn seeded_jitter_is_reproducible() {
        let p = policy();
        let run = |seed| {
            let mut rng = SplitMix64::new(seed);
            (1..6)
                .map(|n| p.delay_after(n, &mut rng).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(run(42), run(42));
        assert_ne!(run(42), run(43));
    }

    #[test]
    fn default_policy_is_three_attempts() {
        let p = RetryPolicy::default();
        let mut rng = Fixed(0.0);
        assert_eq!(p.delay_after(1, &mut rng), Some(ms(500)));
        assert_eq!(p.delay_after(2, &mut rng), Some(ms(1_000)));
        assert_eq!(p.delay_after(3, &mut rng), None);
    }

    proptest! {
        #[test]
        fn splitmix_stays_in_the_unit_interval(seed in any::<u64>()) {
            let mut rng = SplitMix64::new(seed);
            for _ in 0..64 {
                let x = rng.next_unit();
                prop_assert!((0.0..1.0).contains(&x), "{x}");
            }
        }

        // Every wait lies between (1 - jitter) * backoff and backoff, and
        // backoff never exceeds the cap.
        #[test]
        fn waits_stay_within_jitter_bounds(
            seed in any::<u64>(),
            base in 1u64..5_000,
            cap in 1u64..60_000,
            multiplier in 1u32..5,
            jitter in 0.0f64..=1.0,
            failure in 1u32..20,
        ) {
            let p = RetryPolicy {
                max_attempts: 50,
                base_delay: ms(base),
                multiplier,
                max_delay: ms(cap),
                jitter,
            };
            let backoff = p.backoff(failure);
            prop_assert!(backoff <= ms(cap));
            let wait = p.delay_after(failure, &mut SplitMix64::new(seed)).unwrap();
            prop_assert!(wait <= backoff);
            prop_assert!(wait >= backoff.mul_f64(1.0 - jitter).saturating_sub(Duration::from_nanos(1)));
        }
    }
}
