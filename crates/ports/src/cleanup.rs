//! Cleanup after transcription (Q2): text in, cleaned text out.

use std::future::Future;
use std::time::Duration;

use thiserror::Error;

/// Why a [`Cleaner::clean`] gave no reply. The messages never contain the
/// transcript, the reply or the API key. Whatever the error, the pipeline
/// pastes the raw transcript.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CleanupError {
    /// Cleanup is off: on-device dictation, the mock engine, or no cloud
    /// engine. Not a failure; nothing is counted.
    #[error("cleanup is off")]
    Off,
    /// No usable key: none saved, still being read, or the keychain
    /// refused.
    #[error("no API key is available for cleanup")]
    NoKey,
    /// The provider rejected the key (401, 403).
    #[error("the provider rejected the API key")]
    InvalidKey,
    /// The account has no credits left (402).
    #[error("the account is out of credits")]
    OutOfCredits,
    /// The provider is rate limiting (429).
    #[error("rate limited by the provider")]
    RateLimited,
    /// No provider can serve the model with the privacy filters the request
    /// carries (404, or a 400 about the routing). The filters are never
    /// dropped to get a route.
    #[error("no provider can take the request with its privacy filters")]
    NoRoute,
    /// The provider failed (5xx) or answered with another status; holds the
    /// HTTP status only.
    #[error("the provider failed (HTTP {0})")]
    Provider(u16),
    /// The request took too long.
    #[error("cleanup timed out")]
    Timeout,
    /// The provider couldn't be reached.
    #[error("couldn't reach the provider")]
    Network,
    /// The reply wasn't a chat reply with text.
    #[error("the reply couldn't be read")]
    BadReply,
}

/// A text cleanup engine (`OpenRouter`, or a fake in tests).
///
/// Threading: `&self`; the future is `Send`, runs on the pipeline's tokio
/// runtime and never blocks it. Dropping it cancels the request, which is
/// how the pipeline's deadline cancels a slow cleanup.
pub trait Cleaner: Send + Sync {
    /// The model's reply for `text`, without the transcript tags. The
    /// caller judges it (`dictation_core::cleanup::judge`) before pasting
    /// it. Implementations don't retry: the deadline is too short.
    ///
    /// # Errors
    ///
    /// A [`CleanupError`] as documented on each variant;
    /// [`CleanupError::Off`] when cleanup isn't configured.
    fn clean(&self, text: &str) -> impl Future<Output = Result<String, CleanupError>> + Send;

    /// How long this cleaner may take; `None` for the pipeline's default
    /// (`dictation_core::cleanup::TIMEOUT`). The on-device cleaner (Q4)
    /// needs longer on slower Macs.
    fn deadline(&self) -> Option<Duration> {
        None
    }

    /// The system prompt this cleaner sends, which the guard checks the
    /// reply doesn't echo; `None` for the cloud prompt
    /// (`dictation_core::cleanup::prompt`).
    fn prompt(&self) -> Option<&'static str> {
        None
    }
}

/// A [`Cleaner`] that is always off.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCleanup;

impl Cleaner for NoCleanup {
    async fn clean(&self, _text: &str) -> Result<String, CleanupError> {
        Err(CleanupError::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_hold_no_text() {
        assert_eq!(
            CleanupError::Provider(503).to_string(),
            "the provider failed (HTTP 503)"
        );
        assert_eq!(CleanupError::Off.to_string(), "cleanup is off");
    }
}
