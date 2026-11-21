//! Speech to text.

use std::fmt;
use std::future::Future;

use thiserror::Error;

use dictation_core::diagnostics::ErrorCode;
use dictation_core::redact;

use crate::Audio;

/// Why a [`Transcriber::transcribe`] failed. The messages never contain the
/// audio, the transcript or the API key.
#[derive(Clone, PartialEq, Eq, Error)]
pub enum TranscribeError {
    /// No API key is stored for the provider.
    #[error("no API key is saved")]
    MissingKey,
    /// The API key is still being read from the keychain (the app reads it
    /// off the startup path, so a locked keychain can't block startup).
    #[error("the API key is still being read from the keychain")]
    KeyPending,
    /// The keychain couldn't give the key: it is locked, needs the user to
    /// allow access, refused, or didn't answer in time. The text says why
    /// and never holds the key.
    #[error("couldn't read the API key from the keychain: {}", redact::summary(.0))]
    KeychainUnavailable(String),
    /// The provider rejected the API key.
    #[error("the API key is missing or invalid")]
    InvalidKey,
    /// The provider is rate limiting us; retrying later may work.
    #[error("rate limited by the provider")]
    RateLimited,
    /// The `OpenRouter` account has no credits left (402).
    #[error("the account is out of credits")]
    OutOfCredits,
    /// No provider can serve the model with the privacy filters the request
    /// carries (Q2). The filters are never dropped to find one.
    #[error("no provider can take the request with its privacy filters")]
    NoRoute,
    /// The request took too long.
    #[error("transcription timed out")]
    Timeout,
    /// The provider couldn't be reached. The text is a short summary that
    /// holds no key, transcript or provider reply body (#41): `Display`
    /// cuts it to one short line with any key hidden
    /// ([`dictation_core::redact::summary`]), as a backstop.
    #[error("network error: {}", redact::summary(.0))]
    Network(String),
    /// The on-device engine is selected but its model isn't downloaded (or
    /// the catalogue no longer has the saved one), so there is nothing to
    /// run (L1).
    #[error("the on-device model isn't downloaded")]
    ModelMissing,
    /// The on-device model is being checked and loaded (seconds, after a
    /// switch of model); dictation works again once it is (L1).
    #[error("the on-device model is still loading")]
    ModelLoading,
    /// The on-device model's file couldn't be loaded (damaged, or not a
    /// model transcribe.cpp knows). The text says why, for the log.
    #[error("couldn't load the on-device model: {}", redact::summary(.0))]
    ModelLoadFailed(String),
    /// The provider or engine returned an error or a response we can't read.
    /// Same rule as [`Network`](Self::Network): a short summary such as
    /// "the provider failed (HTTP 503)", never the reply body.
    #[error("transcription engine error: {}", redact::summary(.0))]
    Engine(String),
}

/// Like the derived `Debug`, but the text of a variant shows as its
/// [`summary`](dictation_core::redact::summary), so logging `?error` can't
/// leak a key or a reply body either.
impl fmt::Debug for TranscribeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingKey => f.write_str("MissingKey"),
            Self::KeyPending => f.write_str("KeyPending"),
            Self::KeychainUnavailable(text) => f
                .debug_tuple("KeychainUnavailable")
                .field(&redact::summary(text))
                .finish(),
            Self::InvalidKey => f.write_str("InvalidKey"),
            Self::RateLimited => f.write_str("RateLimited"),
            Self::OutOfCredits => f.write_str("OutOfCredits"),
            Self::NoRoute => f.write_str("NoRoute"),
            Self::Timeout => f.write_str("Timeout"),
            Self::Network(text) => f
                .debug_tuple("Network")
                .field(&redact::summary(text))
                .finish(),
            Self::ModelMissing => f.write_str("ModelMissing"),
            Self::ModelLoading => f.write_str("ModelLoading"),
            Self::ModelLoadFailed(text) => f
                .debug_tuple("ModelLoadFailed")
                .field(&redact::summary(text))
                .finish(),
            Self::Engine(text) => f
                .debug_tuple("Engine")
                .field(&redact::summary(text))
                .finish(),
        }
    }
}

impl TranscribeError {
    /// The code the user sees for this error (its sentence and fix are in
    /// `dictation_core::diagnostics`). The details stay in the `Display`
    /// text, for the log.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::MissingKey => ErrorCode::KeyMissing,
            Self::KeyPending | Self::KeychainUnavailable(_) => ErrorCode::KeyUnreadable,
            Self::InvalidKey => ErrorCode::KeyRejected,
            Self::RateLimited => ErrorCode::ProviderQuota,
            Self::OutOfCredits => ErrorCode::OutOfCredits,
            Self::Timeout | Self::Network(_) => ErrorCode::NetworkUnreachable,
            Self::ModelMissing => ErrorCode::ModelMissing,
            Self::ModelLoading => ErrorCode::ModelLoading,
            Self::ModelLoadFailed(_) => ErrorCode::ModelLoadingFailed,
            Self::NoRoute | Self::Engine(_) => ErrorCode::EngineFailed,
        }
    }
}

/// A speech-to-text engine (`OpenRouter`, a mock, a local model).
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
    /// [`TranscribeError::MissingKey`], [`TranscribeError::InvalidKey`],
    /// [`TranscribeError::RateLimited`],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_has_its_code() {
        let cases = [
            (TranscribeError::MissingKey, ErrorCode::KeyMissing),
            (TranscribeError::KeyPending, ErrorCode::KeyUnreadable),
            (
                TranscribeError::KeychainUnavailable("locked".into()),
                ErrorCode::KeyUnreadable,
            ),
            (TranscribeError::InvalidKey, ErrorCode::KeyRejected),
            (TranscribeError::RateLimited, ErrorCode::ProviderQuota),
            (TranscribeError::OutOfCredits, ErrorCode::OutOfCredits),
            (TranscribeError::NoRoute, ErrorCode::EngineFailed),
            (TranscribeError::Timeout, ErrorCode::NetworkUnreachable),
            (
                TranscribeError::Network("dns".into()),
                ErrorCode::NetworkUnreachable,
            ),
            (TranscribeError::ModelMissing, ErrorCode::ModelMissing),
            (TranscribeError::ModelLoading, ErrorCode::ModelLoading),
            (
                TranscribeError::ModelLoadFailed("bad file".into()),
                ErrorCode::ModelLoadingFailed,
            ),
            (
                TranscribeError::Engine("the provider failed (HTTP 503)".into()),
                ErrorCode::EngineFailed,
            ),
        ];
        for (error, code) in cases {
            assert_eq!(error.code(), code, "{error:?}");
        }
    }

    /// #41: whatever a provider or library puts in these strings, the text
    /// that reaches the info log (`Display`) is one short line without a key.
    #[test]
    fn display_and_debug_hide_keys_and_cut_long_bodies() {
        let key = "gsk_dummy_NOT_A_REAL_KEY_0123456789";
        let body = format!(
            "{{\"error\": \"Invalid key {key}\"}}\n{}",
            "echoed audio ".repeat(80)
        );
        for error in [
            TranscribeError::Network(body.clone()),
            TranscribeError::Engine(body.clone()),
            TranscribeError::ModelLoadFailed(body.clone()),
            TranscribeError::KeychainUnavailable(body),
        ] {
            let text = error.to_string();
            assert!(!text.contains(key), "{text}");
            assert!(!text.contains('\n'), "{text}");
            assert!(text.chars().count() < 300, "{text}");
            let debug = format!("{error:?}");
            assert!(!debug.contains(key) && debug.len() < 300, "{debug}");
        }
    }
}
