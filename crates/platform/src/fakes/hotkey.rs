use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use dictation_ports::{HotkeyError, HotkeyEvent, HotkeySink, HotkeySource};

use super::lock;

/// A [`HotkeySource`] driven by the test: scripted events are sent in order
/// as soon as it starts, and [`emit`](Self::emit) sends more while it
/// listens. Unlike a real source, the sink runs on the caller's thread
/// (inside `start` or `emit`).
///
/// The sink is called without the fake's lock held, so it may call back
/// into the fake: `is_listening`, `stop`, or `emit` (that event is queued
/// and delivered, in order, after the current one returns).
#[derive(Clone, Default)]
pub struct FakeHotkey {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    script: Vec<HotkeyEvent>,
    listening: bool,
    /// `None` while listening only during a sink call (it is taken out).
    sink: Option<HotkeySink>,
    queue: VecDeque<HotkeyEvent>,
    delivering: bool,
    fail_start: Option<HotkeyError>,
}

impl FakeHotkey {
    /// A source with no scripted events.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A source that sends `events`, in order, when it starts.
    #[must_use]
    pub fn scripted(events: impl IntoIterator<Item = HotkeyEvent>) -> Self {
        let fake = Self::new();
        lock(&fake.state).script = events.into_iter().collect();
        fake
    }

    /// Sends `event` to the sink. Returns whether anyone was listening; like
    /// a real key press, an event while stopped is lost.
    pub fn emit(&self, event: HotkeyEvent) -> bool {
        {
            let mut state = lock(&self.state);
            if !state.listening {
                return false;
            }
            state.queue.push_back(event);
        }
        self.deliver();
        true
    }

    /// Makes the next [`start`](HotkeySource::start) fail with `error`.
    pub fn fail_next_start(&self, error: HotkeyError) {
        lock(&self.state).fail_start = Some(error);
    }

    /// Whether the source is listening.
    #[must_use]
    pub fn is_listening(&self) -> bool {
        lock(&self.state).listening
    }

    /// Sends queued events to the sink, one at a time, with the lock
    /// released during each call. A nested call (from inside the sink)
    /// returns at once; the outer one delivers what it queued.
    fn deliver(&self) {
        let mut state = lock(&self.state);
        if state.delivering {
            return;
        }
        state.delivering = true;
        while state.listening {
            let Some(event) = state.queue.pop_front() else {
                break;
            };
            let Some(mut sink) = state.sink.take() else {
                break;
            };
            drop(state);
            sink(event);
            state = lock(&self.state);
            // Not if the sink stopped the source (or restarted it with a
            // new sink).
            if state.listening && state.sink.is_none() {
                state.sink = Some(sink);
            } else {
                // Dropped without the lock, as in `stop` (#44).
                drop(state);
                drop(sink);
                state = lock(&self.state);
            }
        }
        state.queue.clear();
        state.delivering = false;
    }
}

impl HotkeySource for FakeHotkey {
    fn start(&mut self, sink: HotkeySink) -> Result<(), HotkeyError> {
        {
            let mut state = lock(&self.state);
            if state.listening {
                return Err(HotkeyError::AlreadyStarted);
            }
            if let Some(error) = state.fail_start.take() {
                return Err(error);
            }
            let script = std::mem::take(&mut state.script);
            state.queue.extend(script);
            state.listening = true;
            state.sink = Some(sink);
        }
        self.deliver();
        Ok(())
    }

    fn stop(&mut self) {
        let sink = {
            let mut state = lock(&self.state);
            state.listening = false;
            state.queue.clear();
            state.sink.take()
        };
        // Dropped without the lock, in case dropping it calls back in.
        drop(sink);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use HotkeyEvent::{CancelPressed, Pressed, Released};

    fn channel_sink() -> (HotkeySink, mpsc::Receiver<HotkeyEvent>) {
        let (tx, rx) = mpsc::channel();
        let sink: HotkeySink = Box::new(move |event| {
            let _ = tx.send(event);
        });
        (sink, rx)
    }

    #[test]
    fn emits_scripted_events_in_order_on_start() {
        let mut hotkey = FakeHotkey::scripted([Pressed, Released, Pressed, CancelPressed]);
        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            [Pressed, Released, Pressed, CancelPressed]
        );
    }

    #[test]
    fn emit_reaches_the_sink_only_while_listening() {
        let mut hotkey = FakeHotkey::new();
        let handle = hotkey.clone();
        assert!(!handle.emit(Pressed), "lost before start");

        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert!(handle.is_listening());
        assert!(handle.emit(Pressed));
        assert!(handle.emit(Released));

        hotkey.stop();
        assert!(!handle.is_listening());
        assert!(!handle.emit(Pressed), "lost after stop");
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Pressed, Released]);
    }

    #[test]
    fn start_twice_is_already_started() {
        let mut hotkey = FakeHotkey::new();
        hotkey.start(channel_sink().0).unwrap();
        assert_eq!(
            hotkey.start(channel_sink().0),
            Err(HotkeyError::AlreadyStarted)
        );
    }

    #[test]
    fn failed_start_never_calls_the_sink() {
        let mut hotkey = FakeHotkey::scripted([Pressed]);
        hotkey.fail_next_start(HotkeyError::PermissionDenied);
        let (sink, rx) = channel_sink();
        assert_eq!(hotkey.start(sink), Err(HotkeyError::PermissionDenied));
        assert!(!hotkey.is_listening());
        assert!(rx.try_recv().is_err());

        // The failure is one-shot; the script is still there.
        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Pressed]);
    }

    #[test]
    fn stop_when_not_listening_does_nothing() {
        let mut hotkey = FakeHotkey::new();
        hotkey.stop();
        assert!(!hotkey.is_listening());
    }

    #[test]
    fn the_sink_can_call_back_into_the_fake() {
        let mut hotkey = FakeHotkey::scripted([Pressed]);
        let handle = hotkey.clone();
        let (tx, rx) = mpsc::channel();
        hotkey
            .start(Box::new(move |event| {
                // Neither call deadlocks: the lock isn't held here.
                let _ = tx.send((event, handle.is_listening()));
                if event == Pressed {
                    assert!(handle.emit(Released));
                }
            }))
            .unwrap();
        assert!(hotkey.emit(CancelPressed));
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            [(Pressed, true), (Released, true), (CancelPressed, true)]
        );
    }

    #[test]
    fn stop_from_inside_the_sink_drops_the_rest() {
        let mut hotkey = FakeHotkey::scripted([Pressed, Released]);
        let mut handle = hotkey.clone();
        let (tx, rx) = mpsc::channel();
        hotkey
            .start(Box::new(move |event| {
                let _ = tx.send(event);
                handle.stop();
            }))
            .unwrap();
        assert!(!hotkey.is_listening());
        assert!(!hotkey.emit(Pressed));
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Pressed]);
    }

    /// Calls back into the fake when dropped.
    struct TouchOnDrop(FakeHotkey, mpsc::Sender<bool>);

    impl Drop for TouchOnDrop {
        fn drop(&mut self) {
            let _ = self.1.send(self.0.is_listening());
        }
    }

    #[test]
    fn a_sink_stopped_from_inside_is_dropped_without_the_lock() {
        let mut hotkey = FakeHotkey::scripted([Pressed]);
        let mut handle = hotkey.clone();
        let (tx, rx) = mpsc::channel();
        let touch = TouchOnDrop(hotkey.clone(), tx);
        hotkey
            .start(Box::new(move |_| {
                let _ = &touch;
                handle.stop();
            }))
            .unwrap();
        // Dropping the sink called `is_listening` without deadlocking.
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [false]);
    }
}
