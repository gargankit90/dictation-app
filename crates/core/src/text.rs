//! Transcript cleanup, run on every transcript before it is pasted:
//!
//! 1. **Trim** surrounding whitespace.
//! 2. **Drop empty** results (nothing left after trimming).
//! 3. **Drop Whisper-on-silence phrases.** Whisper-family models invent
//!    stock phrases ("Thank you.", "Thanks for watching!") when given
//!    silence or noise. A result is dropped only if it is *exactly* one of
//!    [`SILENCE_PHRASES`] **and** the engine also says there was no speech
//!    ([`Transcript::no_speech_prob`] at or above [`NO_SPEECH_THRESHOLD`]).
//!    A user who really says "thank you" has a low probability and keeps
//!    their text; an engine that reports no probability never triggers the
//!    filter, since without it silence can't be told from a short reply.
//! 4. **Smart leading space.** [`clean`] takes the character before the
//!    cursor, if the caller knows it, and starts the text with one space
//!    when it would otherwise be glued to the word before it.
//!
//! The pipeline calls [`clean`] with `preceding: None` today: no adapter
//! reads the text before the cursor yet (that needs the macOS
//! Accessibility API), so the rule *when the preceding character is
//! unknown: add no space*. A missing space is easy to fix by hand; a stray
//! one in a fresh field or after a newline is not what the user asked for.
//!
//! Pure functions, no I/O. Nothing here logs: a transcript is user content.

use std::fmt;

/// Mean `no_speech_prob` (over the segments) at or above which an engine is
/// taken to say "there was no speech".
///
/// 0.6 is the default `no_speech_threshold` of OpenAI's reference Whisper
/// implementation, which treats a segment above it as silent (in
/// combination with a low average log-probability, which the API doesn't
/// return per request). We use the same number rather than inventing one:
/// real speech scores far below it, silence and noise that make Whisper
/// hallucinate score near 1, and because the phrase must match exactly as
/// well, a wrong call costs at most one very short, stock-phrase dictation.
pub const NO_SPEECH_THRESHOLD: f64 = 0.6;

/// The stock phrases Whisper-family models produce for silence. Compared
/// after trimming, ignoring ASCII case, and as a whole: "Thank you so much"
/// is never a match. Kept small on purpose; add a phrase only when seen in
/// the wild.
pub const SILENCE_PHRASES: &[&str] = &[
    "Thank you.",
    "Thank you!",
    "Thank you",
    "Thanks for watching!",
    "Thanks for watching.",
    "Thanks for watching",
    "Thank you for watching.",
    "Thank you for watching!",
    "Thank you for watching",
    "you",
    "you.",
    ".",
    "Bye.",
    "Bye!",
    "Bye",
];

/// What a transcriber returned: the text, plus what it said about speech.
///
/// `Debug` shows the text's length, never the text.
#[derive(Clone, PartialEq)]
pub struct Transcript {
    /// The transcribed text, untrimmed.
    pub text: String,
    /// The engine's probability that the audio had no speech (0 to 1), the
    /// mean over its segments ([`mean_no_speech_prob`]). `None` if the
    /// engine doesn't report one.
    pub no_speech_prob: Option<f64>,
}

impl Transcript {
    /// A transcript from an engine that reports no speech probability.
    #[must_use]
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            no_speech_prob: None,
        }
    }
}

impl fmt::Debug for Transcript {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Transcript")
            .field("len", &self.text.len())
            .field("no_speech_prob", &self.no_speech_prob)
            .finish()
    }
}

/// Why [`clean`] dropped a transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dropped {
    /// Nothing but whitespace.
    Empty,
    /// A [`SILENCE_PHRASES`] match with a high `no_speech_prob`.
    Silence,
}

/// The result of [`clean`].
#[derive(Clone, PartialEq, Eq)]
pub enum Cleaned {
    /// Paste this text.
    Text(String),
    /// Paste nothing, and don't tell the user.
    Dropped(Dropped),
}

impl fmt::Debug for Cleaned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cleaned::Text(text) => f.debug_struct("Text").field("len", &text.len()).finish(),
            Cleaned::Dropped(why) => f.debug_tuple("Dropped").field(why).finish(),
        }
    }
}

/// The mean of the segments' `no_speech_prob`s, ignoring values that aren't
/// finite; `None` if there are none (no segments, so nothing to judge by).
#[must_use]
pub fn mean_no_speech_prob(probs: impl IntoIterator<Item = f64>) -> Option<f64> {
    let (sum, count) = probs
        .into_iter()
        .filter(|p| p.is_finite())
        .fold((0.0, 0u32), |(sum, count), p| (sum + p, count + 1));
    (count > 0).then(|| sum / f64::from(count))
}

