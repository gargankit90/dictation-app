//! [`OpenRouterCleanup`]: the cleanup pass after transcription (Q2), through
//! `OpenRouter`'s `POST {base_url}/chat/completions`.
//!
//! The transcript's text (never the audio) goes up with the cleanup prompt
//! (`dictation_core::cleanup`), temperature 0, low reasoning effort for a
//! model that reasons by default (GLM), and the model's privacy filter:
//! `provider: {zdr: true, data_collection: "deny"}` for the default cleanup
//! model. The filter is never dropped; if no provider matches, the request
//! fails and the raw transcript is pasted. The reply's text comes back
//! without the transcript tags; the pipeline judges it.
//!
//! One attempt, no retries: the pipeline's deadline
//! (`dictation_core::cleanup::TIMEOUT`) is shorter than any backoff, and
//! dropping the future (the deadline passing) cancels the request.
//!
//! Privacy: the key goes only into the `Authorization` header; errors hold
//! statuses only; logs (debug) hold lengths and statuses, never a text.

use std::time::Duration;

use dictation_core::cleanup;
use dictation_core::cloud_models::{self, CloudModel};
use dictation_ports::{Cleaner, CleanupError, Secret};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::openrouter::{BASE_URL, headers, provider_filter};

/// Output tokens a reply may use beyond half the transcript's characters:
/// room for a short reasoning pass, as in the eval.
const TOKEN_ROOM: usize = 1_280;

/// The cleanup engine: a chat model at `OpenRouter` and the user's key.
pub struct OpenRouterCleanup {
    client: Client,
    endpoint: String,
    model: &'static CloudModel,
    key: Secret,
    timeout: Duration,
}

impl OpenRouterCleanup {
    /// The catalog's cleanup model at [`BASE_URL`] with `key`.
    ///
    /// # Errors
    ///
    /// [`CleanupError::Network`] if the HTTP client can't be built.
    pub fn new(key: Secret) -> Result<Self, CleanupError> {
        Self::at(BASE_URL, key)
    }

    /// As [`new`](Self::new), at `base_url` (tests point it at a local
    /// server).
    ///
    /// # Errors
    ///
    /// [`CleanupError::Network`] if the HTTP client can't be built.
    pub fn at(base_url: &str, key: Secret) -> Result<Self, CleanupError> {
        let client = crate::http::client(base_url).map_err(|_| CleanupError::Network)?;
        Ok(Self {
            client,
            endpoint: format!("{}/chat/completions", base_url.trim_end_matches('/')),
            model: cloud_models::default_cleanup(),
            key,
            timeout: cleanup::TIMEOUT,
        })
    }

    /// Uses `timeout` for the request instead of the cleanup deadline.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The model's id.
    #[must_use]
    pub fn model(&self) -> &'static str {
        self.model.id
    }

    async fn request(&self, text: &str) -> Result<String, CleanupError> {
        // The error never echoes the key.
        let headers = headers(&self.key).map_err(|_| CleanupError::InvalidKey)?;
        let response = self
            .client
            .post(&self.endpoint)
            .headers(headers)
            .timeout(self.timeout)
            .body(request_body(self.model, text).to_string())
            .send()
            .await
            .map_err(|e| from_reqwest(&e))?;
        let status = response.status();
        if !status.is_success() {
            // The body isn't read: it could echo the request.
            return Err(from_status(status));
        }
        let body = response.bytes().await.map_err(|e| from_reqwest(&e))?;
        parse_reply(&body).ok_or(CleanupError::BadReply)
    }
}

impl Cleaner for OpenRouterCleanup {
    async fn clean(&self, text: &str) -> Result<String, CleanupError> {
        let result = self.request(text).await;
        match &result {
            Ok(reply) => tracing::debug!(
                model = self.model.id,
                chars = text.chars().count(),
                reply_chars = reply.chars().count(),
                "cleanup replied"
            ),
            Err(error) => tracing::debug!(model = self.model.id, %error, "cleanup failed"),
        }
        result
    }
}

impl std::fmt::Debug for OpenRouterCleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenRouterCleanup")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model.id)
            .field("key", &self.key)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// The chat request for `text`.
fn request_body(model: &CloudModel, text: &str) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model.id,
        "temperature": 0,
        "max_tokens": text.chars().count() / 2 + TOKEN_ROOM,
        "messages": [
            { "role": "system", "content": cleanup::prompt() },
            { "role": "user", "content": cleanup::user_message(text) },
        ],
        "provider": provider_filter(model.privacy),
    });
    if model.low_reasoning {
        body["reasoning"] = serde_json::json!({ "effort": "low", "exclude": true });
    }
    body
}

