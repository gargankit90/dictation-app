//! [`OpenRouter`]: speech to text through `OpenRouter`'s OpenAI-compatible
//! `POST {base_url}/audio/transcriptions` endpoint (Q2), with the user's own
//! key.
//!
//! The audio goes up as a 16-bit mono WAV (`dictation_core::wav`),
//! base64-encoded in a JSON body (`input_audio: {data, format: "wav"}`, the
//! route the hosted-ASR eval used), with the model,
//! a `prompt` only for a model whose providers get it from `OpenRouter`
//! ([`dictation_core::cloud_models::CloudModel::prompt`]), and the model's
//! privacy filter as `provider: {data_collection: "deny"}` (plus
//! `zdr: true` where the catalog asks for it). The filter is on every
//! request and never dropped: if no provider matches, the request fails
//! (closed). `HTTP-Referer` and `X-Title` attribute the requests to the app.
//!
//! The reply is `{"text": …}`.
//!
//! Each request may take [`DEFAULT_TIMEOUT`] plus half a second per second
//! of audio, so a long dictation on a slow connection still gets up.
//! Rate limits (429), server errors (5xx), timeouts and network errors are
//! retried under a [`RetryPolicy`]; a `Retry-After` header (delay seconds)
//! on a 429 or 5xx replaces the backoff, up to [`MAX_RETRY_AFTER`]. A
//! rejected key (401, 403), no credits (402), a redirect (never followed),
//! any other 4xx and an unreadable reply are not retried; a 404 is `OpenRouter`'s
//! "no eligible route" for the model with its privacy filter
//! ([`TranscribeError::NoRoute`]). There is no fallback to another model:
//! switching models is a catalog change.
//!
//! Privacy: the key is a [`Secret`] and goes only into the `Authorization`
//! header (marked sensitive). Errors and logs never contain the key, the
//! audio or the transcript; reply bodies of failed requests are never read;
//! logs are at debug and hold statuses, sizes and lengths only.

use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use dictation_core::cloud_models::{self, PrivacyFilter};
use dictation_core::retry::{RetryPolicy, SplitMix64};
use dictation_core::wav;
use dictation_ports::{Audio, Secret, TranscribeError, Transcriber};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

/// The longest `Retry-After` honoured; a provider asking for longer gets
/// [`TranscribeError::RateLimited`] instead of a long silent wait.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// How long one request may take before it counts as a timeout, before
/// [`Config::timeout_grows_with_audio`] adds time for a long clip.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// `OpenRouter`'s API root; `/audio/transcriptions` and `/chat/completions`
/// are appended.
pub const BASE_URL: &str = "https://openrouter.ai/api/v1";

/// The `HTTP-Referer` header: the app's site, for `OpenRouter`'s app
/// attribution.
pub const REFERER: &str = "https://vocetlabs.com";

/// The `X-Title` header: the app's name, for `OpenRouter`'s app attribution.
pub const TITLE: &str = "Vocet";

/// The headers every `OpenRouter` request carries: the key (sensitive, never
/// logged) and the app attribution.
///
/// # Errors
///
/// [`TranscribeError::InvalidKey`] if the key can't be a header value; the
/// error never echoes it.
pub(crate) fn headers(key: &Secret) -> Result<HeaderMap, TranscribeError> {
    let mut value = HeaderValue::from_str(&format!("Bearer {}", key.expose()))
        .map_err(|_| TranscribeError::InvalidKey)?;
    value.set_sensitive(true);
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, value);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("http-referer", HeaderValue::from_static(REFERER));
    headers.insert("x-title", HeaderValue::from_static(TITLE));
    Ok(headers)
}

/// The `provider` object for `filter`.
pub(crate) fn provider_filter(filter: PrivacyFilter) -> serde_json::Value {
    if filter.zdr() {
        serde_json::json!({ "zdr": true, "data_collection": "deny" })
    } else {
        serde_json::json!({ "data_collection": "deny" })
    }
}

