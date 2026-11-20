//! On-device cleanup (Q4): the prompt and settings for the local cleanup
//! model, exactly as PR #326 evaluated them (`eval/local-cleanup` on that
//! branch: `prompts.py` v3 and `clean.py`), where Gemma 4 E2B with these
//! took OrukQwen's drafts from 58.9% to 76.8% sendable (PR #331).
//!
//! Pure data and two small functions; the runner lives in
//! `dictation-transcribe` (`llama`). Prompt v1, on which v3 builds, is
//! adapted from Blurt's cleanup instruction (MIT, credited in `NOTICE`).

use std::time::Duration;

/// The system prompt: `prompts.py`'s `PROMPTS["v3"][0]`, byte for byte.
pub const SYSTEM_PROMPT: &str = include_str!("local_cleanup/prompt_v3.txt");

/// Sampling temperature: greedy.
pub const TEMPERATURE: f32 = 0.0;

/// Top-k sampling: 1, greedy.
pub const TOP_K: u32 = 1;

/// The fixed seed.
pub const SEED: u32 = 1;

/// The context window, in tokens (`-c 4096`). The prompt's fixed part is
/// about 870 tokens; the rest is the transcript and the reply.
pub const CONTEXT_TOKENS: u32 = 4096;

/// How long one cleanup may take before the raw text is pasted instead.
/// PR #326 measured 0.52 s median, 1.35 s p90 and 4.7 s worst on an M4 Pro;
/// slower Macs need more, and a dictation must never stall for long.
pub const TIMEOUT: Duration = Duration::from_secs(4);

/// The user turn for `raw`: the transcript between `<transcript>` tags.
#[must_use]
pub fn user_message(raw: &str) -> String {
    format!("<transcript>\n{raw}\n</transcript>")
}

/// The most tokens the reply may have: half the transcript's length in
/// characters plus 100, as the eval capped it (`len(raw) // 2 + 100`).
#[must_use]
pub fn max_tokens(raw: &str) -> u32 {
    let half = raw.chars().count() / 2;
    u32::try_from(half).map_or(u32::MAX, |half| half.saturating_add(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_is_v3_byte_for_byte() {
        // SHA-256 of prompts.py's PROMPTS["v3"][0] on eval/local-cleanup,
        // computed with Python from that file on 2026-11-20.
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(SYSTEM_PROMPT.as_bytes());
        let hex = digest.iter().fold(String::new(), |mut hex, b| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{b:02x}");
            hex
        });
        assert_eq!(
            hex,
            "7b791b1cfc3d40d71fc6f73c5ceac5c1893d8696e34194f53186e56773ce9585"
        );
        assert!(SYSTEM_PROMPT.starts_with("You clean up dictated speech-to-text transcripts."));
        assert!(SYSTEM_PROMPT.ends_with("Return only the cleaned transcript."));
    }

    #[test]
    fn the_user_turn_wraps_the_transcript() {
        assert_eq!(
            user_message("um hello"),
            "<transcript>\num hello\n</transcript>"
        );
    }

    #[test]
    fn the_reply_cap_is_half_the_characters_plus_100() {
        assert_eq!(max_tokens(""), 100);
        assert_eq!(max_tokens("abcd"), 102);
        // Characters, not bytes, as Python's len().
        assert_eq!(max_tokens("éééé"), 102);
        assert_eq!(max_tokens(&"a".repeat(1001)), 600);
    }

    #[test]
    fn the_settings_are_the_evals() {
        assert!(TEMPERATURE.abs() < f32::EPSILON);
        assert_eq!((TOP_K, SEED, CONTEXT_TOKENS), (1, 1, 4096));
        assert!(TIMEOUT >= Duration::from_secs(3));
    }
}
