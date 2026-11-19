//! [`AssemblyAi`]: speech to text through `AssemblyAI`'s Dictation API
//! (experimental; branch `exp/assemblyai`; "Vocet Cloud" in the UI).
//!
//! One HTTP request, from <https://www.assemblyai.com/docs/dictation>:
//! `POST {base}/v1/transcribe`, `multipart/form-data` with
//!
//! - `audio`: the clip as a 16-bit mono WAV (`audio/wav`);
//! - `config` (JSON): `llm_instruction` (Blurt's cleanup text for the mode,
//!   see [`crate::assemblyai_prompt`]), `rewrite_mode: "smart"` unless the
//!   settings chose Fast (the key is then left out), and `language_codes`
//!   with the saved language, if the API knows it (an unknown code is a
//!   400, so any other language is left to the service). If the service
//!   answers 400 with `rewrite_mode`, the request is sent once more
//!   without it, and later ones leave it out.
//!
//! The reply holds the verbatim transcript in `text` and the cleaned-up
//! rewrite in `llm_response`, which is `null` (with `llm_error` saying why)
//! when the rewrite failed although the request succeeded. The rewrite is
//! the dictation; the verbatim text is the fallback, also when the rewrite
//! is blank or only an echo of the instruction.
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

use std::sync::atomic::{AtomicBool, Ordering};
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

use crate::assemblyai_prompt::{RewriteMode, is_speech};
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
    /// Set once the service answered 400 to `rewrite_mode` and then accepted
    /// the same request without it: later dictations leave it out at once.
    smart_refused: Arc<AtomicBool>,
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
            smart_refused: Arc::new(AtomicBool::new(false)),
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
        let mode = self.mode();
        let sends_mode = mode == RewriteMode::Smart && !self.smart_refused.load(Ordering::Relaxed);
        match self.post(wav, timeout, sends_mode).await {
            // Nobody has verified that the buffered endpoint takes
            // `rewrite_mode` (the config is validated strictly, so an
            // unknown key is a 400): try once more without it, and
            // remember that it worked.
            Err(Rejected::BadRequest) if sends_mode => {
                tracing::info!("assemblyai answered 400 to rewrite_mode; retrying without it");
                let text = self
                    .post(wav, timeout, false)
                    .await
                    .map_err(Rejected::failure)?;
                self.smart_refused.store(true, Ordering::Relaxed);
                tracing::info!("assemblyai takes no rewrite_mode here; dictating without it");
                Ok(text)
            }
            other => other.map_err(Rejected::failure),
        }
    }

    fn mode(&self) -> RewriteMode {
        RewriteMode::from_model(&self.config.model)
    }

    /// One request; `with_mode` sends `rewrite_mode: "smart"`.
    async fn post(
        &self,
        wav: &[u8],
        timeout: Duration,
        with_mode: bool,
    ) -> Result<String, Rejected> {
        let audio = Part::bytes(wav.to_vec())
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|_| Failure::fatal(TranscribeError::Engine("bad MIME type".into())))?;
        let config = Part::text(self.config_part(with_mode))
            .mime_str("application/json")
            .map_err(|_| Failure::fatal(TranscribeError::Engine("bad MIME type".into())))?;
        let form = Form::new().part("audio", audio).part("config", config);
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
            ))
            .into());
        }
        if status == StatusCode::BAD_REQUEST {
            return Err(Rejected::BadRequest);
        }
        if !status.is_success() {
            // The body isn't read: it can echo request details.
            return Err(Failure::from_status(status, response.headers()).into());
        }
        let body = response.bytes().await.map_err(Failure::from_reqwest)?;
        parse(&body, self.mode().instruction()).map_err(|e| Failure::fatal(e).into())
    }

    /// The JSON of the `config` part: the cleanup instruction for the mode,
    /// `rewrite_mode` for Smart (`with_mode`; Fast is the service's default,
    /// so the key is left out), and `language_codes` if the saved language
    /// is one the API knows.
    fn config_part(&self, with_mode: bool) -> String {
        let mut config = serde_json::json!({ "llm_instruction": self.mode().instruction() });
        if with_mode {
            config["rewrite_mode"] = serde_json::json!("smart");
        }
        if let Some(language) = self.config.language.as_deref()
            && LANGUAGES.contains(&language)
        {
            config["language_codes"] = serde_json::json!([language]);
        }
        config.to_string()
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

/// Why a request failed: a 400 is told apart so `rewrite_mode` can be
/// dropped; anything else is a [`Failure`].
enum Rejected {
    BadRequest,
    Failed(Failure),
}

impl From<Failure> for Rejected {
    fn from(failure: Failure) -> Self {
        Self::Failed(failure)
    }
}

impl Rejected {
    fn failure(self) -> Failure {
        match self {
            Self::BadRequest => Failure::fatal(TranscribeError::Engine(
                "the provider rejected the request (HTTP 400)".into(),
            )),
            Self::Failed(failure) => failure,
        }
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

/// The text to paste from a reply: the rewrite if it is a rewrite of the
/// speech, else the verbatim text. Silence (empty verbatim text) pastes
/// nothing whatever the rewrite says. Logs which one and why, and the
/// rewrite's error code (a fixed word such as `timeout`), never the text. The
/// error never quotes the body.
fn parse(body: &[u8], instruction: &str) -> Result<String, TranscribeError> {
    let reply: Reply = serde_json::from_slice(body).map_err(|e| {
        TranscribeError::Engine(format!("unreadable reply ({:?} error)", e.classify()))
    })?;
    let verbatim = reply.text.unwrap_or_default().trim().to_owned();
    let rewrite = reply.llm_response.map(|text| text.trim().to_owned());
    let (text, reason) = if verbatim.is_empty() {
        (verbatim, "silence")
    } else {
        match rewrite {
            None => (verbatim, "no_rewrite"),
            Some(rewrite) if rewrite.is_empty() => (verbatim, "blank_rewrite"),
            Some(rewrite) if !is_speech(&rewrite, instruction) => {
                (verbatim, "echo_or_empty_rewrite")
            }
            Some(rewrite) => (rewrite, "rewrite"),
        }
    };
    tracing::info!(
        source = if reason == "rewrite" {
            "rewrite"
        } else {
            "verbatim"
        },
        reason,
        rewrite_error = reply.llm_error.as_deref().map_or("none", short_code),
        chars = text.chars().count(),
        "assemblyai text"
    );
    Ok(text)
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
