//! M3.1 edge cases for `MockTranscriber` (Tester, run 1): cancellation,
//! concurrent calls, back-to-back delays and empty configuration, all on
//! paused tokio time (no real waiting).

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::time::Duration;

use dictation_ports::{Audio, TranscribeError, Transcriber};
use dictation_transcribe::mock::{DEFAULT_TEXT, MockTranscriber};
use tokio::time::Instant;

fn audio() -> Audio {
    Audio {
        sample_rate: 16_000,
        samples: Vec::new(),
    }
}

#[tokio::test(start_paused = true)]
async fn back_to_back_calls_each_wait_the_delay() {
    let mock = MockTranscriber::new("a").with_delay(Duration::from_millis(400));
    let started = Instant::now();
    for _ in 0..3 {
        assert_eq!(mock.transcribe(&audio()).await.unwrap(), "a");
    }
    assert_eq!(started.elapsed(), Duration::from_millis(1_200));
    assert_eq!(mock.calls(), 3);
}

#[tokio::test(start_paused = true)]
async fn concurrent_calls_wait_in_parallel_and_take_queued_errors_in_order() {
    let mock = MockTranscriber::new("ok")
        .with_delay(Duration::from_secs(1))
        .failing_next([TranscribeError::Timeout]);
    let started = Instant::now();
    let a = audio();
    let (first, second) = tokio::join!(mock.transcribe(&a), mock.transcribe(&a));
    assert_eq!(first, Err(TranscribeError::Timeout));
    assert_eq!(second.unwrap(), "ok");
    assert_eq!(started.elapsed(), Duration::from_secs(1));
    assert_eq!(mock.calls(), 2);
}

/// A call cancelled during its delay still counts and still uses up the
/// queued error it took (documented: the answer is taken at the start).
#[tokio::test(start_paused = true)]
async fn a_cancelled_call_counts_and_consumes_its_answer() {
    let mock = MockTranscriber::new("ok")
        .with_delay(Duration::from_secs(2))
        .failing_next([TranscribeError::RateLimited]);
    let cut = tokio::time::timeout(Duration::from_secs(1), mock.transcribe(&audio())).await;
    assert!(cut.is_err());
    assert_eq!(mock.calls(), 1);
    assert_eq!(mock.transcribe(&audio()).await.unwrap(), "ok");
    assert_eq!(mock.calls(), 2);
}

#[tokio::test]
async fn empty_text_and_empty_audio_are_fine() {
    let mock = MockTranscriber::new("");
    assert_eq!(mock.transcribe(&audio()).await.unwrap(), "");
    assert_eq!(MockTranscriber::default().calls(), 0);
    assert_eq!(
        MockTranscriber::default()
            .transcribe(&audio())
            .await
            .unwrap(),
        DEFAULT_TEXT
    );
}

#[tokio::test]
async fn failing_next_extends_the_queue_and_empty_queue_is_a_no_op() {
    let mock = MockTranscriber::new("ok")
        .failing_next([])
        .failing_next([TranscribeError::Timeout])
        .failing_next([TranscribeError::InvalidKey]);
    assert_eq!(
        mock.transcribe(&audio()).await,
        Err(TranscribeError::Timeout)
    );
    assert_eq!(
        mock.transcribe(&audio()).await,
        Err(TranscribeError::InvalidKey)
    );
    assert_eq!(mock.transcribe(&audio()).await.unwrap(), "ok");
}

#[tokio::test]
async fn always_failing_set_later_still_lets_queued_errors_go_first() {
    let mock = MockTranscriber::new("never")
        .always_failing(TranscribeError::Engine("down".into()))
        .failing_next([TranscribeError::RateLimited])
        .always_failing(TranscribeError::InvalidKey);
    assert_eq!(
        mock.transcribe(&audio()).await,
        Err(TranscribeError::RateLimited)
    );
    // The last always_failing wins.
    assert_eq!(
        mock.transcribe(&audio()).await,
        Err(TranscribeError::InvalidKey)
    );
}

#[tokio::test]
async fn mock_works_behind_a_generic_transcriber_bound() {
    async fn twice<T: Transcriber>(
        t: &T,
    ) -> (
        Result<String, TranscribeError>,
        Result<String, TranscribeError>,
    ) {
        let a = audio();
        (t.transcribe(&a).await, t.transcribe(&a).await)
    }
    let mock = MockTranscriber::new("x").failing_next([TranscribeError::Timeout]);
    let (first, second) = twice(&mock).await;
    assert_eq!(first, Err(TranscribeError::Timeout));
    assert_eq!(second.unwrap(), "x");
}
