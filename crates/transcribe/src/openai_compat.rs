//! [`OpenAiCompat`]: speech to text through an OpenAI-compatible
//! `POST {base_url}/audio/transcriptions` endpoint (Groq, OpenAI).
//!
//! The audio goes up as a 16-bit mono WAV (`dictation_core::wav`) in a
//! multipart form with the model, the language (if set) and
//! `response_format=verbose_json`, so the reply carries per-segment
//! `no_speech_prob` for the hallucination filter.
//!
//! Rate limits (429), server errors (5xx), timeouts and network errors are
//! retried under a [`RetryPolicy`]; a `Retry-After` header (delay seconds or
//! an HTTP date) on a 429 or 5xx replaces the backoff, up to
//! [`MAX_RETRY_AFTER`]. A rejected
//! key (401, 403), any other 4xx and an unreadable reply are not retried.
//!
//! Privacy: the key is a [`Secret`] and goes only into the `Authorization`
//! header (marked sensitive). Errors and logs never contain the key, the
//! audio or the transcript; logs are at debug and hold statuses, sizes and
//! lengths only.

use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use dictation_core::retry::{RetryPolicy, SplitMix64};
use dictation_core::text::{Transcript, mean_no_speech_prob};
use dictation_core::wav;
use dictation_ports::{Audio, Secret, TranscribeError, Transcriber};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, RETRY_AFTER};
use reqwest::multipart::{Form, Part};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

/// The longest `Retry-After` honoured; a provider asking for longer gets
/// [`TranscribeError::RateLimited`] instead of a long silent wait.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// How long one request may take before it counts as a timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// A provider's endpoint and default model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// For logs and settings.
    pub name: &'static str,
    /// The API root; `/audio/transcriptions` is appended.
    pub base_url: &'static str,
    /// The model sent with each request.
    pub model: &'static str,
}

/// Groq: `whisper-large-v3-turbo`. Direct uploads up to 25 MB on the free
/// tier (see `docs/ENGINEERING.md`).
pub const GROQ: Preset = Preset {
    name: "groq",
    base_url: "https://api.groq.com/openai/v1",
    model: "whisper-large-v3-turbo",
};

/// OpenAI: `whisper-1`, its model that supports `verbose_json`.
pub const OPENAI: Preset = Preset {
    name: "openai",
    base_url: "https://api.openai.com/v1",
    model: "whisper-1",
};

/// A transcript with the per-segment detail `verbose_json` returns.
#[derive(Debug, Clone, PartialEq)]
pub struct Transcription {
    /// The whole text, trimmed.
    pub text: String,
    /// The segments, in order; empty if the provider sent none.
    pub segments: Vec<Segment>,
}

impl Transcription {
    /// The text with the mean `no_speech_prob` of the segments, for the
    /// pipeline's silence filter; `None` if the provider sent no segments.
    #[must_use]
    pub fn into_transcript(self) -> Transcript {
        Transcript {
            no_speech_prob: mean_no_speech_prob(self.segments.iter().map(|s| s.no_speech_prob)),
            text: self.text,
        }
    }
}

/// One segment of a [`Transcription`].
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Start, in seconds from the beginning of the audio.
    pub start: f64,
    /// End, in seconds.
    pub end: f64,
    /// The segment's text.
    pub text: String,
    /// The model's probability that the segment is silence or noise
    /// (closer to 1 means probably no speech); 0 if missing.
    pub no_speech_prob: f64,
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

/// Settings for an [`OpenAiCompat`] engine.
#[derive(Debug, Clone)]
pub struct Config {
    /// The API root, e.g. [`GROQ`]`.base_url`.
    pub base_url: String,
    /// The model name.
    pub model: String,
    /// An ISO-639-1 language code (`"en"`), or `None` to let the model
    /// detect it.
    pub language: Option<String>,
    /// Per-request timeout.
    pub timeout: Duration,
    /// Retries for rate limits, server errors, timeouts and network errors.
    pub retry: RetryPolicy,
}

impl Config {
    /// `preset`'s endpoint and model, no language, the default timeout and
    /// retry policy.
    #[must_use]
    pub fn from_preset(preset: Preset) -> Self {
        Self {
            base_url: preset.base_url.to_owned(),
            model: preset.model.to_owned(),
            language: None,
            timeout: DEFAULT_TIMEOUT,
            retry: RetryPolicy::default(),
        }
    }
}

