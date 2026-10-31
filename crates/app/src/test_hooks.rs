//! Test hooks: the `--press` and `--release` command-line flags, which feed
//! synthetic hotkey events into the pipeline so e2e scripts can drive a
//! running app without a keyboard.
//!
//! [`SyntheticHotkey`] wraps the real [`HotkeySource`] and adds an
//! [`Injector`]; the shell parses each launch's flags with [`hook_events`]
//! (a second launch's arrive through the single-instance plugin) and injects
//! them. These are test hooks, not a feature: nothing else sends synthetic
//! keys.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dictation_core::error::DictationError;
use dictation_ports::{HotkeyError, HotkeyEvent, HotkeySink, HotkeySource, Permission};

/// The flag that injects [`HotkeyEvent::Pressed`].
pub const PRESS_FLAG: &str = "--press";
/// The flag that injects [`HotkeyEvent::Released`].
pub const RELEASE_FLAG: &str = "--release";

/// The hotkey events `args` ask for, in order. Every other argument (the
/// program path, macOS's `-psn_…`, anything unknown) is ignored, so a launch
/// without hooks yields nothing.
#[must_use]
pub fn hook_events<I, S>(args: I) -> Vec<HotkeyEvent>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .filter_map(|arg| match arg.as_ref() {
            PRESS_FLAG => Some(HotkeyEvent::Pressed),
            RELEASE_FLAG => Some(HotkeyEvent::Released),
            _ => None,
        })
        .collect()
}

type SharedSink = Arc<Mutex<Option<HotkeySink>>>;

fn lock(sink: &SharedSink) -> MutexGuard<'_, Option<HotkeySink>> {
    sink.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A [`HotkeySource`] that sends both the real source's events and the ones
/// an [`Injector`] injects to the same sink, one at a time.
///
/// If the real source can't start (no Accessibility, no adapter for this
/// OS), `start` still succeeds so the hooks keep working; the error is
/// logged and kept in [`real_error`](Self::real_error).
pub struct SyntheticHotkey<H> {
    real: H,
    sink: SharedSink,
    real_error: Option<HotkeyError>,
}

/// Injects synthetic events into a [`SyntheticHotkey`]. Cheap to clone;
/// callable from any thread.
#[derive(Clone)]
pub struct Injector {
    sink: SharedSink,
}

impl<H: HotkeySource> SyntheticHotkey<H> {
    /// Wraps `real`; returns the source and its injector.
    pub fn new(real: H) -> (Self, Injector) {
        let sink = SharedSink::default();
        let injector = Injector {
            sink: Arc::clone(&sink),
        };
        (
            Self {
                real,
                sink,
                real_error: None,
            },
            injector,
        )
    }

    /// Why the real source didn't start at the last
    /// [`start`](HotkeySource::start), if it didn't.
    #[must_use]
    pub fn real_error(&self) -> Option<&HotkeyError> {
        self.real_error.as_ref()
    }
}

impl<H: HotkeySource> HotkeySource for SyntheticHotkey<H> {
    fn start(&mut self, sink: HotkeySink) -> Result<(), HotkeyError> {
        {
            let mut shared = lock(&self.sink);
            if shared.is_some() {
                return Err(HotkeyError::AlreadyStarted);
            }
            *shared = Some(sink);
        }
        let forward = Arc::clone(&self.sink);
        self.real_error = self
            .real
            .start(Box::new(move |event| {
                if let Some(sink) = lock(&forward).as_mut() {
                    sink(event);
                }
            }))
            .err();
        match &self.real_error {
            None => {}
            Some(HotkeyError::PermissionDenied) => tracing::warn!(
                error = %DictationError::PermissionMissing(Permission::Accessibility),
                "PermissionMissing: the hotkey is off; only the --press/--release test hooks work"
            ),
            Some(error) => tracing::warn!(
                %error,
                "the hotkey is off; only the --press/--release test hooks work"
            ),
        }
        Ok(())
    }

    fn stop(&mut self) {
        // The real source first, without holding the lock: its stop may wait
        // for a sink call that needs it.
        self.real.stop();
        *lock(&self.sink) = None;
    }
}

impl Injector {
    /// Sends `event` to the pipeline. Returns whether it was listening; an
    /// event while stopped is lost, like a real key press.
    pub fn inject(&self, event: HotkeyEvent) -> bool {
        match lock(&self.sink).as_mut() {
            Some(sink) => {
                sink(event);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use HotkeyEvent::{CancelPressed, Pressed, Released};
    use dictation_platform::fakes::FakeHotkey;

    use super::*;

    fn channel_sink() -> (HotkeySink, mpsc::Receiver<HotkeyEvent>) {
        let (tx, rx) = mpsc::channel();
        (
            Box::new(move |event| {
                let _ = tx.send(event);
            }),
            rx,
        )
    }

    #[test]
    fn parses_hook_flags_in_order_and_ignores_the_rest() {
        assert_eq!(
            hook_events([
                "/Applications/dictation-app.app/Contents/MacOS/dictation-app",
                "--press",
                "-psn_0_12345",
                "--verbose",
                "--release",
                "--press",
            ]),
            [Pressed, Released, Pressed]
        );
        assert!(hook_events(["dictation-app"]).is_empty());
        assert!(hook_events(Vec::<String>::new()).is_empty());
        // Exact match only.
        assert!(hook_events(["--PRESS", "--press=1", "press", "-press"]).is_empty());
    }

    #[test]
    fn real_and_injected_events_reach_the_same_sink() {
        let real = FakeHotkey::new();
        let (mut hotkey, injector) = SyntheticHotkey::new(real.clone());
        assert!(!injector.inject(Pressed), "lost before start");

        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert!(hotkey.real_error().is_none());
        assert!(injector.inject(Pressed));
        assert!(real.emit(Released));
        assert!(real.emit(CancelPressed));
        assert!(injector.inject(Released));

        hotkey.stop();
        assert!(!real.is_listening());
        assert!(!injector.inject(Pressed), "lost after stop");
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            [Pressed, Released, CancelPressed, Released]
        );
    }

    #[test]
    fn hooks_work_when_the_real_source_cannot_start() {
        let real = FakeHotkey::new();
        real.fail_next_start(HotkeyError::PermissionDenied);
        let (mut hotkey, injector) = SyntheticHotkey::new(real);
        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert_eq!(hotkey.real_error(), Some(&HotkeyError::PermissionDenied));
        assert!(injector.inject(Pressed));
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Pressed]);
    }

    #[test]
    fn real_scripted_events_on_start_are_forwarded() {
        let (mut hotkey, _injector) = SyntheticHotkey::new(FakeHotkey::scripted([Pressed]));
        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Pressed]);
    }

    #[test]
    fn start_twice_is_already_started_and_restart_works() {
        let (mut hotkey, injector) = SyntheticHotkey::new(FakeHotkey::new());
        hotkey.start(channel_sink().0).unwrap();
        assert_eq!(
            hotkey.start(channel_sink().0),
            Err(HotkeyError::AlreadyStarted)
        );
        hotkey.stop();
        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert!(injector.inject(Released));
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Released]);
    }

    #[test]
    fn injector_works_from_another_thread() {
        let (mut hotkey, injector) = SyntheticHotkey::new(FakeHotkey::new());
        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        std::thread::spawn(move || injector.inject(Pressed))
            .join()
            .unwrap();
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Pressed]);
    }
}
