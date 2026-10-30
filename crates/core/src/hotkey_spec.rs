//! The dictation hotkey as the user writes it: `RightAlt`, `Ctrl+Shift+Space`.
//!
//! A spec is up to one entry per modifier (each either side, left or right)
//! plus at most one key, which must come last. Names are case-insensitive;
//! [`Display`](fmt::Display) writes the canonical form (modifiers in the
//! order Ctrl, Alt, Shift, Cmd, then the key), which parses back to the same
//! spec.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// A modifier key, either side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    /// Control.
    Ctrl,
    /// Option on macOS, Alt on Windows.
    Alt,
    /// Shift.
    Shift,
    /// Command on macOS, Windows key on Windows.
    Cmd,
}

impl Modifier {
    /// Every modifier, in canonical order.
    pub const ALL: [Self; 4] = [Self::Ctrl, Self::Alt, Self::Shift, Self::Cmd];

    fn index(self) -> usize {
        self as usize
    }

    fn name(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Cmd => "Cmd",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name {
            "ctrl" | "control" => Some(Self::Ctrl),
            "alt" | "option" | "opt" => Some(Self::Alt),
            "shift" => Some(Self::Shift),
            "cmd" | "command" | "meta" | "super" | "win" => Some(Self::Cmd),
            _ => None,
        }
    }
}

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Which physical key of a [`Modifier`] a spec asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// Either the left or the right one.
    Either,
    /// Only the left one.
    Left,
    /// Only the right one.
    Right,
}

/// A non-modifier key a spec can end with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A letter, `'A'..='Z'`.
    Letter(char),
    /// A digit on the main row, `0..=9`.
    Digit(u8),
    /// A function key, `1..=20`.
    F(u8),
    /// Space bar.
    Space,
    /// Return / Enter.
    Enter,
    /// Tab.
    Tab,
    /// Backspace (Delete on a Mac keyboard).
    Backspace,
    /// Escape. Reserved for cancelling, so no spec may use it.
    Escape,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
}

impl Key {
    /// Whether pressing this key alone (or with Shift) types something, so
    /// it can't be a hotkey without another modifier.
    fn types_text(self) -> bool {
        matches!(
            self,
            Self::Letter(_)
                | Self::Digit(_)
                | Self::Space
                | Self::Enter
                | Self::Tab
                | Self::Backspace
        )
    }

    fn parse(name: &str) -> Option<Self> {
        let key = match name {
            "space" => Self::Space,
            "enter" | "return" => Self::Enter,
            "tab" => Self::Tab,
            "backspace" => Self::Backspace,
            "escape" | "esc" => Self::Escape,
            "up" => Self::Up,
            "down" => Self::Down,
            "left" => Self::Left,
            "right" => Self::Right,
            _ => {
                let mut chars = name.chars();
                return match (chars.next(), chars.as_str()) {
                    (Some(c @ 'a'..='z'), "") => Some(Self::Letter(c.to_ascii_uppercase())),
                    (Some(c @ '0'..='9'), "") => Some(Self::Digit(c as u8 - b'0')),
                    (Some('f'), n) if !n.starts_with('0') => match n.parse::<u8>() {
                        Ok(n @ 1..=20) => Some(Self::F(n)),
                        _ => None,
                    },
                    _ => None,
                };
            }
        };
        Some(key)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Letter(c) => write!(f, "{c}"),
            Self::Digit(d) => write!(f, "{d}"),
            Self::F(n) => write!(f, "F{n}"),
            Self::Space => f.write_str("Space"),
            Self::Enter => f.write_str("Enter"),
            Self::Tab => f.write_str("Tab"),
            Self::Backspace => f.write_str("Backspace"),
            Self::Escape => f.write_str("Escape"),
            Self::Up => f.write_str("Up"),
            Self::Down => f.write_str("Down"),
            Self::Left => f.write_str("Left"),
            Self::Right => f.write_str("Right"),
        }
    }
}

/// Why a hotkey string isn't a valid [`HotkeySpec`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HotkeySpecError {
    /// Nothing between two `+`, or an empty string.
    #[error("the hotkey has an empty part")]
    EmptyPart,
    /// A name that is neither a modifier nor a key.
    #[error("unknown key name: {0}")]
    UnknownName(String),
    /// The same modifier twice, including both sides of it
    /// (`LeftAlt+RightAlt`) or a side and either side (`Ctrl+LeftCtrl`).
    #[error("{0} appears more than once")]
    DuplicateModifier(Modifier),
    /// A second key, or a modifier after the key.
    #[error("the key must come last and only once")]
    KeyNotLast,
    /// Escape cancels dictation, so it can't start it.
    #[error("Escape is reserved for cancelling")]
    ReservedKey,
    /// A key that types text (`A`, `Shift+Space`) would fire while typing.
    #[error("{0} needs Ctrl, Alt or Cmd")]
    NeedsModifier(Key),
}

