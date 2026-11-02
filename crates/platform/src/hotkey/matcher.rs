//! Turns a stream of raw key events into [`HotkeyEvent`]s for one
//! [`HotkeySpec`]. Pure and OS-independent, so it's tested on every platform;
//! each OS adapter only converts its events into [`KeyInput`].

use dictation_core::hotkey_spec::{HotkeySpec, Key, Modifier, Side};
use dictation_ports::HotkeyEvent;

/// The left or right key of a [`Modifier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    /// The left one.
    Left,
    /// The right one.
    Right,
}

/// Which side-specific modifier keys are down.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held(u8);

impl Held {
    /// No modifiers down.
    pub const NONE: Self = Self(0);

    const fn bit(modifier: Modifier, hand: Hand) -> u8 {
        // Two bits per modifier (left, right), in declaration order.
        1 << (modifier as u8 * 2 + matches!(hand, Hand::Right) as u8)
    }

    /// `self` plus `modifier` on `hand`.
    #[must_use]
    pub const fn with(self, modifier: Modifier, hand: Hand) -> Self {
        Self(self.0 | Self::bit(modifier, hand))
    }

    /// Whether `modifier` on `hand` is down.
    #[must_use]
    pub fn contains(self, modifier: Modifier, hand: Hand) -> bool {
        self.0 & Self::bit(modifier, hand) != 0
    }

    fn any(self, modifier: Modifier) -> bool {
        self.contains(modifier, Hand::Left) || self.contains(modifier, Hand::Right)
    }

    /// Whether `side` of `modifier` is down; extra keys of the same modifier
    /// don't matter.
    fn satisfies(self, modifier: Modifier, side: Side) -> bool {
        match side {
            Side::Either => self.any(modifier),
            Side::Left => self.contains(modifier, Hand::Left),
            Side::Right => self.contains(modifier, Hand::Right),
        }
    }
}

/// One raw key event, already converted from the OS's form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyInput {
    /// The modifiers down after this event.
    pub held: Held,
    /// The non-modifier key that changed, or `None` if a modifier changed.
    pub key: Option<Key>,
    /// Whether the key (or modifier) went down; key repeats are downs too.
    pub down: bool,
}

/// Matches [`KeyInput`]s against a [`HotkeySpec`].
///
/// - `Pressed` when the event that completes the spec goes down and the held
///   modifiers are exactly the spec's (so `RightAlt` doesn't fire on
///   `Shift+RightAlt`, and typing with a modifier held doesn't fire a
///   modifier-only spec).
/// - `Released` once any part of the spec comes up; modifiers added while it
///   is down don't release it.
/// - `CancelPressed` when Escape goes down, whatever else is held.
/// - Key repeats never produce a second `Pressed` or `CancelPressed`.
#[derive(Debug, Clone)]
pub struct HotkeyMatcher {
    spec: HotkeySpec,
    active: bool,
    key_held: bool,
    escape_held: bool,
}

impl HotkeyMatcher {
    /// A matcher for `spec`, with nothing held.
    #[must_use]
    pub fn new(spec: HotkeySpec) -> Self {
        Self {
            spec,
            active: false,
            key_held: false,
            escape_held: false,
        }
    }

    /// Forgets every key it thought was down, as when it was new. For when
    /// the source may have missed events (a [`HotkeyEvent::Interrupted`]).
    pub fn reset(&mut self) {
        *self = Self::new(self.spec);
    }

    /// The event `input` produces, if any.
    pub fn on_input(&mut self, input: KeyInput) -> Option<HotkeyEvent> {
        let mut completes = false;
        if let Some(key) = input.key {
            if key == Key::Escape {
                let first_down = input.down && !self.escape_held;
                self.escape_held = input.down;
                if first_down {
                    return Some(HotkeyEvent::CancelPressed);
                }
                // The rest of Esc's events (its repeats and its release) fall
                // through to the check below, so a hotkey whose release was
                // missed doesn't stay "held" after Esc cancelled it (#47):
                // the modifiers they carry are the reconciled ones.
            } else if Some(key) == self.spec.key() {
                completes = input.down && !self.key_held;
                self.key_held = input.down;
            }
        } else if self.spec.key().is_none() {
            completes = input.down;
        }
        self.settle(input.held, completes)
    }

