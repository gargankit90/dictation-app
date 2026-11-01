//! Keeping API keys and transcripts out of logs and error messages
//! (`docs/ENGINEERING.md`: never log either at info level or above, and
//! never a key at any level).
//!
//! The first line of defence is types that cannot print the secret:
//! [`Hidden`] for text such as a transcript, and `dictation_ports::Secret`
//! for keys. [`secrets`] and [`summary`] are the second: they clean up free
//! text (an error message from a library or a provider) before it is shown
//! or logged. The log file's writer runs [`secrets`] over everything as a
//! last resort (`src-tauri/src/logging`).

use std::borrow::Cow;
use std::fmt;

/// What replaces a key found in text.
pub const KEY_PLACEHOLDER: &str = "[key hidden]";

/// The longest [`summary`], in characters.
pub const MAX_SUMMARY_CHARS: usize = 200;

/// Text that must not be logged, such as a transcript. Both `Display` and
/// `Debug` show only how long it is, so `%Hidden(&text)` and
/// `?Hidden(&text)` are safe in a log field at any level.
#[derive(Clone, Copy)]
pub struct Hidden<'a>(pub &'a str);

impl fmt::Display for Hidden<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{} chars hidden>", self.0.chars().count())
    }
}

impl fmt::Debug for Hidden<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// The prefixes of the API keys we know (Groq `gsk_…`, `OpenAI` `sk-…`,
/// including `sk-proj-…`).
const KEY_PREFIXES: [&str; 2] = ["gsk_", "sk-"];

/// A key has at least this many characters after its prefix; shorter
/// tokens are words that happen to start the same way.
const MIN_KEY_TAIL: usize = 4;

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')
}

fn looks_like_key(token: &str) -> bool {
    KEY_PREFIXES.iter().any(|prefix| {
        token
            .strip_prefix(prefix)
            .is_some_and(|tail| tail.chars().count() >= MIN_KEY_TAIL)
    })
}

/// `text` with every API key replaced by [`KEY_PLACEHOLDER`]: tokens that
/// look like a Groq or `OpenAI` key, and whatever follows `Bearer `. Borrows
/// when there is nothing to hide.
#[must_use]
pub fn secrets(text: &str) -> Cow<'_, str> {
    let mut out: Option<String> = None;
    let mut copied = 0;
    let mut after_bearer = false;
    let mut start = None;
    // A sentinel at the end closes the last token.
    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        match (start, is_token_char(c)) {
            (None, true) => start = Some(i),
            (Some(from), false) => {
                start = None;
                let token = &text[from..i];
                if looks_like_key(token) || (after_bearer && !token.is_empty()) {
                    let out = out.get_or_insert_with(String::new);
                    out.push_str(&text[copied..from]);
                    out.push_str(KEY_PLACEHOLDER);
                    copied = i;
                }
                // Only the header's spelling, so prose about "bearer bonds"
                // is left alone; a punctuation mark ends the pairing.
                after_bearer = token == "Bearer" && c.is_whitespace();
            }
            (Some(_), true) => {}
            // Anything but whitespace between "Bearer" and a word ends the
            // pairing.
            (None, false) => {
                if !c.is_whitespace() {
                    after_bearer = false;
                }
            }
        }
    }
    match out {
        Some(mut out) => {
            out.push_str(&text[copied..]);
            Cow::Owned(out)
        }
        None => Cow::Borrowed(text),
    }
}

/// A short, single-line, key-free version of `text` for an error message
/// that may come from outside: [`secrets`] applied, whitespace (newlines
/// included) collapsed to single spaces, and cut to [`MAX_SUMMARY_CHARS`]
/// characters with an ellipsis. A provider's whole error body therefore can't
/// ride along in an error string.
#[must_use]
pub fn summary(text: &str) -> String {
    let cleaned = secrets(text);
    let mut words = cleaned.split_whitespace();
    let mut out = String::new();
    for word in words.by_ref() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        if out.chars().count() > MAX_SUMMARY_CHARS {
            break;
        }
    }
    if out.chars().count() > MAX_SUMMARY_CHARS || words.next().is_some() {
        let mut cut: String = out.chars().take(MAX_SUMMARY_CHARS).collect();
        cut.push('…');
        return cut;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROQ: &str = "gsk_dummy_NOT_A_REAL_KEY_0123456789";
    const OPENAI: &str = "sk-proj-NOTAREALKEY0123456789abcdef";

    #[test]
    fn hidden_shows_only_the_length() {
        let text = "secret words";
        assert_eq!(format!("{}", Hidden(text)), "<12 chars hidden>");
        assert_eq!(format!("{:?}", Hidden(text)), "<12 chars hidden>");
        // Characters, not bytes.
        assert_eq!(Hidden("héllo").to_string(), "<5 chars hidden>");
        assert_eq!(Hidden("").to_string(), "<0 chars hidden>");
    }

    #[test]
    fn keys_are_replaced_wherever_they_appear() {
        for key in [GROQ, OPENAI, "gsk_invalid_test_key"] {
            let text = format!("401 for {key}, retrying (key={key}); \"{key}\"");
            let cleaned = secrets(&text);
            assert!(!cleaned.contains(key), "{cleaned}");
            assert_eq!(
                cleaned.matches(KEY_PLACEHOLDER).count(),
                3,
                "all three: {cleaned}"
            );
        }
    }

    #[test]
    fn text_without_a_key_is_borrowed_unchanged() {
        for text in [
            "",
            "the provider failed (HTTP 503)",
            "task-force desk-top gsk_ sk-1 sk- gsk_abc",
            "Bearer",
            "a bearer of news",
        ] {
            let cleaned = secrets(text);
            assert!(matches!(cleaned, Cow::Borrowed(_)), "{text:?} -> {cleaned}");
            assert_eq!(cleaned, text);
        }
    }

    #[test]
    fn a_bearer_token_is_hidden_whatever_it_looks_like() {
        assert_eq!(
            secrets("header Authorization: Bearer abc123xyz failed"),
            format!("header Authorization: Bearer {KEY_PLACEHOLDER} failed")
        );
        assert_eq!(
            secrets("Bearer   spaced.token-1"),
            format!("Bearer   {KEY_PLACEHOLDER}")
        );
        // Not carried over to a later word.
        assert_eq!(secrets("Bearer: nothing"), "Bearer: nothing");
    }

    #[test]
    fn a_key_at_the_start_or_end_and_next_to_punctuation() {
        assert_eq!(secrets(GROQ), KEY_PLACEHOLDER);
        assert_eq!(
            secrets(&format!("({GROQ})")),
            format!("({KEY_PLACEHOLDER})")
        );
        assert_eq!(
            secrets(&format!("é{OPENAI}é")),
            format!("é{KEY_PLACEHOLDER}é")
        );
    }

    #[test]
    fn summary_is_one_short_line() {
        assert_eq!(summary("a\n  b\t c"), "a b c");
        assert_eq!(summary(""), "");
        assert_eq!(
            summary(&format!("bad {GROQ}\nnext")),
            "bad [key hidden] next"
        );
        let long = "word ".repeat(100);
        let short = summary(&long);
        assert_eq!(short.chars().count(), MAX_SUMMARY_CHARS + 1, "{short}");
        assert!(short.ends_with('…'));
        // Exactly at the limit is kept whole.
        let exact = "x".repeat(MAX_SUMMARY_CHARS);
        assert_eq!(summary(&exact), exact);
        let one_long_word = "y".repeat(MAX_SUMMARY_CHARS * 3);
        assert_eq!(
            summary(&one_long_word).chars().count(),
            MAX_SUMMARY_CHARS + 1
        );
    }
}
