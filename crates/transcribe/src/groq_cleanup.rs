//! Experimental (not for `main`): an LLM cleanup pass after a Groq Whisper
//! transcription.
//!
//! The transcript's **text** (never the audio) goes to Groq's
//! OpenAI-compatible `POST {base_url}/chat/completions` with the same key,
//! the cleanup system prompt ([`crate::groq_cleanup_prompt`]) and
//! temperature 0. The reply is pasted only if [`judge`] accepts it: it says
//! something, is not an echo of the prompt, is not much longer than the
//! transcript, and keeps every number, email/URL/file name and capitalised
//! name of the transcript. Otherwise, and on any error or a timeout
//! ([`DEFAULT_TIMEOUT`]), the raw Whisper text is pasted. Cleanup never
//! fails a dictation.
//!
//! Logs say which text was used, why, the model and the latency; never a
//! text, and never the key.
//!
//! Eval only, off by default: [`EVAL_DUMP_ENV`] (the same opt-in as PR
//! #322) names a directory, and each cleanup appends one JSON line to
//! [`DUMP_FILE`] there (raw text, reply, pasted text, reason, model,
//! prompt, latency). [`EVAL_PROMPT_ENV`] names a file whose text replaces
//! the built-in system prompt, so prompt versions can be compared without
//! a rebuild.

use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use dictation_ports::{Secret, TranscribeError};
use reqwest::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::groq_cleanup_prompt::{self, CLOSE_TAG, OPEN_TAG, PROMPT_NAME, SYSTEM_PROMPT};

/// The fast, small model offered in Settings.
pub const FAST_MODEL: &str = "llama-3.1-8b-instant";

/// The stronger model offered in Settings.
pub const STRONG_MODEL: &str = "llama-3.3-70b-versatile";

/// How long a cleanup may take before the raw text is pasted instead.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

/// Eval only: a directory to append [`DUMP_FILE`] to (holds transcripts).
pub const EVAL_DUMP_ENV: &str = "VOCET_EVAL_DUMP_DIR";

/// The dump file inside [`EVAL_DUMP_ENV`]'s directory.
pub const DUMP_FILE: &str = "groq-replies.jsonl";

/// Eval only: a file whose text replaces the built-in system prompt.
pub const EVAL_PROMPT_ENV: &str = "VOCET_EVAL_CLEANUP_PROMPT_FILE";

/// A reply longer than this many times the transcript (plus
/// [`LENGTH_SLACK`] characters) is not a cleanup.
const LENGTH_RATIO: f64 = 1.25;

/// Characters a reply may add beyond [`LENGTH_RATIO`] (punctuation added
/// to a short transcript).
const LENGTH_SLACK: usize = 20;

/// A prompt sentence shorter than this, normalised, is something a person
/// says and is not held against a reply.
const SENTENCE_FLOOR: usize = 12;

/// Which text was pasted, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The reply passed every check: it was pasted.
    Cleaned,
    /// The reply had no words.
    Empty,
    /// The reply was (part of) the prompt.
    Echo,
    /// The reply was much longer than the transcript.
    TooLong,
    /// A number of the transcript is missing from the reply.
    DroppedNumber,
    /// An email, URL or file name of the transcript is missing.
    DroppedLink,
    /// A capitalised name of the transcript is missing.
    DroppedName,
    /// The request took longer than the timeout.
    Timeout,
    /// The provider answered with an error status.
    HttpError(u16),
    /// The request failed before an answer (network).
    Network,
    /// The answer could not be read.
    BadReply,
}

impl Reason {
    /// Whether the cleaned reply was pasted.
    #[must_use]
    pub fn cleaned(self) -> bool {
        self == Self::Cleaned
    }

    /// A short name for logs and the dump.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Cleaned => "cleaned".to_owned(),
            Self::Empty => "empty".to_owned(),
            Self::Echo => "echo".to_owned(),
            Self::TooLong => "too_long".to_owned(),
            Self::DroppedNumber => "dropped_number".to_owned(),
            Self::DroppedLink => "dropped_link".to_owned(),
            Self::DroppedName => "dropped_name".to_owned(),
            Self::Timeout => "timeout".to_owned(),
            Self::HttpError(status) => format!("http_{status}"),
            Self::Network => "network".to_owned(),
            Self::BadReply => "bad_reply".to_owned(),
        }
    }
}

