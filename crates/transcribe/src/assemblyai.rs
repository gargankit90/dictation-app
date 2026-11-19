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

use std::io::Write as _;
use std::path::PathBuf;
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
    /// Eval only: where to append each reply (see [`EVAL_DUMP_ENV`]).
    dump_dir: Option<PathBuf>,
}

/// Eval only, off by default: when this environment variable names a
/// directory when the engine is built, every request appends one JSON line
/// to `<dir>/assemblyai-replies.jsonl` (see [`DUMP_FILE`]): the full reply,
/// both texts, which one was pasted and why. It holds transcripts, so it is
/// for a test VM's eval runs; the normal log is unchanged. Never a key or
/// a header. (A GUI launch doesn't inherit a shell's environment: start the
/// app from the shell that sets it.)
pub const EVAL_DUMP_ENV: &str = "VOCET_EVAL_DUMP_DIR";

/// The file the dump appends to, inside the directory.
pub const DUMP_FILE: &str = "assemblyai-replies.jsonl";

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
            dump_dir: std::env::var_os(EVAL_DUMP_ENV)
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
        })
    }

    /// Sets (or clears) the eval dump directory, instead of the environment's.
    #[must_use]
    pub fn with_dump_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.dump_dir = dir;
        self
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
        let sent = Sent {
            rewrite_mode: with_mode.then_some("smart"),
            instruction: match self.mode() {
                RewriteMode::Smart => "smart",
                RewriteMode::Fast => "fast",
            },
            status: status.as_u16(),
        };
        if !status.is_success() {
            // Only what was sent and the status; the body isn't read.
            self.dump(&sent, None);
        }
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
        let parsed = parse(&body, self.mode().instruction()).map_err(Failure::fatal)?;
        self.dump(&sent, Some(&parsed));
        Ok(parsed.text)
    }

    /// Appends one line to the eval dump, if it is on. A failure to write
    /// never fails a dictation.
    fn dump(&self, sent: &Sent, parsed: Option<&Parsed>) {
        let Some(dir) = &self.dump_dir else { return };
        let line = dump_line(sent, parsed, std::time::SystemTime::now());
        let written = std::fs::create_dir_all(dir).and_then(|()| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(DUMP_FILE))
                .and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()))
        });
        if let Err(error) = written {
            tracing::debug!(kind = ?error.kind(), "could not write the eval dump");
        }
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
    #[serde(default)]
    session_id: Option<serde_json::Value>,
    #[serde(default)]
    audio_duration_ms: Option<serde_json::Value>,
    #[serde(default)]
    request_time_ms: Option<serde_json::Value>,
    #[serde(default)]
    sync_time_ms: Option<serde_json::Value>,
}

/// What a request sent, for the eval dump (no key, no header).
struct Sent {
    rewrite_mode: Option<&'static str>,
    instruction: &'static str,
    status: u16,
}

/// A parsed reply: the text to paste and everything the eval dump records.
struct Parsed {
    text: String,
    reason: &'static str,
    reply: Reply,
}

/// The dump's JSON line for one request.
fn dump_line(sent: &Sent, parsed: Option<&Parsed>, now: std::time::SystemTime) -> String {
    let millis = now
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let mut line = serde_json::json!({
        "ts": iso8601(millis),
        "rewrite_mode_sent": sent.rewrite_mode,
        "instruction": sent.instruction,
        "http_status": sent.status,
    });
    if let Some(parsed) = parsed {
        let reply = &parsed.reply;
        line["verbatim_text"] = serde_json::json!(reply.text);
        line["llm_response"] = serde_json::json!(reply.llm_response);
        line["llm_error"] = serde_json::json!(reply.llm_error);
        line["pasted"] = serde_json::json!(if parsed.reason == "rewrite" {
            "rewrite"
        } else {
            "verbatim"
        });
        line["pasted_reason"] = serde_json::json!(parsed.reason);
        line["pasted_text"] = serde_json::json!(parsed.text);
        line["session_id"] = serde_json::json!(reply.session_id);
        line["audio_duration_ms"] = serde_json::json!(reply.audio_duration_ms);
        line["request_time_ms"] = serde_json::json!(reply.request_time_ms);
        line["sync_time_ms"] = serde_json::json!(reply.sync_time_ms);
    }
    line.to_string()
}

/// `millis` since the Unix epoch as `2026-11-19T07:08:09.123Z`.
fn iso8601(millis: u128) -> String {
    let secs = (millis / 1000) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60,
        millis % 1000
    )
}

/// The text to paste from a reply: the rewrite if it is a rewrite of the
/// speech, else the verbatim text. Silence (empty verbatim text) pastes
/// nothing whatever the rewrite says. Logs which one and why, and the
/// rewrite's error code (a fixed word such as `timeout`), never the text. The
/// error never quotes the body.
fn parse(body: &[u8], instruction: &str) -> Result<Parsed, TranscribeError> {
    let reply: Reply = serde_json::from_slice(body).map_err(|e| {
        TranscribeError::Engine(format!("unreadable reply ({:?} error)", e.classify()))
    })?;
    let verbatim = reply.text.clone().unwrap_or_default().trim().to_owned();
    let rewrite = reply
        .llm_response
        .clone()
        .map(|text| text.trim().to_owned());
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
    Ok(Parsed {
        text,
        reason,
        reply,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_timestamp_is_utc_iso_8601() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
        // 2026-11-19 07:08:09.123 UTC.
        assert_eq!(iso8601(1_795_072_089_123), "2026-11-19T07:08:09.123Z");
        // A leap day.
        assert_eq!(iso8601(1_709_164_800_000), "2024-02-29T00:00:00.000Z");
    }
}
