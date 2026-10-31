//! [`HotkeySource`] on macOS: a listen-only `CGEventTap` through the
//! `handy-keys` crate (MIT), matched by [`HotkeyMatcher`].
//!
//! The tap sees every key, so nothing here logs or keeps them: only the
//! [`HotkeyEvent`]s the matcher produces leave this module.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use dictation_core::hotkey_spec::{HotkeySpec, Key, Modifier};
use dictation_ports::{HotkeyError, HotkeySink, HotkeySource};
use handy_keys::{KeyEvent, KeyboardListener, Modifiers};

use super::matcher::{Hand, Held, HotkeyMatcher, KeyInput};
use crate::permissions::macos::is_accessibility_trusted;

/// How often the listener thread checks for [`stop`](HotkeySource::stop)
/// while no keys arrive.
const STOP_POLL: Duration = Duration::from_millis(50);

/// The global hotkey on macOS. Needs Accessibility; without it
/// [`start`](HotkeySource::start) fails with
/// [`HotkeyError::PermissionDenied`]. Modifier-only specs keep working under
/// Secure Input (macOS still delivers modifier changes); specs with a key
/// don't.
pub struct MacHotkey {
    spec: HotkeySpec,
    running: Option<Running>,
}

struct Running {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl MacHotkey {
    /// A source for `spec`; it listens only once started.
    #[must_use]
    pub fn new(spec: HotkeySpec) -> Self {
        Self {
            spec,
            running: None,
        }
    }
}

impl HotkeySource for MacHotkey {
    fn start(&mut self, mut sink: HotkeySink) -> Result<(), HotkeyError> {
        if self.running.is_some() {
            return Err(HotkeyError::AlreadyStarted);
        }
        // handy-keys checks too, but its error can't tell this apart from a
        // failed tap without matching its message.
        if !is_accessibility_trusted() {
            return Err(HotkeyError::PermissionDenied);
        }
        let listener = KeyboardListener::new().map_err(|e| match e {
            handy_keys::Error::AccessibilityNotGranted => HotkeyError::PermissionDenied,
            other => HotkeyError::Unavailable(other.to_string()),
        })?;

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let mut matcher = HotkeyMatcher::new(self.spec);
        let thread = thread::Builder::new()
            .name("hotkey".into())
            .spawn(move || {
                while !thread_stop.load(Ordering::SeqCst) {
                    match listener.recv_timeout(STOP_POLL) {
                        Ok(event) => {
                            if let Some(out) = to_input(&event).and_then(|i| matcher.on_input(i)) {
                                sink(out);
                            }
                        }
                        Err(handy_keys::Error::Timeout) => {}
                        Err(_) => break,
                    }
                }
                // Dropping the listener stops and joins the tap thread.
            })
            .map_err(|e| HotkeyError::Unavailable(e.to_string()))?;
        self.running = Some(Running { stop, thread });
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            running.stop.store(true, Ordering::SeqCst);
            // A panic in the sink already ended the thread; nothing to undo.
            let _ = running.thread.join();
        }
    }
}