/// What a cleanup did.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The text to paste: the reply, or the raw transcript.
    pub text: String,
    /// Why that text.
    pub reason: Reason,
    /// The model's reply, if one came back.
    pub reply: Option<String>,
    /// How long the cleanup took.
    pub latency: Duration,
}

/// The cleanup pass: a Groq chat model and the key.
pub struct Cleanup {
    client: Client,
    endpoint: String,
    model: String,
    key: Secret,
    timeout: Duration,
    prompt: String,
    prompt_name: String,
    dump_dir: Option<PathBuf>,
}

impl Cleanup {
    /// A cleanup with `model` at `base_url` (Groq's API root), the key, the
    /// default timeout, and the built-in prompt unless [`EVAL_PROMPT_ENV`]
    /// names a readable file. [`EVAL_DUMP_ENV`] turns the dump on.
    ///
    /// # Errors
    ///
    /// [`TranscribeError::Engine`] if the HTTP client can't be built.
    pub fn new(base_url: &str, model: &str, key: Secret) -> Result<Self, TranscribeError> {
        let mut cleanup = Self {
            client: crate::http::client(base_url)?,
            endpoint: format!("{}/chat/completions", base_url.trim_end_matches('/')),
            model: model.to_owned(),
            key,
            timeout: DEFAULT_TIMEOUT,
            prompt: SYSTEM_PROMPT.to_owned(),
            prompt_name: PROMPT_NAME.to_owned(),
            dump_dir: std::env::var_os(EVAL_DUMP_ENV)
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
        };
        if let Some(path) = std::env::var_os(EVAL_PROMPT_ENV).filter(|p| !p.is_empty()) {
            let path = PathBuf::from(path);
            match std::fs::read_to_string(&path) {
                Ok(prompt) if !prompt.trim().is_empty() => {
                    let name = path
                        .file_stem()
                        .map_or_else(|| "eval".to_owned(), |s| s.to_string_lossy().into_owned());
                    cleanup = cleanup.with_prompt(&name, prompt.trim());
                }
                Ok(_) => tracing::warn!("the eval cleanup prompt file is empty; built-in prompt"),
                Err(error) => {
                    tracing::warn!(kind = ?error.kind(), "could not read the eval cleanup prompt; built-in prompt");
                }
            }
        }
        tracing::info!(model, prompt = %cleanup.prompt_name, dump = cleanup.dump_dir.is_some(), "groq cleanup on");
        Ok(cleanup)
    }

    /// Uses `timeout` instead of [`DEFAULT_TIMEOUT`].
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Uses `prompt` (named `name` in logs and the dump) as the system prompt.
    #[must_use]
    pub fn with_prompt(mut self, name: &str, prompt: &str) -> Self {
        name.clone_into(&mut self.prompt_name);
        prompt.clone_into(&mut self.prompt);
        self
    }

    /// Sets (or clears) the dump directory, instead of the environment's.
    #[must_use]
    pub fn with_dump_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.dump_dir = dir;
        self
    }

    /// The model name.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Cleans `raw`: the reply if [`judge`] accepts it, else `raw`. Never
    /// fails; an empty `raw` is returned as is without a request.
    pub async fn clean(&self, raw: &str) -> Outcome {
        let started = Instant::now();
        if raw.trim().is_empty() {
            return Outcome {
                text: raw.to_owned(),
                reason: Reason::Empty,
                reply: None,
                latency: Duration::ZERO,
            };
        }
        let (reply, reason) = match self.request(raw).await {
            Ok(reply) => {
                let reason = judge(raw, &reply, &self.prompt);
                (Some(reply), reason)
            }
            Err(reason) => (None, reason),
        };
        let text = match (&reply, reason) {
            (Some(reply), Reason::Cleaned) => reply.clone(),
            _ => raw.to_owned(),
        };
        let outcome = Outcome {
            text,
            reason,
            reply,
            latency: started.elapsed(),
        };
        tracing::info!(
            used = if reason.cleaned() { "cleaned" } else { "raw" },
            reason = %reason.name(),
            model = %self.model,
            prompt = %self.prompt_name,
            latency_ms = outcome.latency.as_millis() as u64,
            raw_chars = raw.chars().count(),
            pasted_chars = outcome.text.chars().count(),
            "groq cleanup"
        );
        self.dump(raw, &outcome);
        outcome
    }

    async fn request(&self, raw: &str) -> Result<String, Reason> {
        let mut auth = HeaderValue::from_str(&format!("Bearer {}", self.key.expose()))
            .map_err(|_| Reason::HttpError(401))?;
        auth.set_sensitive(true);
        let response = self
            .client
            .post(&self.endpoint)
            .header(AUTHORIZATION, auth)
            .header(CONTENT_TYPE, "application/json")
            .timeout(self.timeout)
            .body(request_body(&self.model, &self.prompt, raw).to_string())
            .send()
            .await
            .map_err(|e| from_reqwest(&e))?;
        let status = response.status();
        if !status.is_success() {
            // The body isn't read: it could echo the request.
            return Err(Reason::HttpError(status.as_u16()));
        }
        let body = response.bytes().await.map_err(|e| from_reqwest(&e))?;
        parse_reply(&body).ok_or(Reason::BadReply)
    }

    /// Appends one line to the eval dump, if it is on. Never fails a
    /// dictation.
    fn dump(&self, raw: &str, outcome: &Outcome) {
        let Some(dir) = &self.dump_dir else { return };
        let line = serde_json::json!({
            "time_ms": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
            "model": self.model,
            "prompt": self.prompt_name,
            "prompt_sha": prompt_sha(&self.prompt),
            "raw_text": raw,
            "reply": outcome.reply,
            "pasted_text": outcome.text,
            "used": if outcome.reason.cleaned() { "cleaned" } else { "raw" },
            "reason": outcome.reason.name(),
            "cleanup_ms": outcome.latency.as_millis() as u64,
        });
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
}

