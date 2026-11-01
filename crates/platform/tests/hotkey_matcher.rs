//! Tester (M1.4 run 1): fake OS event streams through the public
//! `HotkeyMatcher`, for the criterion 2 cases the unit tests don't cover.
//! OS-independent, so it runs in the Linux gate and the macOS job.

use dictation_core::hotkey_spec::{Key, Modifier};
use dictation_platform::hotkey::{Hand, Held, HotkeyMatcher, KeyInput};
use dictation_ports::HotkeyEvent::{self, CancelPressed, Pressed, Released};

const NONE: Held = Held::NONE;
const R_ALT: Held = NONE.with(Modifier::Alt, Hand::Right);
const L_ALT: Held = NONE.with(Modifier::Alt, Hand::Left);
const CTRL: Held = NONE.with(Modifier::Ctrl, Hand::Left);
const SHIFT: Held = NONE.with(Modifier::Shift, Hand::Left);
const CTRL_SHIFT: Held = CTRL.with(Modifier::Shift, Hand::Left);

fn modifier(held: Held, down: bool) -> KeyInput {
    KeyInput {
        held,
        key: None,
        down,
    }
}

fn key(held: Held, key: Key, down: bool) -> KeyInput {
    KeyInput {
        held,
        key: Some(key),
        down,
    }
}

#[cfg(test)] // so clippy's allow-unwrap-in-tests covers this helper
fn run(spec: &str, inputs: &[KeyInput]) -> Vec<HotkeyEvent> {
    let mut matcher = HotkeyMatcher::new(spec.parse().unwrap());
    inputs.iter().filter_map(|&i| matcher.on_input(i)).collect()
}

#[test]
fn default_spec_is_right_option() {
    let mut matcher = HotkeyMatcher::new(Default::default());
    assert_eq!(matcher.on_input(modifier(R_ALT, true)), Some(Pressed));
    assert_eq!(matcher.on_input(modifier(NONE, false)), Some(Released));
}

#[test]
fn left_option_alone_and_with_keys_is_ignored() {
    let events = run(
        "RightAlt",
        &[
            modifier(L_ALT, true),
            key(L_ALT, Key::Letter('E'), true),
            key(L_ALT, Key::Letter('E'), false),
            modifier(NONE, false),
        ],
    );
    assert!(events.is_empty());
}

#[test]
fn right_option_released_while_left_option_held() {
    let both = R_ALT.with(Modifier::Alt, Hand::Left);
    let events = run(
        "RightAlt",
        &[
            modifier(R_ALT, true),
            modifier(both, true),
            modifier(L_ALT, false),
            modifier(NONE, false),
        ],
    );
    assert_eq!(events, [Pressed, Released]);
}

#[test]
fn left_option_held_then_right_option_presses() {
    // Extra keys of a modifier the spec names don't block it (the matcher's
    // `satisfies` rule), so Left Option held doesn't stop Right Option.
    let both = L_ALT.with(Modifier::Alt, Hand::Right);
    let events = run(
        "RightAlt",
        &[
            modifier(L_ALT, true),
            modifier(both, true),
            modifier(L_ALT, false),
            modifier(NONE, false),
        ],
    );
    assert_eq!(events, [Pressed, Released]);
}

#[test]
fn modifier_only_repeat_does_not_press_twice() {
    let events = run(
        "RightAlt",
        &[
            modifier(R_ALT, true),
            modifier(R_ALT, true),
            modifier(R_ALT, true),
            modifier(NONE, false),
        ],
    );
    assert_eq!(events, [Pressed, Released]);
}

#[test]
fn chord_with_modifiers_released_shift_first() {
    let events = run(
        "Ctrl+Shift+Space",
        &[
            modifier(CTRL, true),
            modifier(CTRL_SHIFT, true),
            key(CTRL_SHIFT, Key::Space, true),
            modifier(CTRL, false),
            modifier(NONE, false),
            key(NONE, Key::Space, false),
        ],
    );
    assert_eq!(events, [Pressed, Released]);
}

#[test]
fn chord_with_modifiers_released_ctrl_first() {
    let events = run(
        "Ctrl+Shift+Space",
        &[
            modifier(SHIFT, true),
            modifier(CTRL_SHIFT, true),
            key(CTRL_SHIFT, Key::Space, true),
            modifier(SHIFT, false),
            key(SHIFT, Key::Space, false),
            modifier(NONE, false),
        ],
    );
    assert_eq!(events, [Pressed, Released]);
}

