//! Transcript cleanup, run on every transcript before it is pasted:
//!
//! 1. **Trim** surrounding whitespace.
//! 2. **Drop empty** results (nothing left after trimming).
//! 3. **Drop results with no words**: nothing but punctuation and
//!    whitespace, no letter or digit in any script (`.`, `...`, `?!`). A
//!    short, near-silent recording can come back as `.` whatever the engine
//!    says about speech (#172); pasted, it would replace the user's
//!    selection with a dot. Nobody dictates a lone punctuation mark.
//! 4. **Smart leading space.** [`clean`] takes the character before the
//!    cursor, if the caller knows it, and starts the text with one space
//!    when it would otherwise be glued to the word before it.
//!
//! The pipeline reads that character from the focused app (the
//! `TextBeforeCursor` port; macOS Accessibility, M5.5a) right after the
//! transcript arrives. At the start of a field, or when the app doesn't
//! say (no text cursor, a password field, a read that fails or times out),
//! it passes `None`, and the rule is *when the preceding character is
//! unknown: add no space*. A missing space is easy to fix by hand; a stray
//! one in a fresh field or after a newline is not what the user asked for.
//!
//! Pure functions, no I/O. Nothing here logs: a transcript is user content.

use std::fmt;

/// Why [`clean`] dropped a transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dropped {
    /// Nothing but whitespace.
    Empty,
    /// No letter or digit: only punctuation, symbols and whitespace.
    NoWords,
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

/// Cleans `transcript` for pasting (see the module docs). `preceding` is
/// the character before the cursor in the target app, if known.
#[must_use]
pub fn clean(transcript: &str, preceding: Option<char>) -> Cleaned {
    let text = transcript.trim();
    if text.is_empty() {
        return Cleaned::Dropped(Dropped::Empty);
    }
    if !text.chars().any(char::is_alphanumeric) {
        return Cleaned::Dropped(Dropped::NoWords);
    }
    if needs_leading_space(text, preceding) {
        Cleaned::Text(format!(" {text}"))
    } else {
        Cleaned::Text(text.to_owned())
    }
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
            let cleaned = clean(raw, None);
            assert_eq!(text(&cleaned), "hello world", "{raw:?}");
        }
    }

    #[test]
    fn keeps_inner_whitespace_and_text_as_is() {
        let cleaned = clean(" line one\n\nline  two ", None);
        assert_eq!(text(&cleaned), "line one\n\nline  two");
    }

    // Empty results.

    #[test]
    fn empty_and_blank_results_are_dropped() {
        for raw in ["", " ", "\n", "\t\r\n ", "\u{3000}", "\u{a0}\u{2003}"] {
            assert_eq!(
                clean(raw, None),
                Cleaned::Dropped(Dropped::Empty),
                "{raw:?}"
            );
            // Even with a space to add, there's nothing to paste.
            assert_eq!(
                clean(raw, Some('a')),
                Cleaned::Dropped(Dropped::Empty),
                "{raw:?}"
            );
        }
    }

    // No words (#172).

    /// M5.9 criterion 5: a transcript with no letter or digit is dropped,
    /// whatever the engine says about speech and wherever the cursor is.
    #[test]
    fn criterion_5_punctuation_and_symbols_alone_are_dropped() {
        for raw in [
            ".",
            " . ",
            "...",
            "?!",
            "\u{2026}",
            "-",
            "\u{3002}",
            "( )",
            ". ,\n;",
            "\u{1f600}",
        ] {
            for preceding in [None, Some('a')] {
                assert_eq!(
                    clean(raw, preceding),
                    Cleaned::Dropped(Dropped::NoWords),
                    "{raw:?} {preceding:?}"
                );
            }
        }
    }

    #[test]
    fn one_letter_or_digit_in_any_script_is_enough_to_keep_it() {
        for raw in [
            "a", "7", "I.", "?a", "\u{5b57}", "\u{e9}!", "\u{661}", "...ok",
        ] {
            assert_eq!(clean(raw, None), Cleaned::Text(raw.to_owned()), "{raw:?}");
        }
    }

    // Smart leading space.

    #[test]
    fn a_space_is_added_after_a_non_space_character() {
        for before in ['a', 'Z', '7', '.', ',', '!', ')', '\u{201d}', 'é', '字'] {
            let cleaned = clean("next word", Some(before));
            assert_eq!(text(&cleaned), " next word", "{before:?}");
        }
    }

    #[test]
    fn no_space_is_added_at_the_start_or_after_whitespace_or_an_opener() {
        for before in [
            ' ', '\n', '\t', '\u{a0}', '(', '[', '{', '\u{201c}', '\u{2018}',
        ] {
            let cleaned = clean("next word", Some(before));
            assert_eq!(text(&cleaned), "next word", "{before:?}");
        }
    }

    #[test]
    fn no_space_is_added_when_the_preceding_character_is_unknown() {
        let cleaned = clean("next word", None);
        assert_eq!(text(&cleaned), "next word");
    }

    #[test]
    fn no_space_is_added_before_punctuation_that_attaches_to_the_word() {
        for raw in [", and then", ". Next", "? Yes", ") then"] {
            let cleaned = clean(raw, Some('d'));
            assert_eq!(text(&cleaned), raw, "{raw:?}");
        }
    }

    #[test]
    fn the_space_is_added_after_trimming_not_doubled() {
        let cleaned = clean("   spaced out  ", Some('x'));
        assert_eq!(text(&cleaned), " spaced out");
    }

    #[test]
    fn debug_never_shows_the_text() {
        let secret = "my secret words";
        assert!(!format!("{:?}", Cleaned::Text(secret.into())).contains("secret"));
    }

    proptest! {
        // Without a preceding character the output is the trimmed text, so
        // cleaning is idempotent and the result never starts or ends with
        // whitespace.
        #[test]
        fn cleaning_is_idempotent_and_trims(raw in "\\PC{0,40}") {
            let first = clean(&raw, None);
            if let Cleaned::Text(out) = &first {
                prop_assert!(!out.is_empty());
                prop_assert_eq!(out.trim(), out.as_str());
                let again = clean(out, None);
                prop_assert_eq!(again, first);
            }
        }

        // A leading space is the only thing the preceding character can add.
        #[test]
        fn the_preceding_character_only_adds_one_space(
            raw in "\\PC{0,40}",
            before in proptest::option::of(any::<char>()),
        ) {
            let plain = clean(&raw, None);
            let spaced = clean(&raw, before);
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