/// Standard base64 with padding (RFC 4648), for the audio in the JSON body.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Waits between attempts. [`TokioWait`] in the app; tests record the
/// waits instead, so retries never really sleep.
pub trait Wait: Send + Sync {
    /// Waits `duration`.
    fn wait(&self, duration: Duration) -> impl Future<Output = ()> + Send;
}

/// [`Wait`] with `tokio::time::sleep`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioWait;

impl Wait for TokioWait {
    async fn wait(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

/// Settings for an [`OpenRouter`] engine.
#[derive(Debug, Clone)]
pub struct Config {
    /// The API root, [`BASE_URL`] unless a test points it elsewhere.
    pub base_url: String,
    /// The speech model's id.
    pub model: String,
    /// Text that helps recognition (names, terms), sent as `prompt` only if
    /// set. The engine builder sets it only for a model whose providers get
    /// it ([`cloud_models::CloudModel::prompt`]); nothing sets one yet.
    pub prompt: Option<String>,
    /// The `provider` filter on every request.
    pub privacy: PrivacyFilter,
    /// Per-request timeout, for a clip of no length.
    pub timeout: Duration,
    /// Whether each request gets half a second more per second of audio.
    /// On for dictation; off for the key check, which has a fixed budget.
    pub timeout_grows_with_audio: bool,
    /// Retries for rate limits, server errors, timeouts and network errors.
    pub retry: RetryPolicy,
}

impl Config {
    /// The catalog's default speech model, at
    /// [`BASE_URL`], the default timeout and retry policy.
    #[must_use]
    pub fn openrouter() -> Self {
        Self::for_model(cloud_models::DEFAULT_SPEECH)
    }

    /// `model` (a catalog id, or any other id) with the catalog's privacy
    /// filter for it (the stricter zero-retention filter for an id the
    /// catalog doesn't have).
    #[must_use]
    pub fn for_model(model: &str) -> Self {
        let entry = cloud_models::speech(model);
        Self {
            base_url: BASE_URL.to_owned(),
            model: model.to_owned(),
            prompt: None,
            privacy: entry.map_or(PrivacyFilter::ZeroDataRetention, |m| m.privacy),
            timeout: DEFAULT_TIMEOUT,
            timeout_grows_with_audio: true,
            retry: RetryPolicy::default(),
        }
    }
}

/// The `OpenRouter` speech-to-text engine.
pub struct OpenRouter<W = TokioWait> {
    client: Client,
    endpoint: String,
    config: Config,
    key: Secret,
    wait: W,
    rng: Arc<Mutex<SplitMix64>>,
}

impl OpenRouter<TokioWait> {
    /// An engine for `config` with the user's `key`.
    ///
    /// # Errors
    ///
    /// [`TranscribeError::Engine`] if the HTTP client can't be built.
    pub fn new(config: Config, key: Secret) -> Result<Self, TranscribeError> {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self::with_wait(config, key, TokioWait, seed)
    }
}

impl<W: Wait> OpenRouter<W> {
    /// An engine that waits between attempts with `wait` and draws jitter
    /// from a generator seeded with `seed`.
    ///
    /// # Errors
    ///
    /// [`TranscribeError::Engine`] if the HTTP client can't be built.
    pub fn with_wait(
        config: Config,
        key: Secret,
        wait: W,
        seed: u64,
    ) -> Result<Self, TranscribeError> {
        let client = crate::http::client(&config.base_url)?;
        let endpoint = format!(
            "{}/audio/transcriptions",
            config.base_url.trim_end_matches('/')
        );
        Ok(Self {
            client,
            endpoint,
            config,
            key,
            wait,
            rng: Arc::new(Mutex::new(SplitMix64::new(seed))),
        })
    }

    /// Transcribes `audio`.
    ///
    /// # Errors
    ///
    /// As [`Transcriber::transcribe`], after retries.
    async fn transcribe_retrying(&self, audio: &Audio) -> Result<String, TranscribeError> {
        let wav = wav::encode_f32(&audio.samples, audio.sample_rate)
            .map_err(|e| TranscribeError::Engine(format!("could not encode the audio: {e}")))?;
        let timeout = if self.config.timeout_grows_with_audio {
            self.config.timeout.saturating_add(audio.duration() / 2)
        } else {
            self.config.timeout
        };
        // Encoded once; each attempt only changes the model.
        let audio_b64 = base64(&wav);
        tracing::debug!(bytes = wav.len(), model = %self.config.model, ?timeout, "transcribing");
        let mut failures = 0u32;
        loop {
            let error = match self.attempt(&audio_b64, timeout).await {
                Ok(text) => {
                    tracing::debug!(chars = text.chars().count(), "transcribed");
                    return Ok(text);
                }
                Err(failure) => failure,
            };
            failures = failures.saturating_add(1);
            let Some(delay) = self.retry_delay(&error, failures) else {
                tracing::debug!(failures, error = %error.error, "giving up");
                return Err(error.error);
            };
            tracing::debug!(failures, error = %error.error, ?delay, "retrying");
            self.wait.wait(delay).await;
        }
    }

    /// How long to wait before the next attempt, or `None` to give up.
    fn retry_delay(&self, failure: &Failure, failures: u32) -> Option<Duration> {
        if !failure.retryable {
            return None;
        }
        let backoff = {
            let mut rng = self.rng.lock().unwrap_or_else(PoisonError::into_inner);
            self.config.retry.delay_after(failures, &mut *rng)
        }?;
        match failure.retry_after {
            Some(asked) if asked > MAX_RETRY_AFTER => None,
            Some(asked) => Some(asked),
            None => Some(backoff),
        }
    }

    async fn attempt(&self, audio_b64: &str, timeout: Duration) -> Result<String, Failure> {
        let body = self.request_body(audio_b64);
        let response = self
            .client
            .post(&self.endpoint)
            .headers(self.auth_header()?)
            .timeout(timeout)
            .body(body.to_string())
            .send()
            .await
            .map_err(Failure::from_reqwest)?;

        let status = response.status();
        if !status.is_success() {
            return Err(Failure::from_status(status, response.headers()));
        }
        let body = response.bytes().await.map_err(Failure::from_reqwest)?;
        parse(&body).map_err(Failure::fatal)
    }

    /// The JSON body: the audio, the model, the privacy filter (always),
    /// and the prompt if set. No language: the model detects it.
    fn request_body(&self, audio_b64: &str) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": self.config.model,
            "input_audio": { "data": audio_b64, "format": "wav" },
            "provider": provider_filter(self.config.privacy),
        });
        if let Some(prompt) = &self.config.prompt {
            body["prompt"] = serde_json::json!(prompt);
        }
        body
    }

    fn auth_header(&self) -> Result<HeaderMap, Failure> {
        // The error never echoes the key.
        headers(&self.key).map_err(Failure::fatal)
    }
}

