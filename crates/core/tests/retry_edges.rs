//! M3.1 edge cases for `core::retry` (Tester, run 1): exact attempt counts
//! across policies, large failure numbers, huge caps, and a fake-clock run
//! with seeded jitter. No real time passes: the clock is a plain `Duration`.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::time::Duration;

use dictation_core::retry::{RandomSource, Retry, RetryDecision, RetryPolicy, SplitMix64};

struct Fixed(f64);

impl RandomSource for Fixed {
    fn next_unit(&mut self) -> f64 {
        self.0
    }
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn policy(max_attempts: u32, jitter: f64) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        base_delay: ms(100),
        multiplier: 3,
        max_delay: ms(2_000),
        jitter,
    }
}

/// Runs an always-failing operation on a fake clock; returns the attempt
/// times (the first at `start`).
fn run(p: RetryPolicy, start: Duration, rng: &mut impl RandomSource) -> Vec<Duration> {
    let mut retry = Retry::new(p);
    let mut now = start;
    let mut attempts = vec![now];
    while let RetryDecision::RetryAt(at) = retry.failed(now, rng) {
        assert!(at >= now, "retry scheduled in the past");
        now = at;
        attempts.push(now);
        assert!(attempts.len() <= 1_000, "never gave up");
    }
    attempts
}

#[test]
fn makes_exactly_max_attempts() {
    for n in 1..=12 {
        let attempts = run(policy(n, 0.3), ms(0), &mut SplitMix64::new(u64::from(n)));
        assert_eq!(attempts.len(), n as usize, "max_attempts {n}");
    }
    assert_eq!(run(policy(0, 0.3), ms(0), &mut SplitMix64::new(0)).len(), 1);
}

#[test]
fn multiplier_three_sequence_with_cap_on_a_fake_clock() {
    let attempts = run(policy(7, 0.0), ms(1_000), &mut Fixed(0.5));
    assert_eq!(
        attempts,
        [
            ms(1_000),
            ms(1_100), // +100
            ms(1_400), // +300
            ms(2_300), // +900
            ms(4_300), // +2000 (2700 capped)
            ms(6_300), // +2000
            ms(8_300), // +2000
        ]
    );
}

#[test]
fn seeded_runs_are_identical_and_within_bounds() {
    let p = policy(8, 0.25);
    let a = run(p, ms(0), &mut SplitMix64::new(7));
    let b = run(p, ms(0), &mut SplitMix64::new(7));
    assert_eq!(a, b);
    for (i, pair) in a.windows(2).enumerate() {
        let wait = pair[1] - pair[0];
        let backoff = p.backoff(u32::try_from(i).unwrap() + 1);
        assert!(wait <= backoff, "wait {wait:?} above backoff {backoff:?}");
        assert!(wait >= backoff.mul_f64(0.75), "wait {wait:?} too short");
        assert!(!wait.is_zero(), "zero wait with jitter < 1");
    }
}

#[test]
fn large_failure_numbers_stay_at_the_cap() {
    let p = policy(u32::MAX, 0.0);
    for n in [32, 64, 1_000, u32::MAX - 1] {
        assert_eq!(p.backoff(n), ms(2_000), "failure {n}");
        assert_eq!(p.delay_after(n, &mut Fixed(0.0)), Some(ms(2_000)));
    }
    assert_eq!(p.delay_after(u32::MAX, &mut Fixed(0.0)), None);
    assert_eq!(p.backoff(u32::MAX), ms(2_000));
}

#[test]
fn failures_counter_saturates_and_keeps_giving_up() {
    let mut retry = Retry::new(policy(2, 0.0));
    let mut rng = Fixed(0.0);
    assert!(matches!(
        retry.failed(ms(0), &mut rng),
        RetryDecision::RetryAt(_)
    ));
    for _ in 0..10 {
        assert_eq!(retry.failed(ms(0), &mut rng), RetryDecision::GiveUp);
    }
    assert_eq!(retry.failures(), 11);
}

#[test]
fn retry_time_saturates_at_the_end_of_the_clock() {
    let mut retry = Retry::new(policy(3, 0.0));
    assert_eq!(
        retry.failed(Duration::MAX, &mut Fixed(0.0)),
        RetryDecision::RetryAt(Duration::MAX)
    );
}

#[test]
fn zero_base_delay_retries_at_once() {
    let p = RetryPolicy {
        base_delay: Duration::ZERO,
        ..policy(3, 0.5)
    };
    assert_eq!(run(p, ms(10), &mut SplitMix64::new(1)), [ms(10); 3]);
}

#[test]
fn a_cap_below_the_base_wins() {
    let p = RetryPolicy {
        max_delay: ms(50),
        ..policy(3, 0.0)
    };
    assert_eq!(p.backoff(1), ms(50));
}

/// A huge cap (here the largest `Duration`) is a plausible "no cap" setting,
/// and the unit test `backoff_saturates_instead_of_overflowing` uses it. Fails
/// at 5597cc5: `Duration::mul_f64` panics converting ~2^64 s back from f64.
/// Un-ignore once `delay_after` saturates (Tester M3.1 run 1, finding 1).
#[test]
#[ignore = "M3.1 run 1 finding 1: delay_after panics with max_delay = Duration::MAX"]
fn a_huge_cap_does_not_panic_when_jitter_is_applied() {
    let p = RetryPolicy {
        multiplier: u32::MAX,
        max_delay: Duration::MAX,
        ..policy(50, 0.0)
    };
    let result = std::panic::catch_unwind(|| p.delay_after(40, &mut Fixed(0.0)));
    assert!(
        result.is_ok(),
        "delay_after panicked with max_delay = Duration::MAX"
    );
}

/// A NaN jitter (or a source that returns NaN) should not panic either.
/// Fails at 5597cc5: `f64::clamp` passes NaN through to `mul_f64`.
#[test]
#[ignore = "M3.1 run 1 finding 1: delay_after panics on NaN jitter"]
fn nan_jitter_does_not_panic() {
    let p = policy(3, f64::NAN);
    let result = std::panic::catch_unwind(|| p.delay_after(1, &mut Fixed(0.5)));
    assert!(result.is_ok(), "delay_after panicked with jitter = NaN");
    let result = std::panic::catch_unwind(|| policy(3, 0.5).delay_after(1, &mut Fixed(f64::NAN)));
    assert!(
        result.is_ok(),
        "delay_after panicked with a NaN random source"
    );
}