/// A validated dictation hotkey; [`Default`] is modifier-only Right Option
/// (`RightAlt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HotkeySpec {
    modifiers: [Option<Side>; 4],
    key: Option<Key>,
}

impl Default for HotkeySpec {
    fn default() -> Self {
        let mut modifiers = [None; 4];
        modifiers[Modifier::Alt.index()] = Some(Side::Right);
        Self {
            modifiers,
            key: None,
        }
    }
}

impl HotkeySpec {
    /// Which side of `modifier` the spec needs, or `None` if it must not be
    /// held.
    #[must_use]
    pub fn modifier(&self, modifier: Modifier) -> Option<Side> {
        self.modifiers[modifier.index()]
    }

    /// The key after the modifiers, or `None` for a modifier-only spec.
    #[must_use]
    pub fn key(&self) -> Option<Key> {
        self.key
    }
}

/// Splits `rightalt` into (`Alt`, `Right`).
fn parse_modifier(name: &str) -> Option<(Modifier, Side)> {
    if let Some(rest) = name.strip_prefix("left") {
        Modifier::parse(rest).map(|m| (m, Side::Left))
    } else if let Some(rest) = name.strip_prefix("right") {
        Modifier::parse(rest).map(|m| (m, Side::Right))
    } else {
        Modifier::parse(name).map(|m| (m, Side::Either))
    }
}

impl FromStr for HotkeySpec {
    type Err = HotkeySpecError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut modifiers = [None; 4];
        let mut key = None;
        for part in s.split('+') {
            let name = part.trim().to_ascii_lowercase();
            if name.is_empty() {
                return Err(HotkeySpecError::EmptyPart);
            }
            // "Left" and "Right" alone are arrow keys, not sides.
            if let Some((modifier, side)) = parse_modifier(&name) {
                if key.is_some() {
                    return Err(HotkeySpecError::KeyNotLast);
                }
                let slot = &mut modifiers[modifier.index()];
                if slot.is_some() {
                    return Err(HotkeySpecError::DuplicateModifier(modifier));
                }
                *slot = Some(side);
            } else if let Some(k) = Key::parse(&name) {
                if key.is_some() {
                    return Err(HotkeySpecError::KeyNotLast);
                }
                key = Some(k);
            } else {
                return Err(HotkeySpecError::UnknownName(part.trim().to_owned()));
            }
        }
        if let Some(k) = key {
            if k == Key::Escape {
                return Err(HotkeySpecError::ReservedKey);
            }
            let only_shift = Modifier::ALL
                .iter()
                .all(|&m| m == Modifier::Shift || modifiers[m.index()].is_none());
            if k.types_text() && only_shift {
                return Err(HotkeySpecError::NeedsModifier(k));
            }
        }
        Ok(Self { modifiers, key })
    }
}