impl Drop for MacHotkey {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Converts a `handy-keys` event, or `None` for events the matcher doesn't
/// need (Fn, Caps Lock, mouse buttons, keys no spec can name).
fn to_input(event: &KeyEvent) -> Option<KeyInput> {
    let key = match event.key {
        // Fn has no side and no spec can name it.
        None if event.changed_modifier == Some(Modifiers::FN) => return None,
        None => None,
        Some(key) => Some(to_key(key)?),
    };
    Some(KeyInput {
        held: to_held(event.modifiers),
        key,
        down: event.is_key_down,
    })
}

fn to_held(modifiers: Modifiers) -> Held {
    const SIDES: [(Modifiers, Modifier, Hand); 8] = [
        (Modifiers::CTRL_LEFT, Modifier::Ctrl, Hand::Left),
        (Modifiers::CTRL_RIGHT, Modifier::Ctrl, Hand::Right),
        (Modifiers::OPT_LEFT, Modifier::Alt, Hand::Left),
        (Modifiers::OPT_RIGHT, Modifier::Alt, Hand::Right),
        (Modifiers::SHIFT_LEFT, Modifier::Shift, Hand::Left),
        (Modifiers::SHIFT_RIGHT, Modifier::Shift, Hand::Right),
        (Modifiers::CMD_LEFT, Modifier::Cmd, Hand::Left),
        (Modifiers::CMD_RIGHT, Modifier::Cmd, Hand::Right),
    ];
    SIDES
        .iter()
        .filter(|(bit, _, _)| modifiers.contains(*bit))
        .fold(Held::NONE, |held, &(_, m, h)| held.with(m, h))
}

fn to_key(key: handy_keys::Key) -> Option<Key> {
    use handy_keys::Key as K;
    let letter = |c| Some(Key::Letter(c));
    match key {
        K::A => letter('A'),
        K::B => letter('B'),
        K::C => letter('C'),
        K::D => letter('D'),
        K::E => letter('E'),
        K::F => letter('F'),
        K::G => letter('G'),
        K::H => letter('H'),
        K::I => letter('I'),
        K::J => letter('J'),
        K::K => letter('K'),
        K::L => letter('L'),
        K::M => letter('M'),
        K::N => letter('N'),
        K::O => letter('O'),
        K::P => letter('P'),
        K::Q => letter('Q'),
        K::R => letter('R'),
        K::S => letter('S'),
        K::T => letter('T'),
        K::U => letter('U'),
        K::V => letter('V'),
        K::W => letter('W'),
        K::X => letter('X'),
        K::Y => letter('Y'),
        K::Z => letter('Z'),
        K::Num0 => Some(Key::Digit(0)),
        K::Num1 => Some(Key::Digit(1)),
        K::Num2 => Some(Key::Digit(2)),
        K::Num3 => Some(Key::Digit(3)),
        K::Num4 => Some(Key::Digit(4)),
        K::Num5 => Some(Key::Digit(5)),
        K::Num6 => Some(Key::Digit(6)),
        K::Num7 => Some(Key::Digit(7)),
        K::Num8 => Some(Key::Digit(8)),
        K::Num9 => Some(Key::Digit(9)),
        K::F1 => Some(Key::F(1)),
        K::F2 => Some(Key::F(2)),
        K::F3 => Some(Key::F(3)),
        K::F4 => Some(Key::F(4)),
        K::F5 => Some(Key::F(5)),
        K::F6 => Some(Key::F(6)),
        K::F7 => Some(Key::F(7)),
        K::F8 => Some(Key::F(8)),
        K::F9 => Some(Key::F(9)),
        K::F10 => Some(Key::F(10)),
        K::F11 => Some(Key::F(11)),
        K::F12 => Some(Key::F(12)),
        K::F13 => Some(Key::F(13)),
        K::F14 => Some(Key::F(14)),
        K::F15 => Some(Key::F(15)),
        K::F16 => Some(Key::F(16)),
        K::F17 => Some(Key::F(17)),
        K::F18 => Some(Key::F(18)),
        K::F19 => Some(Key::F(19)),
        K::F20 => Some(Key::F(20)),
        K::Space => Some(Key::Space),
        K::Return => Some(Key::Enter),
        K::Tab => Some(Key::Tab),
        K::Delete => Some(Key::Backspace),
        K::Escape => Some(Key::Escape),
        K::UpArrow => Some(Key::Up),
        K::DownArrow => Some(Key::Down),
        K::LeftArrow => Some(Key::Left),
        K::RightArrow => Some(Key::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use dictation_ports::HotkeyEvent::{CancelPressed, Pressed, Released};

    use super::*;

    fn modifier_event(modifiers: Modifiers, changed: Modifiers, down: bool) -> KeyEvent {
        KeyEvent {
            modifiers,
            key: None,
            is_key_down: down,
            changed_modifier: Some(changed),
        }
    }

    fn key_event(modifiers: Modifiers, key: handy_keys::Key, down: bool) -> KeyEvent {
        KeyEvent {
            modifiers,
            key: Some(key),
            is_key_down: down,
            changed_modifier: None,
        }
    }

    /// The fake OS event stream through the same path the listener thread
    /// uses.
    fn run(spec: HotkeySpec, events: &[KeyEvent]) -> Vec<dictation_ports::HotkeyEvent> {
        let mut matcher = HotkeyMatcher::new(spec);
        events
            .iter()
            .filter_map(|e| to_input(e).and_then(|i| matcher.on_input(i)))
            .collect()
    }

    #[test]
    fn right_option_from_handy_keys_events() {
        let r = Modifiers::OPT_RIGHT;
        let events = run(
            HotkeySpec::default(),
            &[
                modifier_event(r, r, true),
                modifier_event(Modifiers::empty(), r, false),
                // Left Option isn't the default hotkey.
                modifier_event(Modifiers::OPT_LEFT, Modifiers::OPT_LEFT, true),
                modifier_event(Modifiers::empty(), Modifiers::OPT_LEFT, false),
                key_event(Modifiers::empty(), handy_keys::Key::Escape, true),
            ],
        );
        assert_eq!(events, [Pressed, Released, CancelPressed]);
    }

    #[test]
    fn fn_caps_lock_and_mouse_are_ignored() {
        assert_eq!(
            to_input(&modifier_event(Modifiers::FN, Modifiers::FN, true)),
            None
        );
        assert_eq!(
            to_input(&key_event(
                Modifiers::empty(),
                handy_keys::Key::CapsLock,
                true
            )),
            None
        );
        assert_eq!(
            to_input(&key_event(
                Modifiers::OPT_RIGHT,
                handy_keys::Key::MouseLeft,
                true
            )),
            None
        );
    }

    #[test]
    fn held_keeps_each_side() {
        let held = to_held(Modifiers::CTRL_LEFT | Modifiers::CMD_RIGHT | Modifiers::FN);
        assert!(held.contains(Modifier::Ctrl, Hand::Left));
        assert!(held.contains(Modifier::Cmd, Hand::Right));
        assert!(!held.contains(Modifier::Cmd, Hand::Left));
        assert!(!held.contains(Modifier::Alt, Hand::Right));
        assert_eq!(
            to_held(Modifiers::OPT),
            Held::NONE
                .with(Modifier::Alt, Hand::Left)
                .with(Modifier::Alt, Hand::Right)
        );
    }

    #[test]
    fn keys_map_to_spec_keys() {
        use handy_keys::Key as K;
        assert_eq!(to_key(K::Q), Some(Key::Letter('Q')));
        assert_eq!(to_key(K::Num7), Some(Key::Digit(7)));
        assert_eq!(to_key(K::F20), Some(Key::F(20)));
        assert_eq!(to_key(K::Return), Some(Key::Enter));
        assert_eq!(to_key(K::Delete), Some(Key::Backspace));
        assert_eq!(to_key(K::LeftArrow), Some(Key::Left));
        assert_eq!(to_key(K::PageUp), None);
    }

    #[test]
    fn stop_without_start_does_nothing() {
        let mut hotkey = MacHotkey::new(HotkeySpec::default());
        hotkey.stop();
        assert!(hotkey.running.is_none());
    }
}
