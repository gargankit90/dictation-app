//! Tester-written checks for `dictation_core::text` (M4.2 criterion 2),
//! beyond the Developer's unit tests: never panics on any input, Unicode
//! whitespace, NaN/out-of-range probabilities, and the silence filter's
//! three conditions at the boundary.

use dictation_core::text::{
    Cleaned, Dropped, NO_SPEECH_THRESHOLD, SILENCE_PHRASES, Transcript, clean,
};
use proptest::prelude::*;

fn with_prob(text: &str, p: Option<f64>) -> Transcript {
    Transcript {
        text: text.into(),
        no_speech_prob: p,
    }
}

proptest! {
    #[test]
    fn never_panics_on_any_input(
        raw in any::<String>(),
        prob in proptest::option::of(any::<f64>()),
        before in proptest::option::of(any::<char>()),
    ) {
        let _ = clean(&with_prob(&raw, prob), before);
    }

    #[test]
    fn output_text_is_never_empty_and_never_padded_without_a_preceding_char(
        raw in any::<String>(),
        prob in proptest::option::of(0.0..=1.0f64),
    ) {
        match clean(&with_prob(&raw, prob), None) {
            Cleaned::Text(t) => {
                prop_assert!(!t.is_empty());
                prop_assert_eq!(t.as_str(), t.trim());
            }
            Cleaned::Dropped(_) => {}
        }
    }

    #[test]
    fn unicode_whitespace_padding_never_changes_the_result(
        core in "[a-zA-Z0-9][a-zA-Z0-9 ]{0,20}[a-zA-Z0-9]",
        pad_l in "[ \t\r\n\u{a0}\u{2003}\u{3000}\u{2028}]{0,5}",
        pad_r in "[ \t\r\n\u{a0}\u{2003}\u{3000}\u{2029}]{0,5}",
    ) {
        let padded = format!("{pad_l}{core}{pad_r}");
        prop_assert_eq!(
            clean(&Transcript::from_text(padded), None),
            Cleaned::Text(core)
        );
    }

    #[test]
    fn real_speech_is_never_dropped_for_silence(
        words in "[a-z]{2,8}( [a-z]{2,8}){1,6}",
        p in 0.0..=1.0f64,
    ) {
        prop_assert!(matches!(clean(&with_prob(&words, Some(p)), None), Cleaned::Text(_)));
    }
}

#[test]
fn silence_phrases_drop_only_at_or_above_threshold_and_never_without_a_probability() {
    for phrase in SILENCE_PHRASES {
        for (p, dropped) in [
            (None, false),
            (Some(0.0), false),
            (Some(0.3), false),
            (Some(NO_SPEECH_THRESHOLD - 1e-9), false),
            (Some(NO_SPEECH_THRESHOLD), true),
            (Some(0.95), true),
            (Some(f64::NAN), false),
        ] {
            let got = clean(&with_prob(phrase, p), None);
            let want_drop = matches!(got, Cleaned::Dropped(Dropped::Silence));
            assert_eq!(want_drop, dropped, "{phrase:?} at {p:?}");
        }
    }
}

#[test]
fn real_thank_you_with_low_probability_is_kept_with_padding_trimmed() {
    assert_eq!(
        clean(&with_prob("  Thank you.\n", Some(0.05)), None),
        Cleaned::Text("Thank you.".into())
    );
}

#[test]
fn empty_is_dropped_as_empty_not_silence_whatever_the_probability() {
    for p in [None, Some(0.0), Some(1.0)] {
        assert_eq!(
            clean(&with_prob(" \u{3000}\n", p), None),
            Cleaned::Dropped(Dropped::Empty)
        );
    }
}
