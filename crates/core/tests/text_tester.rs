//! Tester-written checks for `dictation_core::text` (M4.2 criterion 2),
//! beyond the Developer's unit tests: never panics on any input, and
//! Unicode whitespace.

use dictation_core::text::{Cleaned, Dropped, clean};
use proptest::prelude::*;

proptest! {
    #[test]
    fn never_panics_on_any_input(
        raw in any::<String>(),
        before in proptest::option::of(any::<char>()),
    ) {
        let _ = clean(&raw, before);
    }

    #[test]
    fn output_text_is_never_empty_and_never_padded_without_a_preceding_char(
        raw in any::<String>(),
    ) {
        match clean(&raw, None) {
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
            clean(&padded, None),
            Cleaned::Text(core)
        );
    }
}

#[test]
fn blank_is_dropped_as_empty() {
    assert_eq!(clean(" \u{3000}\n", None), Cleaned::Dropped(Dropped::Empty));
}
