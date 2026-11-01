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
use dictation_ports::{HotkeyError, HotkeyEvent, HotkeySink, HotkeySource};
use handy_keys::{KeyEvent, KeyboardListener, Modifiers};

use super::matcher::{Hand, Held, HotkeyMatcher, KeyInput};
use crate::permissions::macos::is_accessibility_trusted;

/// How often the listener thread checks for [`stop`](HotkeySource::stop)
/// while no keys arrive.
const STOP_POLL: Duration = Duration::from_millis(50);

/// Raw key events for the listener thread: a `handy-keys`
/// [`KeyboardListener`], or a fake in tests.
trait RawKeys: Send {
    /// The next event, [`handy_keys::Error::Timeout`] after `timeout`, or
    /// another error once the listener is dead.
    fn recv_timeout(&self, timeout: Duration) -> handy_keys::Result<KeyEvent>;
}

impl RawKeys for KeyboardListener {
    fn recv_timeout(&self, timeout: Duration) -> handy_keys::Result<KeyEvent> {
        KeyboardListener::recv_timeout(self, timeout)
    }
}

/// Installs the keyboard hook: [`open_listener`], or a fake in tests.
type Open = Box<dyn FnMut() -> Result<Box<dyn RawKeys>, HotkeyError> + Send>;

/// The global hotkey on macOS. Needs Accessibility; without it
/// [`start`](HotkeySource::start) fails with
/// [`HotkeyError::PermissionDenied`]. Modifier-only specs keep working under
/// Secure Input (macOS still delivers modifier changes); specs with a key
/// don't.
///
/// If the event tap is paused and resumed, or misses a modifier change, it
/// sends [`HotkeyEvent::Interrupted`]; if it dies, [`HotkeyEvent::Failed`],
/// after which `start` works again.
pub struct MacHotkey {
    spec: HotkeySpec,
    open: Open,
    running: Option<Running>,
}

