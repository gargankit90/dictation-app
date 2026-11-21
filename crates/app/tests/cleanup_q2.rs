//! Q2 criterion 2: the cleanup pass in the pipeline, with fakes. A cleaned
//! reply is pasted; a timeout, an error and a reply the guard refuses each
//! paste the raw transcript; a timed-out cleanup is cancelled and doesn't
//! hold up the next dictation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail the test on purpose"
)]

mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use common::{CLEANED, Harness, RAW, Script, WAIT};
use dictation_pipeline::pipeline::UiEvent;
use dictation_ports::{CleanupError, CleanupOutcome};

#[tokio::test]
async fn q2_criterion_2_the_cleaned_text_is_pasted() {
    let mut harness = Harness::start(WAIT, vec![Script::Reply(CLEANED)]);
    let events = harness.dictate().await;
    assert_eq!(Harness::outcomes(&events), [CleanupOutcome::Applied]);
    let (inserter, cleaner) = harness.finish().await;
    assert_eq!(inserter.inserted(), [CLEANED]);
    assert_eq!(*cleaner.seen.lock().unwrap(), [RAW], "the raw transcript");
}

#[tokio::test]
async fn q2_criterion_2_an_error_pastes_the_raw_text() {
    for error in [
        CleanupError::InvalidKey,
        CleanupError::OutOfCredits,
        CleanupError::RateLimited,
        CleanupError::NoRoute,
        CleanupError::Provider(503),
        CleanupError::Network,
        CleanupError::BadReply,
    ] {
        let mut harness = Harness::start(WAIT, vec![Script::Fail(error.clone())]);
        let events = harness.dictate().await;
        assert_eq!(
            Harness::outcomes(&events),
            [CleanupOutcome::FallbackError],
            "{error}"
        );
        assert!(
            !events.iter().any(|e| matches!(e, UiEvent::Error(_))),
            "a cleanup error is never shown: {events:?}"
        );
        let (inserter, _) = harness.finish().await;
        assert_eq!(inserter.inserted(), [RAW], "{error}");
    }
}

#[tokio::test]
async fn q2_criterion_2_a_reply_the_guard_refuses_pastes_the_raw_text() {
    // Drops the name and the number.
    let mut harness = Harness::start(WAIT, vec![Script::Reply("Send the report to her.")]);
    let events = harness.dictate().await;
    assert_eq!(Harness::outcomes(&events), [CleanupOutcome::FallbackGuard]);
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), [RAW]);
}

#[tokio::test]
async fn q2_criterion_2_a_timeout_pastes_the_raw_text_cancels_and_frees_the_next_dictation() {
    let timeout = Duration::from_millis(150);
    let mut harness = Harness::start(
        timeout,
        vec![
            Script::Hang,
            Script::Fail(CleanupError::Timeout),
            Script::Reply(CLEANED),
        ],
    );
    let started = tokio::time::Instant::now();
    let events = harness.dictate().await;
    assert_eq!(
        Harness::outcomes(&events),
        [CleanupOutcome::FallbackTimeout]
    );
    // Cancelled: the hung request's future was dropped at the deadline.
    assert_eq!(harness.cleaner.dropped.load(Ordering::SeqCst), 1);
    // The client's own timeout counts as a timeout too.
    let events = harness.dictate().await;
    assert_eq!(
        Harness::outcomes(&events),
        [CleanupOutcome::FallbackTimeout]
    );
    // The next dictation is cleaned, with no wait left over.
    let events = harness.dictate().await;
    assert_eq!(Harness::outcomes(&events), [CleanupOutcome::Applied]);
    assert!(started.elapsed() < WAIT, "took {:?}", started.elapsed());
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), [RAW, RAW, CLEANED]);
}

#[tokio::test]
async fn q2_cleanup_off_pastes_the_raw_text_and_counts_nothing() {
    let mut harness = Harness::start(WAIT, vec![]);
    let events = harness.dictate().await;
    assert!(Harness::outcomes(&events).is_empty(), "{events:?}");
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), [RAW]);
}

/// QT1: a cleaned dictation, a deadline and a cleaned one again: the
/// cancelled request leaves nothing behind.
#[tokio::test]
async fn qt1_cleaned_then_raw_at_the_deadline_then_cleaned_again() {
    let mut harness = Harness::start(
        Duration::from_millis(150),
        vec![Script::Reply(CLEANED), Script::Hang, Script::Reply(CLEANED)],
    );
    let mut outcomes = Vec::new();
    for _ in 0..3 {
        outcomes.extend(Harness::outcomes(&harness.dictate().await));
    }
    assert_eq!(
        outcomes,
        [
            CleanupOutcome::Applied,
            CleanupOutcome::FallbackTimeout,
            CleanupOutcome::Applied
        ]
    );
    assert_eq!(harness.cleaner.dropped.load(Ordering::SeqCst), 1);
    let (inserter, _) = harness.finish().await;
    assert_eq!(inserter.inserted(), [CLEANED, RAW, CLEANED]);
}
