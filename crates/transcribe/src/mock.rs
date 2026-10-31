//! [`MockTranscriber`]: a configurable engine for tests and for running the
//! app without a provider (`--transcriber mock`).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use dictation_ports::{Audio, TranscribeError, Transcriber};

/// The text a [`MockTranscriber::default`] returns.
pub const DEFAULT_TEXT: &str = "This is a test dictation.";

/// Answers with fixed text after an optional delay, or with configured
/// errors. Clones share their configuration and call count.
///
/// The delay is a `tokio::time::sleep`, so tests with paused time
/// (`#[tokio::test(start_paused = true)]`) don't really wait, and dropping
/// the future cancels it like a real request.
#[derive(Debug, Clone)]
pub struct MockTranscriber {
    state: Arc<Mutex<State>>,
}

#[derive(Debug)]
struct State {
    text: String,
    delay: Duration,
    /// Errors for the next calls, in order, before the text again.
    queued: VecDeque<TranscribeError>,
    /// Returned on every call once `queued` is empty, instead of the text.
    always: Option<TranscribeError>,
    calls: usize,
}

impl Default for MockTranscriber {
    fn default() -> Self {
        Self::new(DEFAULT_TEXT)
    }
}

impl MockTranscriber {
    /// Answers every call with `text`, at once.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                text: text.into(),
                delay: Duration::ZERO,
                queued: VecDeque::new(),
                always: None,
                calls: 0,
            })),
        }
    }

    /// Waits `delay` before answering (or failing), on tokio's clock, so a
    /// test with paused time doesn't really wait. Applies to every later
    /// call, on this mock and its clones; returns the same mock for chaining.
    #[must_use]
    pub fn with_delay(self, delay: Duration) -> Self {
        self.lock().delay = delay;
        self
    }

    /// Fails the next calls with `errors`, one per call in order, then
    /// answers with the text again (or the [`always_failing`] error). Adds
    /// to any errors already queued; shared with clones; returns the same
    /// mock for chaining.
    ///
    /// [`always_failing`]: Self::always_failing
    #[must_use]
    pub fn failing_next(self, errors: impl IntoIterator<Item = TranscribeError>) -> Self {
        self.lock().queued.extend(errors);
        self
    }

    /// Fails every call with `error` once the queued errors from
    /// [`failing_next`](Self::failing_next) are used up, instead of
    /// answering. Replaces an earlier `always_failing` error; shared with
    /// clones; returns the same mock for chaining.
    #[must_use]
    pub fn always_failing(self, error: TranscribeError) -> Self {
        self.lock().always = Some(error);
        self
    }

    /// Calls started so far.
    #[must_use]
    pub fn calls(&self) -> usize {
        self.lock().calls
    }

    // A test that panics while holding the lock poisons it; recovering the
    // guard keeps the mock usable for the rest of the run instead of turning
    // one failure into many. The state is plain data, valid at any point.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes this call's answer and delay, counting the call.
    fn next_answer(&self) -> (Duration, Result<String, TranscribeError>) {
        let mut state = self.lock();
        state.calls += 1;
        let answer = match state.queued.pop_front() {
            Some(error) => Err(error),
            None => match &state.always {
                Some(error) => Err(error.clone()),
                None => Ok(state.text.clone()),
            },
        };
        (state.delay, answer)
    }
}

impl Transcriber for MockTranscriber {
    async fn transcribe(&self, _audio: &Audio) -> Result<String, TranscribeError> {
        let (delay, answer) = self.next_answer();
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::Instant;

    use super::*;

    fn audio() -> Audio {
        Audio {
            sample_rate: 16_000,
            samples: vec![0.0; 1_600],
        }
    }

    #[tokio::test]
    async fn returns_its_text() {
        let mock = MockTranscriber::new("hello world");
        assert_eq!(mock.transcribe(&audio()).await.unwrap(), "hello world");
        assert_eq!(mock.transcribe(&audio()).await.unwrap(), "hello world");
        assert_eq!(mock.calls(), 2);
        assert_eq!(
            MockTranscriber::default()
                .transcribe(&audio())
                .await
                .unwrap(),
            DEFAULT_TEXT
        );
    }

    #[tokio::test(start_paused = true)]
    async fn honours_its_delay_on_paused_time() {
        let mock = MockTranscriber::new("late").with_delay(Duration::from_secs(3));
        let started = Instant::now();
        assert_eq!(mock.transcribe(&audio()).await.unwrap(), "late");
        assert_eq!(started.elapsed(), Duration::from_secs(3));
    }

    #[tokio::test(start_paused = true)]
    async fn does_not_answer_before_the_delay() {
        let mock = MockTranscriber::new("late").with_delay(Duration::from_secs(3));
        let early =
            tokio::time::timeout(Duration::from_millis(2_999), mock.transcribe(&audio())).await;
        assert!(early.is_err(), "answered before its delay");
        assert_eq!(mock.calls(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn no_delay_answers_without_time_passing() {
        let mock = MockTranscriber::new("now");
        let started = Instant::now();
        mock.transcribe(&audio()).await.unwrap();
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    #[tokio::test]
    async fn queued_errors_come_first_then_the_text() {
        let mock = MockTranscriber::new("ok").failing_next([
            TranscribeError::RateLimited,
            TranscribeError::Network("reset".into()),
        ]);
        assert_eq!(
            mock.transcribe(&audio()).await,
            Err(TranscribeError::RateLimited)
        );
        assert_eq!(
            mock.transcribe(&audio()).await,
            Err(TranscribeError::Network("reset".into()))
        );
        assert_eq!(mock.transcribe(&audio()).await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn always_failing_fails_every_call_after_the_queue() {
        let mock = MockTranscriber::new("never")
            .failing_next([TranscribeError::Timeout])
            .always_failing(TranscribeError::InvalidKey);
        assert_eq!(
            mock.transcribe(&audio()).await,
            Err(TranscribeError::Timeout)
        );
        for _ in 0..3 {
            assert_eq!(
                mock.transcribe(&audio()).await,
                Err(TranscribeError::InvalidKey)
            );
        }
        assert_eq!(mock.calls(), 4);
    }

    #[tokio::test(start_paused = true)]
    async fn errors_also_wait_for_the_delay() {
        let mock = MockTranscriber::new("x")
            .with_delay(Duration::from_millis(250))
            .always_failing(TranscribeError::Engine("boom".into()));
        let started = Instant::now();
        assert_eq!(
            mock.transcribe(&audio()).await,
            Err(TranscribeError::Engine("boom".into()))
        );
        assert_eq!(started.elapsed(), Duration::from_millis(250));
    }

    #[tokio::test]
    async fn clones_share_configuration_and_calls() {
        let mock = MockTranscriber::new("shared");
        let handle = mock.clone().failing_next([TranscribeError::Timeout]);
        assert_eq!(
            mock.transcribe(&audio()).await,
            Err(TranscribeError::Timeout)
        );
        assert_eq!(handle.calls(), 1);
    }
}
