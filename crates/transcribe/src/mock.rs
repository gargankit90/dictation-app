//! [`MockTranscriber`]: a fixed-text engine for tests and for running the
//! app without a provider (`--transcriber mock`).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dictation_ports::{Audio, TranscribeError, Transcriber};

/// The text a [`MockTranscriber::default`] returns.
pub const DEFAULT_TEXT: &str = "This is a test dictation.";

/// Answers every call with fixed text, at once. Clones share their
/// configuration and call count.
#[derive(Debug, Clone)]
pub struct MockTranscriber {
    state: Arc<Mutex<State>>,
}

#[derive(Debug)]
struct State {
    text: String,
    /// Answers for the next calls, in order, before `text` again.
    sequence: VecDeque<String>,
    calls: usize,
}

impl Default for MockTranscriber {
    fn default() -> Self {
        Self::new(DEFAULT_TEXT)
    }
}

impl MockTranscriber {
    /// Answers every call with `text`.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                text: text.into(),
                sequence: VecDeque::new(),
                calls: 0,
            })),
        }
    }

    /// Answers the next calls with `texts`, one per call in order, then
    /// with the usual text again (`--mock-text-sequence`). Shared with
    /// clones; returns the same mock for chaining.
    #[must_use]
    pub fn with_texts(self, texts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.lock()
            .sequence
            .extend(texts.into_iter().map(Into::into));
        self
    }

    /// Calls started so far.
    #[must_use]
    pub fn calls(&self) -> usize {
        self.lock().calls
    }

    // A test that panics while holding the lock poisons it; recovering the
    // guard keeps the mock usable. The state is plain data, valid at any point.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Transcriber for MockTranscriber {
    async fn transcribe(&self, _audio: &Audio) -> Result<String, TranscribeError> {
        let mut state = self.lock();
        state.calls += 1;
        Ok(state
            .sequence
            .pop_front()
            .unwrap_or_else(|| state.text.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio() -> Audio {
        Audio {
            sample_rate: 16_000,
            samples: vec![0.0; 1_600],
        }
    }

    #[tokio::test]
    async fn texts_come_in_order_then_the_usual_text() {
        let mock = MockTranscriber::new("usual").with_texts(["one", "two"]);
        for want in ["one", "two", "usual", "usual"] {
            assert_eq!(mock.transcribe(&audio()).await.unwrap(), want);
        }
        assert_eq!(mock.calls(), 4);
    }

    #[tokio::test]
    async fn returns_its_text() {
        let mock = MockTranscriber::new("hello world");
        assert_eq!(mock.transcribe(&audio()).await.unwrap(), "hello world");
        assert_eq!(mock.calls(), 1);
        assert_eq!(
            MockTranscriber::default()
                .transcribe(&audio())
                .await
                .unwrap(),
            DEFAULT_TEXT
        );
    }
}
