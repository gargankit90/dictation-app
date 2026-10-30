use std::sync::{Arc, Mutex};

use dictation_ports::{HotkeyError, HotkeyEvent, HotkeySink, HotkeySource};

use super::lock;

/// A [`HotkeySource`] driven by the test: scripted events are sent in order
/// as soon as it starts, and [`emit`](Self::emit) sends more while it
/// listens. Unlike a real source, the sink runs on the caller's thread
/// (inside `start` or `emit`).
#[derive(Clone, Default)]
pub struct FakeHotkey {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    script: Vec<HotkeyEvent>,
    sink: Option<HotkeySink>,
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
        match lock(&self.state).sink.as_mut() {
            Some(sink) => {
                sink(event);
                true
            }
            None => false,
        }
    }

    /// Makes the next [`start`](HotkeySource::start) fail with `error`.
    pub fn fail_next_start(&self, error: HotkeyError) {
        lock(&self.state).fail_start = Some(error);
    }

    /// Whether the source is listening.
    #[must_use]
    pub fn is_listening(&self) -> bool {
        lock(&self.state).sink.is_some()
    }
}

impl HotkeySource for FakeHotkey {
    fn start(&mut self, mut sink: HotkeySink) -> Result<(), HotkeyError> {
        let mut state = lock(&self.state);
        if state.sink.is_some() {
            return Err(HotkeyError::AlreadyStarted);
        }
        if let Some(error) = state.fail_start.take() {
            return Err(error);
        }
        for event in std::mem::take(&mut state.script) {
            sink(event);
        }
        state.sink = Some(sink);
        Ok(())
    }

    fn stop(&mut self) {
        lock(&self.state).sink = None;
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
}