/// An OpenAI-compatible transcription engine.
pub struct OpenAiCompat<W = TokioWait> {
    client: Client,
    endpoint: String,
    config: Config,
    key: Secret,
    wait: W,
    rng: Arc<Mutex<SplitMix64>>,
}

impl OpenAiCompat<TokioWait> {
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

impl<W: Wait> OpenAiCompat<W> {
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
        let client = crate::http::client(config.timeout)?;
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

    /// Transcribes `audio`, keeping the segments.
    ///
    /// # Errors
    ///
    /// As [`Transcriber::transcribe`], after retries.
    pub async fn transcribe_verbose(
        &self,
        audio: &Audio,
    ) -> Result<Transcription, TranscribeError> {
        let wav = wav::encode_f32(&audio.samples, audio.sample_rate)
            .map_err(|e| TranscribeError::Engine(format!("could not encode the audio: {e}")))?;
        tracing::debug!(bytes = wav.len(), model = %self.config.model, "transcribing");
        let mut failures = 0u32;
        loop {
            let error = match self.attempt(&wav).await {
                Ok(transcription) => {
                    tracing::debug!(
                        chars = transcription.text.chars().count(),
                        segments = transcription.segments.len(),
                        "transcribed"
                    );
                    return Ok(transcription);
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

    async fn attempt(&self, wav: &[u8]) -> Result<Transcription, Failure> {
        let file = Part::bytes(wav.to_vec())
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|_| Failure::fatal(TranscribeError::Engine("bad MIME type".into())))?;
        let mut form = Form::new()
            .part("file", file)
            .text("model", self.config.model.clone())
            .text("response_format", "verbose_json");
        if let Some(language) = &self.config.language {
            form = form.text("language", language.clone());
        }
        let response = self
            .client
            .post(&self.endpoint)
            .headers(self.auth_header()?)
            .multipart(form)
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

    fn auth_header(&self) -> Result<HeaderMap, Failure> {
        let mut value = HeaderValue::from_str(&format!("Bearer {}", self.key.expose()))
            // The message must not echo the key.
            .map_err(|_| Failure::fatal(TranscribeError::InvalidKey))?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, value);
        Ok(headers)
    }
}

impl<W: Wait> Transcriber for OpenAiCompat<W> {
    async fn transcribe(&self, audio: &Audio) -> Result<String, TranscribeError> {
        self.transcribe_verbose(audio).await.map(|t| t.text)
    }

    async fn transcribe_detailed(&self, audio: &Audio) -> Result<Transcript, TranscribeError> {
        self.transcribe_verbose(audio)
            .await
            .map(Transcription::into_transcript)
    }
}

impl<W> std::fmt::Debug for OpenAiCompat<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiCompat")
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
        // The body isn't used: a 401 from OpenAI echoes part of the key.
        let retry_after = headers
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| parse_retry_after(v, SystemTime::now()));
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Self::fatal(TranscribeError::InvalidKey)
            }
            StatusCode::TOO_MANY_REQUESTS => Self::retry(TranscribeError::RateLimited, retry_after),
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

/// A `Retry-After` value (RFC 9110): delay seconds, or an HTTP date turned
/// into the wait from `now` (zero if it's already past). `None` if it's
/// neither, so the backoff applies. The [`MAX_RETRY_AFTER`] cap is applied
/// by the caller.
fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let at = httpdate::parse_http_date(value).ok()?;
    Some(at.duration_since(now).unwrap_or(Duration::ZERO))
}

#[derive(Deserialize)]
struct VerboseJson {
    text: String,
    #[serde(default)]
    segments: Vec<VerboseSegment>,
}

#[derive(Deserialize)]
struct VerboseSegment {
    #[serde(default)]
    start: f64,
    #[serde(default)]
    end: f64,
    #[serde(default)]
    text: String,
    #[serde(default)]
    no_speech_prob: f64,
}

/// Parses a `verbose_json` reply. The error never quotes the body, which
/// could hold the transcript.
fn parse(body: &[u8]) -> Result<Transcription, TranscribeError> {
    let reply: VerboseJson = serde_json::from_slice(body).map_err(|e| {
        TranscribeError::Engine(format!("unreadable reply ({:?} error)", e.classify()))
    })?;
    Ok(Transcription {
        text: reply.text.trim().to_owned(),
        segments: reply
            .segments
            .into_iter()
            .map(|s| Segment {
                start: s.start,
                end: s.end,
                text: s.text,
                no_speech_prob: s.no_speech_prob,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_point_at_the_providers() {
        assert_eq!(GROQ.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(GROQ.model, "whisper-large-v3-turbo");
        assert_eq!(OPENAI.model, "whisper-1");
        let config = Config::from_preset(GROQ);
        assert_eq!(config.base_url, GROQ.base_url);
        assert_eq!(config.timeout, DEFAULT_TIMEOUT);
        assert_eq!(config.language, None);
    }

    #[test]
    fn the_transcript_carries_the_mean_no_speech_prob() {
        let t = parse(
            br#"{"text":"Thank you.","segments":[{"no_speech_prob":0.8},{"no_speech_prob":0.6}]}"#,
        )
        .unwrap()
        .into_transcript();
        assert_eq!(t.text, "Thank you.");
        assert!((t.no_speech_prob.unwrap() - 0.7).abs() < 1e-12);

        // No segments: nothing to judge by, so none.
        let t = parse(br#"{"text":"hi"}"#).unwrap().into_transcript();
        assert_eq!(t.no_speech_prob, None);
    }

    #[test]
    fn parses_text_and_segments_with_defaults() {
        let t = parse(br#"{"text":" hi there ","segments":[{"start":0.0,"end":1.5,"text":" hi","no_speech_prob":0.01},{"text":"x"}]}"#).unwrap();
        assert_eq!(t.text, "hi there");
        assert_eq!(t.segments.len(), 2);
        assert!((t.segments[0].no_speech_prob - 0.01).abs() < 1e-12);
        assert!((t.segments[0].end - 1.5).abs() < 1e-12);
        assert_eq!(t.segments[1].no_speech_prob, 0.0);
        assert!(
            parse(br#"{"text":"no segments"}"#)
                .unwrap()
                .segments
                .is_empty()
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
        // A date long past means retry at once; garbage means the backoff.
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
        );
        assert_eq!(
            Failure::from_status(StatusCode::SERVICE_UNAVAILABLE, &headers).retry_after,
            Some(Duration::ZERO)
        );
        headers.insert(RETRY_AFTER, HeaderValue::from_static("soon"));
        assert_eq!(
            Failure::from_status(StatusCode::SERVICE_UNAVAILABLE, &headers).retry_after,
            None
        );
    }

    #[test]
    fn retry_after_parses_seconds_and_http_dates() {
        let now = httpdate::parse_http_date("Sat, 31 Oct 2026 12:00:00 GMT").unwrap();
        let parse = |v| parse_retry_after(v, now);
        assert_eq!(parse("120"), Some(Duration::from_secs(120)));
        assert_eq!(parse(" 0 "), Some(Duration::ZERO));
        // All three RFC 9110 date forms.
        for date in [
            "Sat, 31 Oct 2026 12:00:05 GMT",
            "Saturday, 31-Oct-26 12:00:05 GMT",
            "Sat Oct 31 12:00:05 2026",
        ] {
            assert_eq!(parse(date), Some(Duration::from_secs(5)), "{date}");
        }
        // Past, or exactly now: no wait.
        assert_eq!(parse("Sat, 31 Oct 2026 11:59:00 GMT"), Some(Duration::ZERO));
        assert_eq!(parse("Sat, 31 Oct 2026 12:00:00 GMT"), Some(Duration::ZERO));
        // A far date parses; the 30 s cap is the caller's.
        assert_eq!(
            parse("Sat, 31 Oct 2026 13:00:00 GMT"),
            Some(Duration::from_secs(3_600))
        );
        for bad in ["", "-5", "1.5", "tomorrow", "Sat, 32 Oct 2026 12:00:00 GMT"] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn debug_redacts_the_key() {
        let engine =
            OpenAiCompat::new(Config::from_preset(GROQ), Secret::new("k-1234567890")).unwrap();
        let shown = format!("{engine:?}");
        assert!(shown.contains("redacted"), "{shown}");
        assert!(!shown.contains("k-1234567890"));
    }

    #[test]
    fn a_key_that_cant_be_a_header_is_invalid_and_not_echoed() {
        let engine = OpenAiCompat::new(Config::from_preset(GROQ), Secret::new("bad\nkey")).unwrap();
        let failure = engine.auth_header().unwrap_err();
        assert_eq!(failure.error, TranscribeError::InvalidKey);
        assert!(!failure.retryable);
    }
}