/// Cleans `transcript` for pasting (see the module docs). `preceding` is
/// the character before the cursor in the target app, if known.
#[must_use]
pub fn clean(transcript: &Transcript, preceding: Option<char>) -> Cleaned {
    let text = transcript.text.trim();
    if text.is_empty() {
        return Cleaned::Dropped(Dropped::Empty);
    }
    if is_silence_hallucination(text, transcript.no_speech_prob) {
        return Cleaned::Dropped(Dropped::Silence);
    }
    if needs_leading_space(text, preceding) {
        Cleaned::Text(format!(" {text}"))
    } else {
        Cleaned::Text(text.to_owned())
    }
}

/// Whether `text` (already trimmed) is a stock silence phrase and
/// `no_speech_prob` says there was no speech.
fn is_silence_hallucination(text: &str, no_speech_prob: Option<f64>) -> bool {
    no_speech_prob.is_some_and(|p| p >= NO_SPEECH_THRESHOLD)
        && SILENCE_PHRASES
            .iter()
            .any(|phrase| phrase.eq_ignore_ascii_case(text))
}

/// Whether a space belongs between `preceding` and `text` (non-empty,
/// trimmed): the character before is a real character that isn't
/// whitespace or an opener, and the text doesn't start with punctuation
/// that attaches to the word before it.
fn needs_leading_space(text: &str, preceding: Option<char>) -> bool {
    let Some(before) = preceding else {
        return false;
    };
    let opener = matches!(before, '(' | '[' | '{' | '\u{201c}' | '\u{2018}' | '\u{ab}');
    let attaches = text
        .chars()
        .next()
        .is_some_and(|c| matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}'));
    !before.is_whitespace() && !before.is_control() && !opener && !attaches
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn said(text: &str, no_speech_prob: f64) -> Transcript {
        Transcript {
            text: text.into(),
            no_speech_prob: Some(no_speech_prob),
        }
    }

    fn text(cleaned: &Cleaned) -> &str {
        match cleaned {
            Cleaned::Text(text) => text,
            Cleaned::Dropped(why) => panic!("dropped: {why:?}"),
        }
    }

    // Trimming.

    #[test]
    fn trims_surrounding_whitespace_of_every_kind() {
        for raw in [
            "hello world",
            "  hello world",
            "hello world \n",
            "\t hello world\r\n ",
            "\u{a0}hello world\u{3000}",
        ] {
            let cleaned = clean(&Transcript::from_text(raw), None);
            assert_eq!(text(&cleaned), "hello world", "{raw:?}");
        }
    }

    #[test]
    fn keeps_inner_whitespace_and_text_as_is() {
        let cleaned = clean(&Transcript::from_text(" line one\n\nline  two "), None);
        assert_eq!(text(&cleaned), "line one\n\nline  two");
    }

    // Empty results.

    #[test]
    fn empty_and_blank_results_are_dropped() {
        for raw in ["", " ", "\n", "\t\r\n ", "\u{3000}", "\u{a0}\u{2003}"] {
            assert_eq!(
                clean(&Transcript::from_text(raw), None),
                Cleaned::Dropped(Dropped::Empty),
                "{raw:?}"
            );
            // Even with a space to add, there's nothing to paste.
            assert_eq!(
                clean(&Transcript::from_text(raw), Some('a')),
                Cleaned::Dropped(Dropped::Empty),
                "{raw:?}"
            );
        }
    }

    // Whisper on silence.

    #[test]
    fn a_silence_phrase_with_high_no_speech_prob_is_dropped() {
        for phrase in SILENCE_PHRASES {
            for prob in [NO_SPEECH_THRESHOLD, 0.8, 1.0] {
                assert_eq!(
                    clean(&said(phrase, prob), None),
                    Cleaned::Dropped(Dropped::Silence),
                    "{phrase:?} at {prob}"
                );
            }
        }
    }

    #[test]
    fn a_silence_phrase_with_low_no_speech_prob_is_kept() {
        for phrase in SILENCE_PHRASES {
            for prob in [0.0, 0.1, 0.59] {
                assert_eq!(
                    clean(&said(phrase, prob), None),
                    Cleaned::Text((*phrase).to_owned()),
                    "{phrase:?} at {prob}"
                );
            }
        }
    }

    #[test]
    fn a_silence_phrase_is_kept_when_the_engine_reports_no_probability() {
        let cleaned = clean(&Transcript::from_text("Thank you."), None);
        assert_eq!(text(&cleaned), "Thank you.");
    }

    #[test]
    fn the_threshold_is_inclusive() {
        let below = NO_SPEECH_THRESHOLD - 1e-9;
        assert!(matches!(clean(&said("you", below), None), Cleaned::Text(_)));
        assert_eq!(
            clean(&said("you", NO_SPEECH_THRESHOLD), None),
            Cleaned::Dropped(Dropped::Silence)
        );
    }

    #[test]
    fn other_text_is_never_dropped_however_silent_the_engine_thinks_it_was() {
        for raw in [
            "Thank you so much for coming.",
            "Thank you, Anna.",
            "you are right",
            "Bye bye",
            "Hello",
            "Thanks",
        ] {
            assert_eq!(
                clean(&said(raw, 1.0), None),
                Cleaned::Text(raw.to_owned()),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn the_phrase_match_ignores_case_and_surrounding_whitespace() {
        for raw in [
            "  thank you.\n",
            "THANK YOU.",
            "Thanks For Watching!",
            " You ",
        ] {
            assert_eq!(
                clean(&said(raw, 0.9), None),
                Cleaned::Dropped(Dropped::Silence),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn mean_no_speech_prob_averages_the_segments() {
        assert_eq!(mean_no_speech_prob([]), None);
        assert_eq!(mean_no_speech_prob([0.25]), Some(0.25));
        let mean = mean_no_speech_prob([0.2, 0.4, 0.9]).unwrap();
        assert!((mean - 0.5).abs() < 1e-12);
    }

    #[test]
    fn mean_no_speech_prob_skips_values_that_are_not_numbers() {
        assert_eq!(mean_no_speech_prob([f64::NAN, f64::INFINITY]), None);
        assert_eq!(mean_no_speech_prob([f64::NAN, 0.5]), Some(0.5));
    }

    // Smart leading space.

    #[test]
    fn a_space_is_added_after_a_non_space_character() {
        for before in ['a', 'Z', '7', '.', ',', '!', ')', '\u{201d}', 'é', '字'] {
            let cleaned = clean(&Transcript::from_text("next word"), Some(before));
            assert_eq!(text(&cleaned), " next word", "{before:?}");
        }
    }

    #[test]
    fn no_space_is_added_at_the_start_or_after_whitespace_or_an_opener() {
        for before in [
            ' ', '\n', '\t', '\u{a0}', '(', '[', '{', '\u{201c}', '\u{2018}',
        ] {
            let cleaned = clean(&Transcript::from_text("next word"), Some(before));
            assert_eq!(text(&cleaned), "next word", "{before:?}");
        }
    }

    #[test]
    fn no_space_is_added_when_the_preceding_character_is_unknown() {
        let cleaned = clean(&Transcript::from_text("next word"), None);
        assert_eq!(text(&cleaned), "next word");
    }

    #[test]
    fn no_space_is_added_before_punctuation_that_attaches_to_the_word() {
        for raw in [", and then", ". Next", "?", ")"] {
            let cleaned = clean(&Transcript::from_text(raw), Some('d'));
            assert_eq!(text(&cleaned), raw, "{raw:?}");
        }
    }

    #[test]
    fn the_space_is_added_after_trimming_not_doubled() {
        let cleaned = clean(&Transcript::from_text("   spaced out  "), Some('x'));
        assert_eq!(text(&cleaned), " spaced out");
    }

    #[test]
    fn debug_never_shows_the_text() {
        let secret = "my secret words";
        assert!(!format!("{:?}", Transcript::from_text(secret)).contains("secret"));
        assert!(!format!("{:?}", Cleaned::Text(secret.into())).contains("secret"));
    }

    proptest! {
        // Without a preceding character the output is the trimmed text, so
        // cleaning is idempotent and the result never starts or ends with
        // whitespace.
        #[test]
        fn cleaning_is_idempotent_and_trims(raw in "\\PC{0,40}", prob in proptest::option::of(0.0..=1.0f64)) {
            let first = clean(&Transcript { text: raw, no_speech_prob: prob }, None);
            if let Cleaned::Text(out) = &first {
                prop_assert!(!out.is_empty());
                prop_assert_eq!(out.trim(), out.as_str());
                let again = clean(&Transcript { text: out.clone(), no_speech_prob: prob }, None);
                prop_assert_eq!(again, first);
            }
        }

        // A leading space is the only thing the preceding character can add.
        #[test]
        fn the_preceding_character_only_adds_one_space(
            raw in "\\PC{0,40}",
            before in proptest::option::of(any::<char>()),
        ) {
            let plain = clean(&Transcript::from_text(raw.clone()), None);
            let spaced = clean(&Transcript::from_text(raw), before);
            match (plain, spaced) {
                (Cleaned::Text(plain), Cleaned::Text(spaced)) => {
                    let with_space = format!(" {plain}");
                    prop_assert!(spaced == plain || spaced == with_space);
                }
                (plain, spaced) => prop_assert_eq!(plain, spaced),
            }
        }
    }
}