struct Running {
    /// Set by `stop`, or by the thread itself when the listener dies.
    done: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl MacHotkey {
    /// A source for `spec`; it listens only once started.
    #[must_use]
    pub fn new(spec: HotkeySpec) -> Self {
        Self::with_open(spec, Box::new(open_listener))
    }

    fn with_open(spec: HotkeySpec, open: Open) -> Self {
        Self {
            spec,
            open,
            running: None,
        }
    }
}

fn open_listener() -> Result<Box<dyn RawKeys>, HotkeyError> {
    // handy-keys checks too, but its error can't tell this apart from a
    // failed tap without matching its message.
    if !is_accessibility_trusted() {
        return Err(HotkeyError::PermissionDenied);
    }
    let listener = KeyboardListener::new().map_err(|e| match e {
        handy_keys::Error::AccessibilityNotGranted => HotkeyError::PermissionDenied,
        other => HotkeyError::Unavailable(other.to_string()),
    })?;
    Ok(Box::new(listener))
}

impl HotkeySource for MacHotkey {
    fn start(&mut self, mut sink: HotkeySink) -> Result<(), HotkeyError> {
        if let Some(running) = &self.running {
            // A thread whose listener died has set `done` and is ending:
            // reap it and start afresh.
            if !running.done.load(Ordering::SeqCst) {
                return Err(HotkeyError::AlreadyStarted);
            }
            self.stop();
        }
        let keys = (self.open)()?;

        let done = Arc::new(AtomicBool::new(false));
        let thread_done = Arc::clone(&done);
        let matcher = HotkeyMatcher::new(self.spec);
        let thread = thread::Builder::new()
            .name("hotkey".into())
            // Dropping `keys` at the end stops and joins the tap thread.
            .spawn(move || listen(&*keys, matcher, &thread_done, &mut sink))
            .map_err(|e| HotkeyError::Unavailable(e.to_string()))?;
        self.running = Some(Running { done, thread });
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(running) = self.running.take() {
            running.done.store(true, Ordering::SeqCst);
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

/// The listener thread: matches `keys` until `done` is set, or until the
/// listener dies, which it reports with [`HotkeyEvent::Failed`] after
/// setting `done` itself.
fn listen(
    keys: &dyn RawKeys,
    mut matcher: HotkeyMatcher,
    done: &AtomicBool,
    sink: &mut HotkeySink,
) {
    while !done.load(Ordering::SeqCst) {
        match keys.recv_timeout(STOP_POLL) {
            Ok(event) if event.is_resync() => {
                // Presses or releases may be lost: whatever was held is over.
                tracing::debug!("the hotkey listener lost track of the keyboard");
                matcher.reset();
                sink(HotkeyEvent::Interrupted);
            }
            Ok(event) => {
                if let Some(out) = to_input(&event).and_then(|i| matcher.on_input(i)) {
                    sink(out);
                }
            }
            Err(handy_keys::Error::Timeout) => {}
            Err(error) => {
                tracing::warn!(%error, "the hotkey listener stopped");
                done.store(true, Ordering::SeqCst);
                sink(HotkeyEvent::Failed);
                return;
            }
        }
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
    use std::sync::mpsc;

    use dictation_core::coordinator::{self, MAX_RECORDING, State};
    use dictation_core::events::{Command, Event};
    use dictation_ports::HotkeyEvent::{CancelPressed, Failed, Interrupted, Pressed, Released};

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

    /// What the patched handy-keys sends, scripted. Disconnecting `events`
    /// is the listener dying, which handy-keys reports as
    /// `EventLoopNotRunning`.
    struct FakeKeys(mpsc::Receiver<KeyEvent>);

    impl RawKeys for FakeKeys {
        fn recv_timeout(&self, timeout: Duration) -> handy_keys::Result<KeyEvent> {
            self.0.recv_timeout(timeout).map_err(|e| match e {
                mpsc::RecvTimeoutError::Timeout => handy_keys::Error::Timeout,
                mpsc::RecvTimeoutError::Disconnected => handy_keys::Error::EventLoopNotRunning,
            })
        }
    }

    /// A started adapter on fake listeners: `keys` feeds the current one,
    /// `events` gets what the adapter sends.
    struct Harness {
        hotkey: MacHotkey,
        listeners: mpsc::Receiver<mpsc::Sender<KeyEvent>>,
        keys: mpsc::Sender<KeyEvent>,
        events: mpsc::Receiver<dictation_ports::HotkeyEvent>,
    }

    const WAIT: Duration = Duration::from_secs(5);

    impl Harness {
        fn start() -> Self {
            let (opened, listeners) = mpsc::channel();
            let mut hotkey = MacHotkey::with_open(
                HotkeySpec::default(),
                Box::new(move || {
                    let (keys, rx) = mpsc::channel();
                    let _ = opened.send(keys);
                    Ok(Box::new(FakeKeys(rx)) as Box<dyn RawKeys>)
                }),
            );
            let (sink, events) = mpsc::channel();
            hotkey
                .start(Box::new(move |e| {
                    let _ = sink.send(e);
                }))
                .unwrap();
            let keys = listeners.recv().unwrap();
            Self {
                hotkey,
                listeners,
                keys,
                events,
            }
        }

        fn send(&self, events: &[KeyEvent]) {
            for &event in events {
                self.keys.send(event).unwrap();
            }
        }

        fn next(&self) -> dictation_ports::HotkeyEvent {
            self.events.recv_timeout(WAIT).unwrap()
        }

        /// Nothing more within a few polls of the listener thread.
        fn assert_quiet(&self) {
            assert_eq!(
                self.events.recv_timeout(STOP_POLL * 4),
                Err(mpsc::RecvTimeoutError::Timeout)
            );
        }
    }

    const RIGHT: Modifiers = Modifiers::OPT_RIGHT;
    const LEFT: Modifiers = Modifiers::OPT_LEFT;

    fn none() -> Modifiers {
        Modifiers::empty()
    }

    #[test]
    fn criterion_1_option_held_at_start_then_released_is_not_a_press() {
        // The patched handy-keys reads a release as a release even when the
        // key was down before the tap started (third_party/handy-keys,
        // dictation_patch_tests). Each case: what it sends, from the start
        // with Option already held, through the next press and release.
        let cases: [(&str, Vec<KeyEvent>); 4] = [
            (
                "right held",
                vec![
                    modifier_event(none(), RIGHT, false),
                    modifier_event(RIGHT, RIGHT, true),
                    modifier_event(none(), RIGHT, false),
                ],
            ),
            (
                "left held",
                vec![
                    modifier_event(none(), LEFT, false),
                    modifier_event(RIGHT, RIGHT, true),
                    modifier_event(none(), RIGHT, false),
                ],
            ),
            (
                "both held, right let go first",
                vec![
                    modifier_event(LEFT, RIGHT, false),
                    modifier_event(none(), LEFT, false),
                    modifier_event(RIGHT, RIGHT, true),
                    modifier_event(none(), RIGHT, false),
                ],
            ),
            (
                // Left's press is found on Right's release: a resync
                // first (#85 review round 1, B1). Idle ignores it.
                "both held, missed by the start snapshot",
                vec![
                    KeyEvent::resync(LEFT),
                    modifier_event(LEFT, RIGHT, false),
                    modifier_event(none(), LEFT, false),
                    modifier_event(RIGHT, RIGHT, true),
                    modifier_event(none(), RIGHT, false),
                ],
            ),
        ];
        for (name, events) in cases {
            let harness = Harness::start();
            harness.send(&events);
            if events[0].is_resync() {
                assert_eq!(harness.next(), Interrupted, "{name}");
            }
            assert_eq!(harness.next(), Pressed, "{name}");
            assert_eq!(harness.next(), Released, "{name}");
            harness.assert_quiet();
        }
    }

    #[test]
    fn criterion_1_right_option_works_while_left_option_stays_held() {
        let harness = Harness::start();
        harness.send(&[
            modifier_event(LEFT, LEFT, true),
            modifier_event(LEFT | RIGHT, RIGHT, true),
            modifier_event(LEFT, RIGHT, false),
        ]);
        assert_eq!(harness.next(), Pressed);
        assert_eq!(harness.next(), Released);
        harness.assert_quiet();
    }

    #[test]
    fn criterion_2_a_resync_while_held_interrupts_and_resets() {
        let harness = Harness::start();
        harness.send(&[
            modifier_event(RIGHT, RIGHT, true),
            // The tap was off while Right Option came up; nothing else
            // comes after the resync.
            KeyEvent::resync(none()),
        ]);
        assert_eq!(harness.next(), Pressed);
        assert_eq!(harness.next(), Interrupted);
        harness.assert_quiet();

        // The matcher starts over: the next hold is a new press.
        harness.send(&[
            modifier_event(RIGHT, RIGHT, true),
            modifier_event(none(), RIGHT, false),
        ]);
        assert_eq!(harness.next(), Pressed);
        assert_eq!(harness.next(), Released);
    }

    #[test]
    fn criterion_2_after_a_resync_a_still_held_key_does_not_release() {
        let harness = Harness::start();
        harness.send(&[
            modifier_event(RIGHT, RIGHT, true),
            KeyEvent::resync(RIGHT),
            // Let go after the tap came back: the hold already ended.
            modifier_event(none(), RIGHT, false),
        ]);
        assert_eq!(harness.next(), Pressed);
        assert_eq!(harness.next(), Interrupted);
        harness.assert_quiet();
    }

    /// What the coordinator does with what the adapter sent, from Idle, one
    /// second apart, then at the duration limit.
    fn coordinate(events: &[dictation_ports::HotkeyEvent]) -> Vec<Command> {
        let mut state = State::Idle;
        let mut commands = Vec::new();
        let mut now = Duration::ZERO;
        for event in events {
            now += Duration::from_secs(1);
            let event = match event {
                Pressed => Event::HotkeyPressed(now),
                Released => Event::HotkeyReleased(now),
                CancelPressed => Event::CancelPressed,
                Interrupted => Event::HotkeyInterrupted,
                Failed => Event::HotkeyFailed,
            };
            let (next, out) = coordinator::step(state, event);
            state = next;
            commands.extend(out);
        }
        let (_, out) = coordinator::step(state, Event::Tick(now + MAX_RECORDING));
        commands.extend(out);
        commands
    }

    /// Everything the adapter sends for `keys`, until it goes quiet.
    fn outputs(keys: &[KeyEvent]) -> Vec<dictation_ports::HotkeyEvent> {
        let harness = Harness::start();
        harness.send(keys);
        let mut out = Vec::new();
        while let Ok(event) = harness.events.recv_timeout(STOP_POLL * 4) {
            out.push(event);
        }
        out
    }

    #[test]
    fn review_b1_missed_release_then_another_modifier_discards() {
        // What the patched handy-keys sends when Right Option's release is
        // missed and Shift (Cmd, Caps Lock, Fn) comes next: a resync with
        // Option gone, then that key's own event (dictation_patch_tests::
        // a_missed_release_seen_on_another_modifier_sends_a_resync_first).
        let shift = Modifiers::SHIFT_LEFT;
        let nexts = [
            modifier_event(shift, shift, true),
            modifier_event(Modifiers::SHIFT_RIGHT, Modifiers::SHIFT_RIGHT, true),
            modifier_event(Modifiers::CMD_RIGHT, Modifiers::CMD_RIGHT, true),
            key_event(none(), handy_keys::Key::CapsLock, true),
            modifier_event(Modifiers::FN, Modifiers::FN, true),
        ];
        for next in nexts {
            let out = outputs(&[
                modifier_event(RIGHT, RIGHT, true),
                KeyEvent::resync(none()),
                next,
                modifier_event(none(), next.changed_modifier.unwrap_or(shift), false),
            ]);
            assert_eq!(out, [Pressed, Interrupted], "{next:?}");
            let commands = coordinate(&out);
            assert!(
                commands.contains(&Command::DiscardRecording),
                "{commands:?}"
            );
            assert!(!commands.contains(&Command::StopRecording), "{commands:?}");
            assert!(!commands.contains(&Command::Transcribe), "{commands:?}");
        }
    }

    #[test]
    fn other_modifiers_and_keys_while_held_do_not_cancel() {
        let (sl, sr) = (Modifiers::SHIFT_LEFT, Modifiers::SHIFT_RIGHT);
        let (cl, cr) = (Modifiers::CMD_LEFT, Modifiers::CMD_RIGHT);
        let fun = Modifiers::FN;
        let out = outputs(&[
            modifier_event(RIGHT, RIGHT, true),
            modifier_event(RIGHT | sl, sl, true),
            key_event(RIGHT | sl, handy_keys::Key::A, true),
            key_event(RIGHT | sl, handy_keys::Key::A, false),
            modifier_event(RIGHT, sl, false),
            modifier_event(RIGHT | sr, sr, true),
            modifier_event(RIGHT, sr, false),
            modifier_event(RIGHT | cl, cl, true),
            modifier_event(RIGHT, cl, false),
            modifier_event(RIGHT | cr, cr, true),
            modifier_event(RIGHT, cr, false),
            key_event(RIGHT, handy_keys::Key::CapsLock, true),
            key_event(RIGHT, handy_keys::Key::CapsLock, false),
            modifier_event(RIGHT | fun, fun, true),
            modifier_event(RIGHT, fun, false),
            modifier_event(none(), RIGHT, false),
        ]);
        assert_eq!(out, [Pressed, Released]);
        let commands = coordinate(&out);
        assert!(commands.contains(&Command::Transcribe), "{commands:?}");
        assert!(
            !commands.contains(&Command::DiscardRecording),
            "{commands:?}"
        );
    }

    #[test]
    fn criterion_3_a_dead_listener_fails_and_start_works_again() {
        let mut harness = Harness::start();
        harness.send(&[modifier_event(RIGHT, RIGHT, true)]);
        assert_eq!(harness.next(), Pressed);

        // The tap dies: handy-keys drops its sender.
        let (dead, _) = mpsc::channel();
        drop(std::mem::replace(&mut harness.keys, dead));
        assert_eq!(harness.next(), Failed);
        // Failed is the last event: the sink is dropped.
        assert_eq!(
            harness.events.recv_timeout(WAIT),
            Err(mpsc::RecvTimeoutError::Disconnected)
        );

        let (sink, events) = mpsc::channel();
        harness
            .hotkey
            .start(Box::new(move |e| {
                let _ = sink.send(e);
            }))
            .unwrap();
        harness.keys = harness.listeners.recv_timeout(WAIT).unwrap();
        harness.events = events;
        harness.send(&[
            modifier_event(RIGHT, RIGHT, true),
            modifier_event(none(), RIGHT, false),
        ]);
        assert_eq!(harness.next(), Pressed);
        assert_eq!(harness.next(), Released);
    }

    #[test]
    fn start_while_listening_is_refused_and_stop_then_start_works() {
        let mut harness = Harness::start();
        assert_eq!(
            harness.hotkey.start(Box::new(|_| {})),
            Err(HotkeyError::AlreadyStarted)
        );
        harness.hotkey.stop();
        assert_eq!(
            harness.events.recv_timeout(WAIT),
            Err(mpsc::RecvTimeoutError::Disconnected),
            "stop drops the sink"
        );
        harness.hotkey.start(Box::new(|_| {})).unwrap();
        assert!(harness.listeners.recv_timeout(WAIT).is_ok());
    }

    #[test]
    fn a_failed_open_leaves_it_stopped() {
        let mut hotkey = MacHotkey::with_open(
            HotkeySpec::default(),
            Box::new(|| Err(HotkeyError::PermissionDenied)),
        );
        assert_eq!(
            hotkey.start(Box::new(|_| {})),
            Err(HotkeyError::PermissionDenied)
        );
        assert!(hotkey.running.is_none());
    }
}
