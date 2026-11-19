//! [`AssemblyAi`]: speech to text through `AssemblyAI`'s Dictation API
//! (experimental; branch `exp/assemblyai`; "Vocet Cloud" in the UI).
//!
//! One HTTP request, from <https://www.assemblyai.com/docs/dictation>:
//! `POST {base}/v1/transcribe`, `multipart/form-data` with
//!
//! - `audio`: the clip as a 16-bit mono WAV (`audio/wav`);
//! - `config` (JSON, optional): `language_codes` with the saved language,
//!   if the API knows it (an unknown code is a 400, so any other language
//!   is left to the service). Nothing else: no `llm_instruction`, so the
//!   service's default cleanup runs (filler removed, punctuation and
//!   capitalisation, self-corrections resolved), and `rewrite_mode` stays
//!   `fast`.
//!
//! The reply holds the verbatim transcript in `text` and the cleaned-up
//! rewrite in `llm_response`, which is `null` (with `llm_error` saying why)
//! when the rewrite failed although the request succeeded. The rewrite is
//! the dictation; the verbatim text is the fallback.
//!
//! The batch endpoint takes up to 20 minutes and 256 MB, so a clip of
//! Vocet's 5-minute maximum (about 9.6 MB of WAV) goes in one request:
//! nothing is split and no audio is dropped. (The live endpoint stops at
//! 120 s; it is for uploading while the user still speaks, which is out of
//! scope here.) A clip the API still refuses as too long (413) is its own
//! clear error.
//!
//! The key goes in the `Authorization` header with no `Bearer` prefix, as
//! `AssemblyAI` documents (marked sensitive). Timeouts, retries and the long
//! clip rule (H1) are those of [`OpenAiCompat`](crate::openai_compat::OpenAiCompat).
//!
//! Privacy: errors and logs never contain the key, the audio or the text;
//! logs hold statuses, sizes, lengths and which text was used.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use dictation_core::retry::SplitMix64;
use dictation_core::text::Transcript;
use dictation_core::upload_limits::{may_retry_after_timeout, request_timeout};
use dictation_core::wav;
use dictation_ports::{Audio, Secret, TranscribeError, Transcriber};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use reqwest::multipart::{Form, Part};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::openai_compat::{Config, Failure, MAX_RETRY_AFTER, TokioWait, Wait, audio_duration};

/// The languages `config.language_codes` accepts (the API answers 400 to
/// any other).
pub const LANGUAGES: [&str; 32] = [
    "en", "es", "de", "fr", "it", "pt", "tr", "nl", "sv", "no", "da", "fi", "hi", "vi", "ar", "he",
    "ja", "ur", "zh", "ko", "ca", "gl", "ru", "ro", "et", "fa", "yue", "af", "mr", "zu", "xh",
    "nn",
];

/// An `AssemblyAI` Dictation API engine.
pub struct AssemblyAi<W = TokioWait> {
    client: Client,
    endpoint: String,
    config: Config,
    key: Secret,
    wait: W,
    rng: Arc<Mutex<SplitMix64>>,
}

impl AssemblyAi<TokioWait> {
    /// An engine for `config` (its `base_url` is the API root, e.g.
    /// `https://dictation.assemblyai.com`) with the user's `key`.
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

impl<W: Wait> AssemblyAi<W> {
    /// As [`new`](AssemblyAi::new), waiting between retries with `wait` and
    /// drawing jitter from a generator seeded with `seed`.
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
        Ok(Self {
            client: crate::http::client(&config.base_url)?,
            endpoint: format!("{}/v1/transcribe", config.base_url.trim_end_matches('/')),
            config,
            key,
            wait,
            rng: Arc::new(Mutex::new(SplitMix64::new(seed))),
        })
    }

    /// Transcribes `audio`: the cleaned-up text, or the verbatim text if the
    /// rewrite is missing.
    ///
    /// # Errors
    ///
    /// As [`Transcriber::transcribe`], after retries.
    pub async fn transcribe_text(&self, audio: &Audio) -> Result<String, TranscribeError> {
        let wav = wav::encode_f32(&audio.samples, audio.sample_rate)
            .map_err(|e| TranscribeError::Engine(format!("could not encode the audio: {e}")))?;
        let clip = audio_duration(audio);
        let timeout = if self.config.timeout_grows_with_audio {
            request_timeout(self.config.timeout, clip)
        } else {
            self.config.timeout
        };
        tracing::debug!(bytes = wav.len(), ?timeout, "transcribing (assemblyai)");
        let mut failures = 0u32;
        let mut timeouts = 0u32;
        loop {
            let error = match self.attempt(&wav, timeout).await {
                Ok(text) => return Ok(text),
                Err(failure) => failure,
            };
            failures = failures.saturating_add(1);
            if error.error == TranscribeError::Timeout {
                timeouts = timeouts.saturating_add(1);
                if !may_retry_after_timeout(clip, self.config.long_clip, timeouts) {
                    return Err(error.error);
                }
            }
            let Some(delay) = self.retry_delay(&error, failures) else {
                tracing::debug!(failures, error = %error.error, "giving up (assemblyai)");
                return Err(error.error);
            };
            tracing::debug!(failures, error = %error.error, ?delay, "retrying (assemblyai)");
            self.wait.wait(delay).await;
        }
    }

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

