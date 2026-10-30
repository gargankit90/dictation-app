//! Speech to text.

use std::future::Future;

use thiserror::Error;

use crate::Audio;

/// Why a [`Transcriber::transcribe`] failed. The messages never contain the
/// audio, the transcript or the API key.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TranscribeError {
    /// No API key is stored, or the provider rejected it.
    #[error("the API key is missing or invalid")]
    InvalidKey,
    /// The provider is rate limiting us; retrying later may work.
    #[error("rate limited by the provider")]
    RateLimited,
    /// The request took too long.
    #[error("transcription timed out")]
    Timeout,
    /// The provider couldn't be reached.
    #[error("network error: {0}")]
    Network(String),
    /// The provider or engine returned an error or a response we can't read.
    #[error("transcription engine error: {0}")]
    Engine(String),
}

/// A speech-to-text engine (Groq, OpenAI, a mock, later local models).
///
/// Threading: `&self`, so one engine can serve overlapping calls. The future
/// is `Send` and runs on the pipeline's tokio runtime; it must not block the
/// executor (network I/O is async; CPU-heavy local models move to a blocking
/// thread). Dropping the future cancels the request.
pub trait Transcriber: Send + Sync {
    /// Turns `audio` into text. Silence may give an empty string; the
    /// coordinator treats that as "no speech".
    ///
    /// # Errors
    ///
    /// [`TranscribeError::InvalidKey`], [`TranscribeError::RateLimited`],
    /// [`TranscribeError::Timeout`], [`TranscribeError::Network`] or
    /// [`TranscribeError::Engine`], as documented on each variant.
    /// Implementations don't retry; the pipeline owns the retry policy.
    ///
    /// Threading: the returned future is `Send`, runs on the tokio runtime
    /// and never blocks it; dropping it cancels the request.
    fn transcribe(
        &self,
        audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send;
}