impl<W: Wait> Transcriber for OpenRouter<W> {
    async fn transcribe(&self, audio: &Audio) -> Result<String, TranscribeError> {
        self.transcribe_retrying(audio).await
    }
}

impl<W> std::fmt::Debug for OpenRouter<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenRouter")
            .field("endpoint", &self.endpoint)
            .field("config", &self.config)
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

/// A failed attempt and whether to retry it.
#[derive(Debug)]
struct Failure {
    error: TranscribeError,
    retryable: bool,
    retry_after: Option<Duration>,
}

impl Failure {
    fn fatal(error: TranscribeError) -> Self {
        Self {
            error,
            retryable: false,
            retry_after: None,
        }
    }

    fn retry(error: TranscribeError, retry_after: Option<Duration>) -> Self {
        Self {
            error,
            retryable: true,
            retry_after,
        }
    }

    fn from_status(status: StatusCode, headers: &HeaderMap) -> Self {
        // The body isn't used: it could echo part of the key or the request.
        let retry_after = headers
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse().ok())
            .map(Duration::from_secs);
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Self::fatal(TranscribeError::InvalidKey)
            }
            StatusCode::PAYMENT_REQUIRED => Self::fatal(TranscribeError::OutOfCredits),
            StatusCode::TOO_MANY_REQUESTS => Self::retry(TranscribeError::RateLimited, retry_after),
            // OpenRouter's "no endpoints found" for the model with this
            // filter. Not retried: the same request gets the same answer.
            StatusCode::NOT_FOUND => Self::fatal(TranscribeError::NoRoute),
            // Never followed (issue #76) and never retried: the same request
            // gets the same answer.
            s if s.is_redirection() => Self::fatal(TranscribeError::Engine(format!(
                "the provider answered with a redirect (HTTP {}), which isn't followed",
                s.as_u16()
            ))),
            s if s.is_server_error() => Self::retry(
                TranscribeError::Engine(format!("the provider failed (HTTP {})", s.as_u16())),
                retry_after,
            ),
            s => Self::fatal(TranscribeError::Engine(format!(
                "the provider rejected the request (HTTP {})",
                s.as_u16()
            ))),
        }
    }

    fn from_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::retry(TranscribeError::Timeout, None)
        } else if error.is_decode() || error.is_body() {
            Self::retry(
                TranscribeError::Network("the reply was cut off".into()),
                None,
            )
        } else {
            // Without the URL: only the host matters, and it's in the config.
            Self::retry(
                TranscribeError::Network(error.without_url().to_string()),
                None,
            )
        }
    }
}