impl fmt::Display for HotkeySpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut sep = "";
        for modifier in Modifier::ALL {
            let prefix = match self.modifier(modifier) {
                None => continue,
                Some(Side::Either) => "",
                Some(Side::Left) => "Left",
                Some(Side::Right) => "Right",
            };
            write!(f, "{sep}{prefix}{modifier}")?;
            sep = "+";
        }
        if let Some(key) = self.key {
            write!(f, "{sep}{key}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn parse(s: &str) -> Result<HotkeySpec, HotkeySpecError> {
        s.parse()
    }

    #[test]
    fn default_is_right_option_alone() {
        let spec = HotkeySpec::default();
        assert_eq!(spec.modifier(Modifier::Alt), Some(Side::Right));
        assert_eq!(spec.modifier(Modifier::Ctrl), None);
        assert_eq!(spec.key(), None);
        assert_eq!(spec.to_string(), "RightAlt");
        assert_eq!(parse("RightAlt"), Ok(spec));
    }

    #[test]
    fn parses_modifiers_and_key() {
        let spec = parse("Ctrl+Shift+Space").unwrap();
        assert_eq!(spec.modifier(Modifier::Ctrl), Some(Side::Either));
        assert_eq!(spec.modifier(Modifier::Shift), Some(Side::Either));
        assert_eq!(spec.modifier(Modifier::Alt), None);
        assert_eq!(spec.modifier(Modifier::Cmd), None);
        assert_eq!(spec.key(), Some(Key::Space));
    }

    #[test]
    fn formats_canonically() {
        let cases = [
            ("RightAlt", "RightAlt"),
            ("RIGHTALT", "RightAlt"),
            ("rightoption", "RightAlt"),
            ("shift + ctrl + space", "Ctrl+Shift+Space"),
            ("Command+Option+k", "Alt+Cmd+K"),
            ("LeftCtrl+f5", "LeftCtrl+F5"),
            ("F13", "F13"),
            ("Cmd+Shift", "Shift+Cmd"),
            ("Ctrl+Right", "Ctrl+Right"),
            ("Alt+return", "Alt+Enter"),
            ("Meta+7", "Cmd+7"),
            ("Ctrl+Tab", "Ctrl+Tab"),
            ("Alt+Backspace", "Alt+Backspace"),
            ("Ctrl+Up", "Ctrl+Up"),
            ("Ctrl+Down", "Ctrl+Down"),
            ("Ctrl+Left", "Ctrl+Left"),
            ("Shift+F1", "Shift+F1"),
        ];
        for (input, canonical) in cases {
            let spec = parse(input).unwrap_or_else(|e| panic!("{input}: {e}"));
            assert_eq!(spec.to_string(), canonical, "{input}");
            assert_eq!(parse(canonical), Ok(spec), "{canonical} round trip");
        }
    }

    #[test]
    fn rejects_invalid_combinations() {
        use HotkeySpecError::*;
        let cases = [
            ("", EmptyPart),
            ("Ctrl+", EmptyPart),
            ("+Space", EmptyPart),
            ("Ctrl++Space", EmptyPart),
            ("Hyper", UnknownName("Hyper".into())),
            ("Ctrl+Banana", UnknownName("Banana".into())),
            ("F0", UnknownName("F0".into())),
            ("F21", UnknownName("F21".into())),
            ("F01", UnknownName("F01".into())),
            ("Ctrl+AB", UnknownName("AB".into())),
            ("RightSpace", UnknownName("RightSpace".into())),
            ("Ctrl+Ctrl+A", DuplicateModifier(Modifier::Ctrl)),
            ("Ctrl+Control+A", DuplicateModifier(Modifier::Ctrl)),
            ("LeftAlt+RightAlt", DuplicateModifier(Modifier::Alt)),
            ("Shift+LeftShift", DuplicateModifier(Modifier::Shift)),
            ("Ctrl+A+B", KeyNotLast),
            ("Space+Ctrl", KeyNotLast),
            ("Escape", ReservedKey),
            ("Ctrl+Esc", ReservedKey),
            ("A", NeedsModifier(Key::Letter('A'))),
            ("Space", NeedsModifier(Key::Space)),
            ("Shift+Space", NeedsModifier(Key::Space)),
            ("Shift+1", NeedsModifier(Key::Digit(1))),
            ("Enter", NeedsModifier(Key::Enter)),
        ];
        for (input, error) in cases {
            assert_eq!(parse(input), Err(error), "{input:?}");
        }
    }

    #[test]
    fn errors_name_the_problem() {
        assert_eq!(
            parse("Ctrl+Ctrl+A").unwrap_err().to_string(),
            "Ctrl appears more than once"
        );
        assert_eq!(
            parse("Shift+A").unwrap_err().to_string(),
            "A needs Ctrl, Alt or Cmd"
        );
        assert_eq!(
            parse("Nope").unwrap_err().to_string(),
            "unknown key name: Nope"
        );
    }

    fn side() -> impl Strategy<Value = Option<Side>> {
        prop_oneof![
            Just(None),
            Just(Some(Side::Either)),
            Just(Some(Side::Left)),
            Just(Some(Side::Right)),
        ]
    }

    fn key() -> impl Strategy<Value = Option<Key>> {
        prop_oneof![
            Just(None),
            (b'A'..=b'Z').prop_map(|c| Some(Key::Letter(char::from(c)))),
            (0u8..=9).prop_map(|d| Some(Key::Digit(d))),
            (1u8..=20).prop_map(|n| Some(Key::F(n))),
            prop::sample::select(vec![
                Key::Space,
                Key::Enter,
                Key::Tab,
                Key::Backspace,
                Key::Escape,
                Key::Up,
                Key::Down,
                Key::Left,
                Key::Right,
            ])
            .prop_map(Some),
        ]
    }

    proptest! {
        // Every spec the parser accepts formats to a string that parses back
        // to the same spec, and that string is a fixed point.
        #[test]
        fn round_trips(ctrl in side(), alt in side(), shift in side(), cmd in side(), key in key()) {
            let spec = HotkeySpec { modifiers: [ctrl, alt, shift, cmd], key };
            let text = spec.to_string();
            match parse(&text) {
                Ok(parsed) => {
                    prop_assert_eq!(parsed, spec);
                    prop_assert_eq!(parsed.to_string(), text);
                }
                // Only the combinations validation forbids may fail.
                Err(HotkeySpecError::EmptyPart) => prop_assert!(spec.modifiers.iter().all(Option::is_none) && key.is_none()),
                Err(HotkeySpecError::ReservedKey) => prop_assert_eq!(key, Some(Key::Escape)),
                Err(HotkeySpecError::NeedsModifier(k)) => prop_assert_eq!(Some(k), key),
                Err(e) => prop_assert!(false, "{text}: {e}"),
            }
        }
    }
}