impl std::fmt::Debug for Cleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cleanup")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("key", &self.key)
            .field("timeout", &self.timeout)
            .field("prompt", &self.prompt_name)
            .finish_non_exhaustive()
    }
}

/// The first 12 hex digits of the prompt's sha256, so runs can be matched
/// to the prompt they used.
#[must_use]
pub fn prompt_sha(prompt: &str) -> String {
    let digest = Sha256::digest(prompt.as_bytes());
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

/// The chat completions request. Reasoning models (`openai/gpt-oss-*`)
/// think briefly and keep their reasoning out of the reply.
fn request_body(model: &str, prompt: &str, raw: &str) -> serde_json::Value {
    let reasoning = model.starts_with("openai/gpt-oss");
    let budget = raw.chars().count() / 2 + if reasoning { 1_280 } else { 256 };
    let mut body = serde_json::json!({
        "model": model,
        "temperature": 0,
        "max_completion_tokens": budget,
        "messages": [
            { "role": "system", "content": prompt },
            { "role": "user", "content": groq_cleanup_prompt::user_message(raw) },
        ],
    });
    if reasoning {
        body["reasoning_effort"] = serde_json::json!("low");
        body["include_reasoning"] = serde_json::json!(false);
    }
    body
}

fn from_reqwest(error: &reqwest::Error) -> Reason {
    if error.is_timeout() {
        Reason::Timeout
    } else {
        Reason::Network
    }
}

#[derive(Deserialize)]
struct ChatReply {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    content: Option<String>,
}

/// The first choice's text, trimmed and without the transcript tags if the
/// model repeated them; `None` if the body isn't a chat reply.
fn parse_reply(body: &[u8]) -> Option<String> {
    let reply: ChatReply = serde_json::from_slice(body).ok()?;
    let content = reply.choices.into_iter().next()?.message.content?;
    let mut text = content.trim();
    text = text.strip_prefix(OPEN_TAG).unwrap_or(text).trim_start();
    text = text.strip_suffix(CLOSE_TAG).unwrap_or(text).trim_end();
    Some(text.to_owned())
}

/// Whether `reply` may replace `raw`: [`Reason::Cleaned`] if so, else the
/// first check it fails.
#[must_use]
pub fn judge(raw: &str, reply: &str, prompt: &str) -> Reason {
    if normalized(reply).is_empty() {
        return Reason::Empty;
    }
    if is_echo(reply, prompt) {
        return Reason::Echo;
    }
    let (raw_len, reply_len) = (raw.chars().count(), reply.chars().count());
    if reply_len as f64 > raw_len as f64 * LENGTH_RATIO + LENGTH_SLACK as f64 {
        return Reason::TooLong;
    }
    let kept = numbers(reply);
    if numbers(raw).iter().any(|n| !kept.contains(n)) {
        return Reason::DroppedNumber;
    }
    let lower = reply.to_lowercase();
    if links(raw).iter().any(|link| !lower.contains(link.as_str())) {
        return Reason::DroppedLink;
    }
    let words = word_parts(reply);
    if names(raw)
        .iter()
        .any(|name| word_parts(name).iter().any(|part| !words.contains(part)))
    {
        return Reason::DroppedName;
    }
    Reason::Cleaned
}

/// Whether `reply` is a run of `prompt`: the whole of the reply is inside
/// the prompt, or one of the prompt's longer sentences is inside the reply.
fn is_echo(reply: &str, prompt: &str) -> bool {
    let reply = format!(" {} ", normalized(reply));
    if format!(" {} ", normalized(prompt)).contains(&reply) {
        return true;
    }
    prompt
        .split(['.', '\n', ':', ';'])
        .map(normalized)
        .filter(|sentence| sentence.chars().count() >= SENTENCE_FLOOR)
        .any(|sentence| reply.contains(&format!(" {sentence} ")))
}

/// Lowercased letters and digits, single-spaced.
fn normalized(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c);
        } else {
            gap = true;
        }
    }
    out
}

