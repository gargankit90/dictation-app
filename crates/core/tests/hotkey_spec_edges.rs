//! Tester (M1.4 run 1): `hotkey_spec` cases the unit tests don't cover:
//! whitespace, case, left/right forms and the side/either conflicts.

use dictation_core::hotkey_spec::{HotkeySpec, HotkeySpecError, Key, Modifier, Side};
use proptest::prelude::*;

fn parse(s: &str) -> Result<HotkeySpec, HotkeySpecError> {
    s.parse()
}

#[test]
fn whitespace_only_is_an_empty_part() {
    for input in [" ", "\t", "  +  ", "Ctrl+ ", " +Space"] {
        assert_eq!(parse(input), Err(HotkeySpecError::EmptyPart), "{input:?}");
    }
}

#[test]
fn whitespace_around_parts_is_trimmed() {
    for input in ["  RightAlt  ", "\tRightAlt\n", " Ctrl + Shift + Space "] {
        let spec = parse(input).unwrap_or_else(|e| panic!("{input:?}: {e}"));
        assert_eq!(parse(&spec.to_string()), Ok(spec), "{input:?}");
    }
}

#[test]
fn whitespace_inside_a_name_is_unknown() {
    assert_eq!(
        parse("Right Alt"),
        Err(HotkeySpecError::UnknownName("Right Alt".into()))
    );
    assert_eq!(
        parse("Ctrl+Sp ace"),
        Err(HotkeySpecError::UnknownName("Sp ace".into()))
    );
}

#[test]
fn names_are_case_insensitive() {
    for input in [
        "rightalt",
        "RIGHTALT",
        "rIgHtAlT",
        "RightOption",
        "rightopt",
    ] {
        assert_eq!(parse(input), Ok(HotkeySpec::default()), "{input:?}");
    }
    assert_eq!(parse("ctrl+SHIFT+space"), parse("Ctrl+Shift+Space"));
    assert_eq!(parse("cmd+f5"), parse("CMD+F5"));
}

#[test]
fn unknown_name_keeps_the_users_spelling() {
    assert_eq!(
        parse("ctrl+HyPeR"),
        Err(HotkeySpecError::UnknownName("HyPeR".into()))
    );
}

#[test]
fn left_and_right_sides_of_every_modifier() {
    for (name, modifier) in [
        ("Ctrl", Modifier::Ctrl),
        ("Alt", Modifier::Alt),
        ("Shift", Modifier::Shift),
        ("Cmd", Modifier::Cmd),
    ] {
        for (prefix, side) in [
            ("", Side::Either),
            ("Left", Side::Left),
            ("Right", Side::Right),
        ] {
            let text = format!("{prefix}{name}");
            let spec = parse(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert_eq!(spec.modifier(modifier), Some(side), "{text}");
            assert_eq!(spec.key(), None, "{text}");
            assert_eq!(spec.to_string(), text);
        }
    }
}

#[test]
fn left_right_conflicts_in_either_order() {
    for (input, modifier) in [
        ("RightAlt+LeftAlt", Modifier::Alt),
        ("LeftCtrl+Ctrl", Modifier::Ctrl),
        ("Ctrl+RightCtrl", Modifier::Ctrl),
        ("RightCmd+RightCmd", Modifier::Cmd),
        ("LeftShift+RightShift", Modifier::Shift),
        ("RightOption+Alt", Modifier::Alt),
        ("Command+LeftMeta", Modifier::Cmd),
    ] {
        assert_eq!(
            parse(input),
            Err(HotkeySpecError::DuplicateModifier(modifier)),
            "{input}"
        );
    }
}

#[test]
fn bare_left_and_right_are_arrow_keys() {
    assert_eq!(parse("Ctrl+Left").unwrap().key(), Some(Key::Left));
    assert_eq!(parse("Ctrl+Right").unwrap().key(), Some(Key::Right));
    // An arrow can't be a side prefix of another key.
    assert_eq!(
        parse("LeftLeft"),
        Err(HotkeySpecError::UnknownName("LeftLeft".into()))
    );
}

#[test]
fn no_key_and_no_modifier_is_rejected() {
    // Only `+` separators: every part is empty.
    assert_eq!(parse("+"), Err(HotkeySpecError::EmptyPart));
    assert_eq!(parse("++"), Err(HotkeySpecError::EmptyPart));
}

#[test]
fn modifier_after_a_key_is_rejected_even_when_distinct() {
    assert_eq!(parse("Ctrl+A+Shift"), Err(HotkeySpecError::KeyNotLast));
    assert_eq!(parse("F5+RightAlt"), Err(HotkeySpecError::KeyNotLast));
}

#[test]
fn text_keys_need_a_non_shift_modifier() {
    for input in ["Tab", "Backspace", "Shift+Enter", "Shift+Z", "LeftShift+0"] {
        assert!(
            matches!(parse(input), Err(HotkeySpecError::NeedsModifier(_))),
            "{input}"
        );
    }
    // Non-text keys may stand alone or with Shift only.
    for input in ["F1", "Shift+F20", "Up", "Shift+Down"] {
        assert!(parse(input).is_ok(), "{input}");
    }
}

proptest! {
    // Any input either fails to parse or formats to a canonical string that
    // parses back to the same spec; parsing never panics.
    #[test]
    fn arbitrary_strings_never_panic_and_round_trip(s in "[A-Za-z0-9+ \\t]{0,24}") {
        if let Ok(spec) = parse(&s) {
            let text = spec.to_string();
            prop_assert_eq!(parse(&text), Ok(spec));
        }
    }

    // Random casing and padding of a valid canonical spec parse the same.
    #[test]
    fn case_and_padding_do_not_matter(
        canonical in prop::sample::select(vec![
            "RightAlt", "Ctrl+Shift+Space", "LeftCtrl+F5", "Alt+Cmd+K", "Shift+Cmd", "F13",
        ]),
        flips in prop::collection::vec(any::<bool>(), 32),
        pad in " {0,2}",
    ) {
        let mangled: String = canonical
            .split('+')
            .map(|part| {
                let cased: String = part
                    .chars()
                    .zip(flips.iter().cycle())
                    .map(|(c, &up)| if up { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() })
                    .collect();
                format!("{pad}{cased}{pad}")
            })
            .collect::<Vec<_>>()
            .join("+");
        prop_assert_eq!(parse(&mangled), parse(canonical));
    }
}
