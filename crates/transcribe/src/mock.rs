//! [`MockTranscriber`]: a fixed-text engine for tests and for running the
//! app without a provider (`--transcriber mock`).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dictation_ports::{Audio, TranscribeError, Transcriber, Transcript};

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
    /// Reported by `transcribe_detailed`; `None` like an engine that
    /// doesn't say.
    no_speech_prob: Option<f64>,
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
                no_speech_prob: None,
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

    /// Reports `no_speech_prob` with the text from `transcribe_detailed`
    /// (by default none, like an engine that doesn't say). Shared with
    /// clones; returns the same mock for chaining.
    #[must_use]
    pub fn with_no_speech_prob(self, no_speech_prob: f64) -> Self {
        self.lock().no_speech_prob = Some(no_speech_prob);
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
    async fn transcribe(&self, audio: &Audio) -> Result<String, TranscribeError> {
        self.transcribe_detailed(audio).await.map(|t| t.text)
    }

    async fn transcribe_detailed(&self, _audio: &Audio) -> Result<Transcript, TranscribeError> {
        let mut state = self.lock();
        state.calls += 1;
        let text = state
            .sequence
            .pop_front()
            .unwrap_or_else(|| state.text.clone());
        Ok(Transcript {
            text,
            no_speech_prob: state.no_speech_prob,
        })
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
    async fn detailed_reports_the_configured_no_speech_prob() {
        let mock = MockTranscriber::new("Thank you.");
        let plain = mock.transcribe_detailed(&audio()).await.unwrap();
        assert_eq!(plain, Transcript::from_text("Thank you."));

        let silent = mock.clone().with_no_speech_prob(0.9);
        let detailed = silent.transcribe_detailed(&audio()).await.unwrap();
        assert_eq!(detailed.no_speech_prob, Some(0.9));
        // The plain call still gives just the text, and clones share state.
        assert_eq!(mock.transcribe(&audio()).await.unwrap(), "Thank you.");
        assert_eq!(mock.calls(), 3);
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