#[derive(Deserialize)]
struct Reply {
    text: String,
}

/// Parses a reply's text, trimmed. The error never quotes the body, which
/// could hold the transcript.
fn parse(body: &[u8]) -> Result<String, TranscribeError> {
    let reply: Reply = serde_json::from_slice(body).map_err(|e| {
        TranscribeError::Engine(format!("unreadable reply ({:?} error)", e.classify()))
    })?;
    Ok(reply.text.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_points_at_openrouter_with_the_catalog_default() {
        let config = Config::openrouter();
        assert_eq!(config.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(config.model, "openai/gpt-4o-mini-transcribe");
        assert_eq!(config.privacy, PrivacyFilter::DenyDataCollection);
        assert_eq!(config.timeout, DEFAULT_TIMEOUT);
        assert_eq!(config.prompt, None);
        // An id the catalog doesn't know gets the strictest filter.
        assert_eq!(
            Config::for_model("someone/new-model").privacy,
            PrivacyFilter::ZeroDataRetention
        );
    }

    #[test]
    fn base64_matches_rfc_4648() {
        for (input, encoded) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
            (&[0xff, 0xfe, 0x00], "//4A"),
        ] {
            assert_eq!(base64(input), encoded, "{input:?}");
        }
    }

    #[test]
    fn the_body_carries_the_filter_and_only_set_fields() {
        let engine = OpenRouter::new(Config::openrouter(), Secret::new("k-1")).unwrap();
        let body = engine.request_body("QUJD");
        assert_eq!(
            body,
            serde_json::json!({
                "model": "openai/gpt-4o-mini-transcribe",
                "input_audio": {"data": "QUJD", "format": "wav"},
                "provider": {"data_collection": "deny"},
            })
        );
        let config = Config {
            prompt: Some("Vocet".into()),
            privacy: PrivacyFilter::ZeroDataRetention,
            ..Config::openrouter()
        };
        let body = OpenRouter::new(config, Secret::new("k-1"))
            .unwrap()
            .request_body("QUJD");
        assert!(body.get("language").is_none());
        assert_eq!(body["prompt"], "Vocet");
        assert_eq!(
            body["provider"],
            serde_json::json!({"zdr": true, "data_collection": "deny"})
        );
    }

    #[test]
    fn every_request_carries_the_key_and_the_attribution() {
        let headers = headers(&Secret::new("k-1234567890")).unwrap();
        assert_eq!(headers["authorization"], "Bearer k-1234567890");
        assert!(headers["authorization"].is_sensitive());
        assert_eq!(headers["http-referer"], "https://vocetlabs.com");
        assert_eq!(headers["x-title"], "Vocet");
        assert_eq!(headers["content-type"], "application/json");
    }

    #[test]
    fn parses_the_text_trimmed() {
        assert_eq!(
            parse(br#"{"text":" hi there ","segments":[]}"#).unwrap(),
            "hi there"
        );
    }

    #[test]
    fn malformed_replies_are_engine_errors_without_the_body() {
        for body in [
            &b"not json"[..],
            br#"{"segments":[]}"#,
            br#"{"text":"secret words""#,
        ] {
            let error = parse(body).unwrap_err();
            assert!(matches!(error, TranscribeError::Engine(_)));
            assert!(!error.to_string().contains("secret words"));
        }
    }

    #[test]
    fn statuses_map_to_errors() {
        let none = HeaderMap::new();
        let f = Failure::from_status(StatusCode::UNAUTHORIZED, &none);
        assert_eq!((f.error, f.retryable), (TranscribeError::InvalidKey, false));
        let f = Failure::from_status(StatusCode::FORBIDDEN, &none);
        assert_eq!(f.error, TranscribeError::InvalidKey);
        let f = Failure::from_status(StatusCode::PAYMENT_REQUIRED, &none);
        assert_eq!(
            (f.error, f.retryable),
            (TranscribeError::OutOfCredits, false)
        );
        let f = Failure::from_status(StatusCode::NOT_FOUND, &none);
        assert_eq!((f.error, f.retryable), (TranscribeError::NoRoute, false));
        let f = Failure::from_status(StatusCode::BAD_REQUEST, &none);
        assert!(!f.retryable);
        assert_eq!(
            f.error.to_string(),
            "transcription engine error: the provider rejected the request (HTTP 400)"
        );
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static(" 7 "));
        let f = Failure::from_status(StatusCode::TOO_MANY_REQUESTS, &headers);
        assert_eq!(f.error, TranscribeError::RateLimited);
        assert_eq!(
            (f.retryable, f.retry_after),
            (true, Some(Duration::from_secs(7)))
        );
        let f = Failure::from_status(StatusCode::BAD_GATEWAY, &none);
        assert!(f.retryable && f.retry_after.is_none());
        // Anything but delay seconds (an HTTP date too) means the backoff.
        for value in ["soon", "Wed, 21 Oct 2015 07:28:00 GMT", "-5"] {
            headers.insert(RETRY_AFTER, HeaderValue::from_static(value));
            assert_eq!(
                Failure::from_status(StatusCode::SERVICE_UNAVAILABLE, &headers).retry_after,
                None,
                "{value}"
            );
        }
        let f = Failure::from_status(StatusCode::FOUND, &none);
        assert!(!f.retryable);
        assert!(
            f.error.to_string().contains("redirect (HTTP 302)"),
            "{}",
            f.error
        );
    }

    #[test]
    fn debug_redacts_the_key() {
        let engine = OpenRouter::new(Config::openrouter(), Secret::new("k-1234567890")).unwrap();
        let shown = format!("{engine:?}");
        assert!(shown.contains("redacted"), "{shown}");
        assert!(!shown.contains("k-1234567890"));
    }

    #[test]
    fn a_key_that_cant_be_a_header_is_invalid_and_not_echoed() {
        let engine = OpenRouter::new(Config::openrouter(), Secret::new("bad\nkey")).unwrap();
        let failure = engine.auth_header().unwrap_err();
        assert_eq!(failure.error, TranscribeError::InvalidKey);
        assert!(!failure.retryable);
    }
}