    /// The event a key no spec can name (Page Up, Caps Lock, a mouse
    /// button) produces, if any, with `held` the modifiers it reports. It
    /// never presses the hotkey, but it can release one whose release was
    /// missed: the OS reports the real modifiers with every key (#47).
    pub fn on_unmapped(&mut self, held: Held) -> Option<HotkeyEvent> {
        self.settle(held, false)
    }

    /// Releases the hotkey if `held` (and the key) no longer make it up, or
    /// presses it if this event `completes` it and `held` is exactly it.
    fn settle(&mut self, held: Held, completes: bool) -> Option<HotkeyEvent> {
        let key_ok = self.spec.key().is_none() || self.key_held;
        let mut required = true;
        let mut exact = true;
        for modifier in Modifier::ALL {
            match self.spec.modifier(modifier) {
                Some(side) => required &= held.satisfies(modifier, side),
                None => exact &= !held.any(modifier),
            }
        }

        if self.active {
            if !(required && key_ok) {
                self.active = false;
                return Some(HotkeyEvent::Released);
            }
        } else if completes && required && exact && key_ok {
            self.active = true;
            return Some(HotkeyEvent::Pressed);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use HotkeyEvent::{CancelPressed, Pressed, Released};

    const RIGHT_ALT: Held = Held::NONE.with(Modifier::Alt, Hand::Right);

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

    /// Feeds `inputs` to a matcher for `spec` and collects what comes out.
    fn run(spec: &str, inputs: &[KeyInput]) -> Vec<HotkeyEvent> {
        let mut matcher = HotkeyMatcher::new(spec.parse().unwrap());
        inputs.iter().filter_map(|&i| matcher.on_input(i)).collect()
    }

    #[test]
    fn every_modifier_key_has_its_own_bit() {
        let mut all = Held::NONE;
        for m in Modifier::ALL {
            for h in [Hand::Left, Hand::Right] {
                let one = Held::NONE.with(m, h);
                assert!(one.contains(m, h));
                assert!(!all.contains(m, h), "{m:?} {h:?} overlaps");
                all = all.with(m, h);
            }
        }
        assert_eq!(all, Held(u8::MAX));
    }

    #[test]
    fn adding_a_key_that_is_already_held_keeps_it_held() {
        let twice = RIGHT_ALT.with(Modifier::Alt, Hand::Right);
        assert_eq!(twice, RIGHT_ALT);
        assert!(twice.contains(Modifier::Alt, Hand::Right));
    }

    #[test]
    fn right_option_press_and_release() {
        let events = run(
            "RightAlt",
            &[modifier(RIGHT_ALT, true), modifier(Held::NONE, false)],
        );
        assert_eq!(events, [Pressed, Released]);
    }

    #[test]
    fn left_option_does_not_trigger_right_option() {
        let left = Held::NONE.with(Modifier::Alt, Hand::Left);
        let events = run(
            "RightAlt",
            &[modifier(left, true), modifier(Held::NONE, false)],
        );
        assert!(events.is_empty());
    }

    #[test]
    fn either_side_matches_both() {
        for hand in [Hand::Left, Hand::Right] {
            let held = Held::NONE.with(Modifier::Alt, hand);
            let events = run("Alt", &[modifier(held, true), modifier(Held::NONE, false)]);
            assert_eq!(events, [Pressed, Released], "{hand:?}");
        }
    }

    #[test]
    fn extra_modifier_before_the_hotkey_blocks_it() {
        let shift = Held::NONE.with(Modifier::Shift, Hand::Left);
        let both = shift.with(Modifier::Alt, Hand::Right);
        let events = run(
            "RightAlt",
            &[
                modifier(shift, true),
                modifier(both, true),
                // Letting go of Shift leaves RightAlt alone, but that's an
                // up event, so it doesn't start dictation.
                modifier(RIGHT_ALT, false),
                // Nor does a key typed while RightAlt is still down.
                key(RIGHT_ALT, Key::Letter('E'), true),
                key(RIGHT_ALT, Key::Letter('E'), false),
                modifier(Held::NONE, false),
            ],
        );
        assert!(events.is_empty());
    }

    #[test]
    fn extra_modifier_while_held_does_not_release() {
        let both = RIGHT_ALT.with(Modifier::Shift, Hand::Left);
        let events = run(
            "RightAlt",
            &[
                modifier(RIGHT_ALT, true),
                modifier(both, true),
                modifier(RIGHT_ALT, false),
                key(RIGHT_ALT, Key::Letter('E'), true),
                modifier(Held::NONE, false),
            ],
        );
        assert_eq!(events, [Pressed, Released]);
    }

    #[test]
    fn releasing_the_hotkey_while_another_modifier_is_held_releases() {
        let shift = Held::NONE.with(Modifier::Shift, Hand::Left);
        let both = RIGHT_ALT.with(Modifier::Shift, Hand::Left);
        let events = run(
            "RightAlt",
            &[
                modifier(RIGHT_ALT, true),
                modifier(both, true),
                modifier(shift, false),
                modifier(Held::NONE, false),
            ],
        );
        assert_eq!(events, [Pressed, Released]);
    }

    #[test]
    fn modifiers_and_key() {
        let ctrl = Held::NONE.with(Modifier::Ctrl, Hand::Left);
        let ctrl_shift = ctrl.with(Modifier::Shift, Hand::Right);
        let events = run(
            "Ctrl+Shift+Space",
            &[
                modifier(ctrl, true),
                modifier(ctrl_shift, true),
                key(ctrl_shift, Key::Space, true),
                // Auto-repeat while held.
                key(ctrl_shift, Key::Space, true),
                key(ctrl_shift, Key::Space, true),
                key(ctrl_shift, Key::Space, false),
                modifier(ctrl, false),
                modifier(Held::NONE, false),
            ],
        );
        assert_eq!(events, [Pressed, Released]);
    }

    #[test]
    fn releasing_a_modifier_of_a_key_spec_releases() {
        let ctrl = Held::NONE.with(Modifier::Ctrl, Hand::Right);
        let events = run(
            "Ctrl+Space",
            &[
                key(ctrl, Key::Space, true),
                key(Held::NONE, Key::Space, true),
                key(Held::NONE, Key::Space, false),
            ],
        );
        assert_eq!(events, [Pressed, Released]);
    }

    #[test]
    fn key_down_before_modifiers_does_not_press_later() {
        let ctrl = Held::NONE.with(Modifier::Ctrl, Hand::Left);
        let events = run(
            "Ctrl+Space",
            &[
                key(Held::NONE, Key::Space, true),
                modifier(ctrl, true),
                // A repeat of the held key isn't a new press.
                key(ctrl, Key::Space, true),
                key(ctrl, Key::Space, false),
                // A fresh press is.
                key(ctrl, Key::Space, true),
                key(ctrl, Key::Space, false),
            ],
        );
        assert_eq!(events, [Pressed, Released]);
    }

    #[test]
    fn other_keys_are_ignored() {
        let ctrl = Held::NONE.with(Modifier::Ctrl, Hand::Left);
        let events = run(
            "Ctrl+Space",
            &[
                key(ctrl, Key::Letter('A'), true),
                key(ctrl, Key::Letter('A'), false),
                key(Held::NONE, Key::Space, true),
                key(Held::NONE, Key::Space, false),
            ],
        );
        assert!(events.is_empty());
    }

    #[test]
    fn function_key_alone() {
        let events = run(
            "F13",
            &[
                key(Held::NONE, Key::F(13), true),
                key(Held::NONE, Key::F(13), false),
            ],
        );
        assert_eq!(events, [Pressed, Released]);
    }

    #[test]
    fn escape_cancels_once_per_press_whatever_is_held() {
        let events = run(
            "RightAlt",
            &[
                key(Held::NONE, Key::Escape, true),
                key(Held::NONE, Key::Escape, true),
                key(Held::NONE, Key::Escape, false),
                modifier(RIGHT_ALT, true),
                key(RIGHT_ALT, Key::Escape, true),
                key(RIGHT_ALT, Key::Escape, false),
                modifier(Held::NONE, false),
            ],
        );
        assert_eq!(events, [CancelPressed, Pressed, CancelPressed, Released]);
    }

    /// #47: macOS drops a modifier release (an event-tap timeout, Mission
    /// Control) and the user taps Esc to cancel. The Esc events carry the
    /// real modifiers, so its release lets the hotkey go, and the next press
    /// of the hotkey works at once.
    #[test]
    fn esc_after_a_missed_modifier_release_clears_the_stuck_state() {
        let events = run(
            "RightAlt",
            &[
                modifier(RIGHT_ALT, true),
                // The modifier's release never arrives; Esc cancels.
                key(Held::NONE, Key::Escape, true),
                key(Held::NONE, Key::Escape, false),
                // A fresh press and release must both be seen.
                modifier(RIGHT_ALT, true),
                modifier(Held::NONE, false),
            ],
        );
        assert_eq!(
            events,
            [Pressed, CancelPressed, Released, Pressed, Released]
        );
    }

    /// #47, the second case: the modifier's release is missed and the next
    /// key is one no spec can name (Page Up, Caps Lock, a mouse button). Its
    /// modifiers let the hotkey go, and the next press works at once.
    #[test]
    fn an_unmapped_key_after_a_missed_modifier_release_clears_the_stuck_state() {
        let mut matcher = HotkeyMatcher::new("RightAlt".parse().unwrap());
        assert_eq!(matcher.on_input(modifier(RIGHT_ALT, true)), Some(Pressed));
        // The release never arrives; Page Up goes down with nothing held.
        assert_eq!(matcher.on_unmapped(Held::NONE), Some(Released));
        assert_eq!(matcher.on_unmapped(Held::NONE), None);
        assert_eq!(matcher.on_input(modifier(RIGHT_ALT, true)), Some(Pressed));
        assert_eq!(
            matcher.on_input(modifier(Held::NONE, false)),
            Some(Released)
        );
    }

    #[test]
    fn an_unmapped_key_never_presses_and_keeps_a_real_hold() {
        let mut matcher = HotkeyMatcher::new("RightAlt".parse().unwrap());
        // RightAlt alone is held when Page Up goes down: not a press.
        assert_eq!(matcher.on_unmapped(RIGHT_ALT), None);
        assert_eq!(matcher.on_input(modifier(Held::NONE, false)), None);
        // While the hotkey is really held, an unmapped key doesn't end it.
        assert_eq!(matcher.on_input(modifier(RIGHT_ALT, true)), Some(Pressed));
        assert_eq!(matcher.on_unmapped(RIGHT_ALT), None);
        assert_eq!(
            matcher.on_input(modifier(Held::NONE, false)),
            Some(Released)
        );
    }

    #[test]
    fn esc_while_the_hotkey_is_really_held_does_not_release_it() {
        let events = run(
            "RightAlt",
            &[
                modifier(RIGHT_ALT, true),
                key(RIGHT_ALT, Key::Escape, true),
                // A repeat and the release of Esc, with the hotkey still down.
                key(RIGHT_ALT, Key::Escape, true),
                key(RIGHT_ALT, Key::Escape, false),
                modifier(Held::NONE, false),
            ],
        );
        assert_eq!(events, [Pressed, CancelPressed, Released]);
    }

    #[test]
    fn reset_forgets_a_hold_and_its_keys() {
        let mut matcher = HotkeyMatcher::new("Ctrl+Space".parse().unwrap());
        let ctrl = Held::NONE.with(Modifier::Ctrl, Hand::Left);
        assert_eq!(matcher.on_input(key(ctrl, Key::Space, true)), Some(Pressed));
        assert_eq!(
            matcher.on_input(key(Held::NONE, Key::Escape, true)),
            Some(CancelPressed)
        );
        matcher.reset();
        // Its release, if it ever comes, isn't reported a second time...
        assert_eq!(matcher.on_input(key(Held::NONE, Key::Space, false)), None);
        // ...and Esc and the hotkey count as fresh presses.
        assert_eq!(
            matcher.on_input(key(Held::NONE, Key::Escape, true)),
            Some(CancelPressed)
        );
        assert_eq!(matcher.on_input(key(ctrl, Key::Space, true)), Some(Pressed));
    }

    #[test]
    fn press_again_after_release() {
        let mut inputs = Vec::new();
        for _ in 0..3 {
            inputs.push(modifier(RIGHT_ALT, true));
            inputs.push(modifier(Held::NONE, false));
        }
        assert_eq!(
            run("RightAlt", &inputs),
            [Pressed, Released, Pressed, Released, Pressed, Released]
        );
    }
}