/// The numbers in `text`: runs of digits, with a `.`, `,` or `:` between
/// two digits kept inside one number and thousands commas removed
/// (`"$1,750.50"` is `1750.50`).
fn numbers(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let joins = matches!(c, '.' | ',' | ':')
            && !current.is_empty()
            && chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if c.is_ascii_digit() {
            current.push(c);
        } else if joins {
            if c != ',' {
                current.push(c);
            }
        } else if !current.is_empty() {
            found.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        found.push(current);
    }
    found
}

/// Emails, URLs and dotted names (`northwind.io`, `billing.ts`) in `text`,
/// lowercased and without surrounding punctuation.
fn links(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| {
            token
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '/')
                .trim_end_matches('/')
                .to_lowercase()
        })
        .filter(|token| is_link(token))
        .collect()
}

fn is_link(token: &str) -> bool {
    if token.contains("://") {
        return true;
    }
    if let Some((user, host)) = token.split_once('@') {
        return !user.is_empty() && host.contains('.');
    }
    let host = token.split('/').next().unwrap_or_default();
    let labels: Vec<&str> = host.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty() && label.chars().all(|c| c.is_alphanumeric() || c == '-')
        })
        && labels
            .last()
            .is_some_and(|tld| tld.chars().count() >= 2 && tld.chars().all(char::is_alphabetic))
}

/// Capitalised words that don't start a sentence (`"Priya"` in "ask
/// Priya", not `"Okay"` in "Okay, so"), without surrounding punctuation.
/// "I" and its contractions don't count.
fn names(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut sentence_start = true;
    for token in text.split_whitespace() {
        let word = token.trim_matches(|c: char| !c.is_alphanumeric());
        let capitalised = word.chars().next().is_some_and(char::is_uppercase);
        let pronoun = word == "I" || word.starts_with("I'") || word.starts_with("I\u{2019}");
        if capitalised && !sentence_start && !pronoun {
            found.push(word.to_owned());
        }
        sentence_start = token.ends_with(['.', '!', '?', ':', '\n']) || word.is_empty();
    }
    found
}