#[test]
fn chord_released_key_first_then_modifiers() {
    let events = run(
        "Ctrl+Shift+Space",
        &[
            modifier(CTRL, true),
            modifier(CTRL_SHIFT, true),
            key(CTRL_SHIFT, Key::Space, true),
            key(CTRL_SHIFT, Key::Space, false),
            modifier(SHIFT, false),
            modifier(NONE, false),
        ],
    );
    assert_eq!(events, [Pressed, Released]);
}

#[test]
fn chord_repeats_after_partial_release_do_not_press_again() {
    let events = run(
        "Ctrl+Shift+Space",
        &[
            modifier(CTRL, true),
            modifier(CTRL_SHIFT, true),
            key(CTRL_SHIFT, Key::Space, true),
            key(CTRL_SHIFT, Key::Space, true),
            // Shift up releases; Space keeps auto-repeating.
            modifier(CTRL, false),
            key(CTRL, Key::Space, true),
            // Shift back down while Space still repeats: not a fresh press.
            modifier(CTRL_SHIFT, true),
            key(CTRL_SHIFT, Key::Space, true),
            key(CTRL_SHIFT, Key::Space, false),
            modifier(NONE, false),
        ],
    );
    assert_eq!(events, [Pressed, Released]);
}

#[test]
fn chord_missing_a_modifier_does_not_press() {
    let events = run(
        "Ctrl+Shift+Space",
        &[
            modifier(CTRL, true),
            key(CTRL, Key::Space, true),
            key(CTRL, Key::Space, false),
            modifier(NONE, false),
        ],
    );
    assert!(events.is_empty());
}

#[test]
fn chord_with_an_extra_modifier_does_not_press() {
    let extra = CTRL_SHIFT.with(Modifier::Cmd, Hand::Left);
    let events = run(
        "Ctrl+Shift+Space",
        &[
            modifier(extra, true),
            key(extra, Key::Space, true),
            key(extra, Key::Space, false),
            modifier(NONE, false),
        ],
    );
    assert!(events.is_empty());
}

#[test]
fn escape_while_held_cancels_but_does_not_release() {
    let events = run(
        "Ctrl+Shift+Space",
        &[
            modifier(CTRL_SHIFT, true),
            key(CTRL_SHIFT, Key::Space, true),
            key(CTRL_SHIFT, Key::Escape, true),
            key(CTRL_SHIFT, Key::Escape, false),
            key(CTRL_SHIFT, Key::Space, false),
            modifier(NONE, false),
        ],
    );
    assert_eq!(events, [Pressed, CancelPressed, Released]);
}

/// A small deterministic generator, so the property test needs no new
/// dev-dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

#[test]
fn random_streams_alternate_pressed_and_released() {
    let modifiers = [
        Modifier::Ctrl,
        Modifier::Alt,
        Modifier::Shift,
        Modifier::Cmd,
    ];
    let keys = [Key::Space, Key::Letter('A'), Key::Escape, Key::F(13)];
    for spec in ["RightAlt", "Ctrl+Shift+Space", "F13", "Alt"] {
        for seed in 0..200 {
            let mut rng = Lcg(seed);
            let mut matcher = HotkeyMatcher::new(spec.parse().unwrap());
            // Model of the physical keyboard: 8 modifier keys, 4 keys.
            let mut held_bits = [false; 8];
            let mut active = false;
            for _ in 0..200 {
                let pick = (rng.next() % 12) as usize;
                let down = rng.next().is_multiple_of(2);
                let key_input = if pick < 8 {
                    held_bits[pick] = down;
                    None
                } else {
                    Some(keys[pick - 8])
                };
                let mut held = Held::NONE;
                for (i, &on) in held_bits.iter().enumerate() {
                    if on {
                        let hand = if i % 2 == 0 { Hand::Left } else { Hand::Right };
                        held = held.with(modifiers[i / 2], hand);
                    }
                }
                let input = KeyInput {
                    held,
                    key: key_input,
                    down,
                };
                match matcher.on_input(input) {
                    Some(Pressed) => {
                        assert!(!active, "{spec} seed {seed}: Pressed twice");
                        active = true;
                    }
                    Some(Released) => {
                        assert!(active, "{spec} seed {seed}: Released without Pressed");
                        active = false;
                    }
                    Some(CancelPressed) => {
                        assert_eq!(key_input, Some(Key::Escape), "{spec} seed {seed}");
                        assert!(down);
                    }
                    Some(other) => panic!("{spec} seed {seed}: unexpected {other:?}"),
                    None => {}
                }
            }
        }
    }
}