fn from_status(status: StatusCode) -> CleanupError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => CleanupError::InvalidKey,
        StatusCode::PAYMENT_REQUIRED => CleanupError::OutOfCredits,
        StatusCode::NOT_FOUND => CleanupError::NoRoute,
        StatusCode::TOO_MANY_REQUESTS => CleanupError::RateLimited,
        s => CleanupError::Provider(s.as_u16()),
    }
}

fn from_reqwest(error: &reqwest::Error) -> CleanupError {
    if error.is_timeout() {
        CleanupError::Timeout
    } else {
        CleanupError::Network
    }
}

#[derive(Deserialize)]
struct ChatReply {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
    /// OpenRouter's normalised reason: `stop` for a finished reply;
    /// `length`, `content_filter` or `error` for a cut-off one.
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    content: Option<String>,
}

/// The first choice's text without the transcript tags; `None` if the body
/// isn't a finished chat reply with text. A reply cut off by the token limit
/// or a filter (any `finish_reason` but `stop`, or none) could be a clean
/// prefix the guard accepts, so it counts as no reply: the raw text is pasted.
fn parse_reply(body: &[u8]) -> Option<String> {
    let reply: ChatReply = serde_json::from_slice(body).ok()?;
    let choice = reply.choices.into_iter().next()?;
    if choice.finish_reason.as_deref() != Some("stop") {
        return None;
    }
    let content = choice.message.content?;
    Some(cleanup::strip_tags(&content).to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests fail on purpose")]
mod tests {
    use super::*;

    #[test]
    fn the_request_is_glm_with_zdr_low_reasoning_and_the_transcript_as_data() {
        let body = request_body(cloud_models::default_cleanup(), "hello");
        assert_eq!(body["model"], "z-ai/glm-5.3-flash");
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["max_tokens"], 2 + 1_280);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], cleanup::prompt());
        assert_eq!(
            body["messages"][1]["content"],
            "<transcript>\nhello\n</transcript>"
        );
        assert_eq!(
            body["provider"],
            serde_json::json!({"zdr": true, "data_collection": "deny"})
        );
        assert_eq!(
            body["reasoning"],
            serde_json::json!({"effort": "low", "exclude": true})
        );
    }

    #[test]
    fn statuses_map_to_typed_errors() {
        for (status, error) in [
            (401, CleanupError::InvalidKey),
            (403, CleanupError::InvalidKey),
            (402, CleanupError::OutOfCredits),
            (404, CleanupError::NoRoute),
            (429, CleanupError::RateLimited),
            (500, CleanupError::Provider(500)),
            (503, CleanupError::Provider(503)),
            (400, CleanupError::Provider(400)),
        ] {
            assert_eq!(from_status(StatusCode::from_u16(status).unwrap()), error);
        }
    }

    #[test]
    fn replies_are_read_without_the_tags() {
        let body = br#"{"choices":[{"message":{"role":"assistant","content":" <transcript>\nHi there.\n</transcript> "},"finish_reason":"stop"}]}"#;
        assert_eq!(parse_reply(body).as_deref(), Some("Hi there."));
        assert_eq!(parse_reply(br#"{"choices":[]}"#), None);
        assert_eq!(parse_reply(b"nope"), None);
        assert_eq!(
            parse_reply(br#"{"choices":[{"message":{"content":null},"finish_reason":"stop"}]}"#),
            None
        );
    }

    /// Review Q2 round 1: a cut-off reply is never pasted.
    #[test]
    fn unfinished_replies_are_bad_replies() {
        for reason in [r#""length""#, r#""content_filter""#, r#""error""#, "null"] {
            let body = format!(
                r#"{{"choices":[{{"message":{{"content":"Please send the report"}},"finish_reason":{reason}}}]}}"#
            );
            assert_eq!(parse_reply(body.as_bytes()), None, "{reason}");
        }
        let missing = br#"{"choices":[{"message":{"content":"Please send the report"}}]}"#;
        assert_eq!(parse_reply(missing), None);
    }

    #[test]
    fn debug_redacts_the_key() {
        let cleanup = OpenRouterCleanup::new(Secret::new("k-1234567890")).unwrap();
        let shown = format!("{cleanup:?}");
        assert!(!shown.contains("k-1234567890"), "{shown}");
        assert_eq!(cleanup.model(), "z-ai/glm-5.3-flash");
    }
}