    async fn attempt(&self, wav: &[u8], timeout: Duration) -> Result<String, Failure> {
        let audio = Part::bytes(wav.to_vec())
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|_| Failure::fatal(TranscribeError::Engine("bad MIME type".into())))?;
        let mut form = Form::new().part("audio", audio);
        if let Some(config) = self.config_part() {
            let config = Part::text(config)
                .mime_str("application/json")
                .map_err(|_| Failure::fatal(TranscribeError::Engine("bad MIME type".into())))?;
            form = form.part("config", config);
        }
        let response = self
            .client
            .post(&self.endpoint)
            .headers(self.auth_header()?)
            .timeout(timeout)
            .multipart(form)
            .send()
            .await
            .map_err(Failure::from_reqwest)?;
        let status = response.status();
        if status == StatusCode::PAYLOAD_TOO_LARGE {
            return Err(Failure::fatal(TranscribeError::Engine(
                "the recording is longer than the provider accepts (HTTP 413)".into(),
            )));
        }
        if !status.is_success() {
            // The body isn't read: it can echo request details.
            return Err(Failure::from_status(status, response.headers()));
        }
        let body = response.bytes().await.map_err(Failure::from_reqwest)?;
        parse(&body).map_err(Failure::fatal)
    }

    /// The JSON of the `config` part, or `None` to send none (defaults).
    fn config_part(&self) -> Option<String> {
        let language = self.config.language.as_deref()?;
        LANGUAGES
            .contains(&language)
            .then(|| serde_json::json!({ "language_codes": [language] }).to_string())
    }

    fn auth_header(&self) -> Result<HeaderMap, Failure> {
        // No "Bearer": AssemblyAI takes the bare key.
        let mut value = HeaderValue::from_str(self.key.expose())
            // The message must not echo the key.
            .map_err(|_| Failure::fatal(TranscribeError::InvalidKey))?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, value);
        Ok(headers)
    }
}

impl<W: Wait> Transcriber for AssemblyAi<W> {
    async fn transcribe(&self, audio: &Audio) -> Result<String, TranscribeError> {
        self.transcribe_text(audio).await
    }

    async fn transcribe_detailed(&self, audio: &Audio) -> Result<Transcript, TranscribeError> {
        // No per-segment no-speech probability: the silence filter has
        // nothing to judge by, as for any engine that reports none.
        self.transcribe_text(audio).await.map(Transcript::from_text)
    }
}

impl<W> std::fmt::Debug for AssemblyAi<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssemblyAi")
            .field("endpoint", &self.endpoint)
            .field("config", &self.config)
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    llm_response: Option<String>,
    #[serde(default)]
    llm_error: Option<String>,
}

/// The text to paste from a reply: the rewrite if there is one, else the
/// verbatim text. Logs which, and the rewrite's error code (a fixed word
/// such as `timeout`), never the text. The error never quotes the body.
fn parse(body: &[u8]) -> Result<String, TranscribeError> {
    let reply: Reply = serde_json::from_slice(body).map_err(|e| {
        TranscribeError::Engine(format!("unreadable reply ({:?} error)", e.classify()))
    })?;
    let verbatim = reply.text.unwrap_or_default().trim().to_owned();
    let rewrite = reply
        .llm_response
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    // An empty verbatim text is silence: nothing to paste, whatever the
    // rewrite says.
    match rewrite {
        Some(text) if !verbatim.is_empty() => {
            tracing::info!(
                source = "rewrite",
                chars = text.chars().count(),
                "assemblyai text"
            );
            Ok(text)
        }
        _ => {
            tracing::info!(
                source = "verbatim",
                rewrite_error = reply.llm_error.as_deref().map_or("none", short_code),
                chars = verbatim.chars().count(),
                "assemblyai text"
            );
            Ok(verbatim)
        }
    }
}

/// `code` if it looks like one of the API's error words (`timeout`,
/// `upstream_error`, ...), else a placeholder: it reaches the log.
fn short_code(code: &str) -> &str {
    if code.len() <= 32 && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        code
    } else {
        "other"
    }
}
