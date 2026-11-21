//! Speech to text.

use std::fmt;
use std::future::Future;

use thiserror::Error;

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
            Self::Engine(text) => f
                .debug_tuple("Engine")
                .field(&redact::summary(text))
                .finish(),
        }
    }
}

impl TranscribeError {
    /// What to tell the user, in plain words and without the provider's
    /// name (the engine doesn't know it). Like the `Display` text it never
    /// holds the audio, the transcript or a key; unlike it, it says what to
    /// do next.
    #[must_use]
    pub fn user_message(&self) -> String {
        match self {
            Self::MissingKey => "Add your API key in Settings.".to_owned(),
            Self::KeyPending => "Still reading your API key from the Keychain. If a dialog asks, \
                 allow access, then try again."
                .to_owned(),
            Self::KeychainUnavailable(_) => "Couldn't read your API key from the Keychain. Unlock \
                 the Keychain or allow access, then try again, or enter the key again in Settings."
                .to_owned(),
            Self::InvalidKey => {
                "The provider rejected your API key. Check it in Settings.".to_owned()
            }
            Self::RateLimited => {
                "The provider is limiting requests right now. Try again in a minute.".to_owned()
            }
            Self::OutOfCredits => "Your OpenRouter credits ran out. Add credits.".to_owned(),
            Self::NoRoute => "No provider can take this dictation with Vocet's privacy settings \
                 right now. Try again in a few minutes."
                .to_owned(),
            Self::Timeout => {
                "The provider didn't answer in time. Check your internet connection.".to_owned()
            }
            Self::Network(_) => {
                "Couldn't reach the provider. Check your internet connection.".to_owned()
            }
            Self::ModelMissing => {
                "The on-device model isn't downloaded. Download the model in Settings.".to_owned()
            }
            Self::ModelLoading => {
                "The on-device model is still loading. Try again in a moment.".to_owned()
            }
            Self::Engine(detail) => format!("Something went wrong: {}.", redact::summary(detail)),
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
    fn every_error_has_a_message_that_says_what_to_do() {
        let cases = [
            (TranscribeError::MissingKey, "Add your API key in Settings."),
            (
                TranscribeError::InvalidKey,
                "The provider rejected your API key. Check it in Settings.",
            ),
            (
                TranscribeError::RateLimited,
                "The provider is limiting requests right now. Try again in a minute.",
            ),
            (
                TranscribeError::Timeout,
                "The provider didn't answer in time. Check your internet connection.",
            ),
            (
                TranscribeError::OutOfCredits,
                "Your OpenRouter credits ran out. Add credits.",
            ),
            (
                TranscribeError::NoRoute,
                "No provider can take this dictation with Vocet's privacy settings right now. Try again in a few minutes.",
            ),
            (
                TranscribeError::Network("dns error".into()),
                "Couldn't reach the provider. Check your internet connection.",
            ),
            (
                TranscribeError::KeyPending,
                "Still reading your API key from the Keychain. If a dialog asks, allow access, then try again.",
            ),
            (
                TranscribeError::KeychainUnavailable("keychain error: locked".into()),
                "Couldn't read your API key from the Keychain. Unlock the Keychain or allow access, then try again, or enter the key again in Settings.",
            ),
            (
                TranscribeError::ModelMissing,
                "The on-device model isn't downloaded. Download the model in Settings.",
            ),
            (
                TranscribeError::ModelLoading,
                "The on-device model is still loading. Try again in a moment.",
            ),
            (
                TranscribeError::Engine("the provider failed (HTTP 503)".into()),
                "Something went wrong: the provider failed (HTTP 503).",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.user_message(), message, "{error:?}");
        }
    }

    /// E1: the error code of a transcription failure is read back from its
    /// `user_message`, so every variant's message must land on its code.
    #[test]
    fn every_error_message_maps_to_its_error_code() {
        use dictation_core::diagnostics::ErrorCode;
        use dictation_core::error::DictationError;

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
                TranscribeError::Engine("couldn't load the on-device model: bad file".into()),
                ErrorCode::ModelLoadingFailed,
            ),
            (
                TranscribeError::Engine("the provider failed (HTTP 503)".into()),
                ErrorCode::EngineFailed,
            ),
        ];
        for (error, code) in cases {
            let shown = DictationError::TranscriptionFailed {
                reason: error.user_message(),
            };
            assert_eq!(ErrorCode::of(&shown), code, "{error:?}");
        }
    }

    /// #41: whatever a provider or library puts in these strings, the text
    /// that reaches the info log (`Display`) and the user is one short line
    /// without a key.
    #[test]
    fn display_and_user_message_hide_keys_and_cut_long_bodies() {
        let key = "gsk_dummy_NOT_A_REAL_KEY_0123456789";
        let body = format!(
            "{{\"error\": \"Invalid key {key}\"}}\n{}",
            "echoed audio ".repeat(80)
        );
        for error in [
            TranscribeError::Network(body.clone()),
            TranscribeError::Engine(body.clone()),
            TranscribeError::KeychainUnavailable(body),
        ] {
            for text in [error.to_string(), error.user_message()] {
                assert!(!text.contains(key), "{text}");
                assert!(!text.contains('\n'), "{text}");
                assert!(text.chars().count() < 300, "{text}");
            }
            let debug = format!("{error:?}");
            assert!(!debug.contains(key) && debug.len() < 300, "{debug}");
        }
    }

    #[test]
    fn keychain_details_stay_out_of_the_user_message() {
        let error = TranscribeError::KeychainUnavailable("keychain error: -25308".into());
        assert!(!error.user_message().contains("25308"));
        assert!(
            error.to_string().contains("25308"),
            "but the log keeps them"
        );
    }

    #[test]
    fn network_details_stay_out_of_the_user_message() {
        let error = TranscribeError::Network("secret-host.internal refused".into());
        assert!(!error.user_message().contains("secret-host"));
    }
}