/// The lowercased runs of letters and digits in `text`.
fn word_parts(text: &str) -> Vec<String> {
    normalized(text)
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = SYSTEM_PROMPT;

    #[test]
    fn a_tidy_reply_is_kept() {
        assert_eq!(
            judge(
                "Okay, so, um, can you, like, send it to Priya by 5 p.m.?",
                "Can you send it to Priya by 5 p.m.?",
                P
            ),
            Reason::Cleaned
        );
        // Spoken symbols written out keep the domain.
        assert_eq!(
            judge(
                "Email ops at northwind.io and the tracker under budget slash 2026.",
                "Email ops@northwind.io and the tracker under budget/2026.",
                P
            ),
            Reason::Cleaned
        );
    }

    #[test]
    fn empty_and_echoed_replies_fall_back() {
        assert_eq!(judge("Um, yeah.", "", P), Reason::Empty);
        assert_eq!(judge("Um, yeah.", " . ", P), Reason::Empty);
        assert_eq!(
            judge("Um, yeah.", "Return only the cleaned transcript.", P),
            Reason::Echo
        );
        assert_eq!(
            judge(
                "Yeah.",
                "Sure! Collapse a stammered immediate repeat of a single word to one copy.",
                P
            ),
            Reason::Echo
        );
    }

    #[test]
    fn a_much_longer_reply_falls_back() {
        let raw = "What's the capital of France?";
        let answer =
            "The capital of France is Paris. It has been the capital since the tenth century.";
        assert_eq!(judge(raw, answer, P), Reason::TooLong);
        // A little longer (punctuation added) is fine.
        assert_eq!(
            judge("ok see you at 5 then", "OK, see you at 5, then.", P),
            Reason::Cleaned
        );
    }

    #[test]
    fn a_dropped_number_falls_back() {
        let raw = "My number is 312-555-0164. Thanks for considering it.";
        assert_eq!(
            judge(raw, "My number is Thanks for considering it.", P),
            Reason::DroppedNumber
        );
        // Thousands commas don't matter; a changed amount does.
        assert_eq!(judge("It's $1,750.", "It's $1750.", P), Reason::Cleaned);
        assert_eq!(
            judge("It's $17.50.", "It's $1,750.", P),
            Reason::DroppedNumber
        );
        // A self-correction resolved drops the first value: raw text.
        assert_eq!(
            judge("January 23rd. Sorry, January 29th.", "January 29th.", P),
            Reason::DroppedNumber
        );
    }

    #[test]
    fn a_dropped_link_falls_back() {
        assert_eq!(
            judge(
                "Register at summit.northwind.io slash register.",
                "Register at the summit page.",
                P
            ),
            Reason::DroppedLink
        );
        assert_eq!(
            judge(
                "Write to d.ramirez@acme.com today.",
                "Write to them today.",
                P
            ),
            Reason::DroppedLink
        );
        assert_eq!(
            judge("See https://x.dev/a for it.", "See it.", P),
            Reason::DroppedLink
        );
        // Abbreviations aren't links.
        assert_eq!(
            judge("At 5 p.m. e.g. now.", "At 5 p.m. now.", P),
            Reason::Cleaned
        );
    }

    #[test]
    fn a_dropped_name_falls_back() {
        assert_eq!(
            judge(
                "So I talked to Tomas Herrera about it.",
                "I talked to him about it.",
                P
            ),
            Reason::DroppedName
        );
        // Sentence-initial capitals and "I" are not names.
        assert_eq!(
            judge(
                "Okay. So I think, um, it works. Like, really.",
                "I think it works. Really.",
                P
            ),
            Reason::Cleaned
        );
    }

    #[test]
    fn the_extractors_find_what_they_should() {
        assert_eq!(
            numbers("v2.4 on 4:30, $1,750 and 3."),
            ["2.4", "4:30", "1750", "3"]
        );
        assert_eq!(
            links("Mail Ops@Northwind.io, see fieldbook.app/new. Also billing.ts!"),
            ["ops@northwind.io", "fieldbook.app/new", "billing.ts"]
        );
        assert_eq!(
            names("Hi Marcus, I'm with Priya. Okay ask Owen Park's team"),
            ["Marcus", "Priya", "Owen", "Park's"]
        );
    }

    #[test]
    fn replies_are_read_without_the_tags() {
        let body = br#"{"choices":[{"message":{"role":"assistant","content":" <transcript>\nHi there.\n</transcript> "}}]}"#;
        assert_eq!(parse_reply(body).as_deref(), Some("Hi there."));
        assert_eq!(parse_reply(br#"{"choices":[]}"#), None);
        assert_eq!(parse_reply(b"nope"), None);
        assert_eq!(
            parse_reply(br#"{"choices":[{"message":{"content":null}}]}"#),
            None
        );
    }

    #[test]
    fn the_request_is_temperature_zero_with_the_transcript_as_data() {
        let body = request_body(FAST_MODEL, "rules", "hello");
        assert_eq!(body["model"], FAST_MODEL);
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["messages"][0]["content"], "rules");
        assert_eq!(
            body["messages"][1]["content"],
            "<transcript>\nhello\n</transcript>"
        );
        assert!(body.get("reasoning_effort").is_none());
        let body = request_body("openai/gpt-oss-120b", "rules", "hello");
        assert_eq!(body["reasoning_effort"], "low");
        assert_eq!(body["include_reasoning"], false);
    }

    #[test]
    fn reasons_have_names() {
        assert_eq!(Reason::HttpError(429).name(), "http_429");
        assert_eq!(Reason::DroppedNumber.name(), "dropped_number");
        assert!(Reason::Cleaned.cleaned() && !Reason::Timeout.cleaned());
        assert_eq!(prompt_sha("x").len(), 12);
    }
}
