//! What the overlay shows (M5.1), driven by the pipeline's [`UiEvent`]s.
//!
//! [`OverlayModel`] is the pure part: it turns each event into the view the
//! overlay should show. [`run`] drives it against an [`OverlaySurface`] (the
//! shell's overlay window) and hides an error after [`ERROR_VISIBLE_FOR`].
//! The overlay never shows the transcript: an error shows only the
//! [`DictationError`](dictation_core::error::DictationError)'s user-facing
//! text, which never contains the transcript or an API key.

use std::time::Duration;

use dictation_core::coordinator::Phase;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::Instant;

use crate::pipeline::UiEvent;

/// How long an error stays on screen unless something newer replaces it.
pub const ERROR_VISIBLE_FOR: Duration = Duration::from_secs(4);

/// What the overlay shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OverlayView {
    /// Nothing: the overlay window is hidden.
    #[default]
    Hidden,
    /// The microphone is recording.
    Recording,
    /// The recording is being transcribed (and then pasted).
    Transcribing,
    /// A dictation failed; `message` is the user-facing error text.
    Error {
        /// The error's `Display` text.
        message: String,
    },
}

/// A change the overlay must make after an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The new view.
    pub view: OverlayView,
    /// Whether this view is an error that hides after
    /// [`ERROR_VISIBLE_FOR`]; a later change cancels the timer.
    pub expires: bool,
}

/// The overlay's state: which view is showing.
#[derive(Debug, Default)]
pub struct OverlayModel {
    view: OverlayView,
}

impl OverlayModel {
    /// The view showing now.
    #[must_use]
    pub fn view(&self) -> &OverlayView {
        &self.view
    }

    /// Applies a pipeline event; `None` if what's showing doesn't change.
    ///
    /// Recording and Transcribing show while the pipeline is in those
    /// phases; pasting (Inserting) keeps Transcribing up until the pipeline
    /// is idle again. An error shows until [`ERROR_VISIBLE_FOR`] passes or
    /// the next recording starts; going idle doesn't hide it.
    pub fn on_event(&mut self, event: &UiEvent) -> Option<Change> {
        let view = match event {
            UiEvent::StateChanged(Phase::Recording) => OverlayView::Recording,
            UiEvent::StateChanged(Phase::Transcribing | Phase::Inserting) => {
                OverlayView::Transcribing
            }
            UiEvent::StateChanged(Phase::Idle) => match self.view {
                OverlayView::Error { .. } => return None,
                _ => OverlayView::Hidden,
            },
            UiEvent::Error(error) => {
                // Always a change, even with the same text: it restarts the
                // timer.
                self.view = OverlayView::Error {
                    message: error.to_string(),
                };
                return Some(Change {
                    view: self.view.clone(),
                    expires: true,
                });
            }
        };
        self.set(view)
    }

    /// The error timer ran out: hides the error if it still shows.
    pub fn on_error_expired(&mut self) -> Option<Change> {
        match self.view {
            OverlayView::Error { .. } => self.set(OverlayView::Hidden),
            _ => None,
        }
    }

    fn set(&mut self, view: OverlayView) -> Option<Change> {
        if self.view == view {
            return None;
        }
        self.view = view.clone();
        Some(Change {
            view,
            expires: false,
        })
    }
}

/// The shell's overlay window.
pub trait OverlaySurface {
    /// Shows `view`, or hides the window for [`OverlayView::Hidden`]. Must
    /// not take focus from the app the user is dictating into.
    fn show(&mut self, view: &OverlayView);
}

/// Shows each event on `surface` until `events` closes; an error hides
/// after `error_visible_for`.
pub async fn run<S: OverlaySurface>(
    mut events: UnboundedReceiver<UiEvent>,
    mut surface: S,
    error_visible_for: Duration,
) {
    let mut model = OverlayModel::default();
    let mut error_deadline: Option<Instant> = None;
    loop {
        let change = tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { break };
                model.on_event(&event)
            }
            () = expiry(error_deadline) => {
                error_deadline = None;
                model.on_error_expired()
            }
        };
        if let Some(change) = change {
            error_deadline = change.expires.then(|| Instant::now() + error_visible_for);
            surface.show(&change.view);
        }
    }
}

/// Completes at `deadline`, or never without one.
async fn expiry(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use dictation_core::error::DictationError;
    use tokio::sync::mpsc;

    use super::*;

    fn error() -> UiEvent {
        UiEvent::Error(DictationError::EmptyTranscript)
    }

    fn error_view() -> OverlayView {
        OverlayView::Error {
            message: DictationError::EmptyTranscript.to_string(),
        }
    }

    fn shown(view: OverlayView) -> Option<Change> {
        Some(Change {
            view,
            expires: false,
        })
    }

    #[test]
    fn starts_hidden() {
        assert_eq!(OverlayModel::default().view(), &OverlayView::Hidden);
    }

    #[test]
    fn a_dictation_shows_recording_then_transcribing_then_hides() {
        let mut model = OverlayModel::default();
        let steps = [
            (Phase::Recording, shown(OverlayView::Recording)),
            (Phase::Transcribing, shown(OverlayView::Transcribing)),
            (Phase::Inserting, None),
            (Phase::Idle, shown(OverlayView::Hidden)),
        ];
        for (phase, expected) in steps {
            assert_eq!(
                model.on_event(&UiEvent::StateChanged(phase)),
                expected,
                "{phase:?}"
            );
        }
    }

    #[test]
    fn idle_while_hidden_changes_nothing() {
        let mut model = OverlayModel::default();
        assert_eq!(model.on_event(&UiEvent::StateChanged(Phase::Idle)), None);
    }

    #[test]
    fn an_error_shows_its_message_and_expires() {
        let mut model = OverlayModel::default();
        model.on_event(&UiEvent::StateChanged(Phase::Transcribing));
        assert_eq!(
            model.on_event(&error()),
            Some(Change {
                view: error_view(),
                expires: true
            })
        );
        assert_eq!(model.view(), &error_view());
    }

    #[test]
    fn an_error_message_never_carries_a_key() {
        let mut model = OverlayModel::default();
        let key = "gsk_abcdefghijklmnop1234567890";
        let change = model
            .on_event(&UiEvent::Error(DictationError::TranscriptionFailed {
                reason: format!("401 Unauthorized for key {key}"),
            }))
            .expect("an error is a change");
        let OverlayView::Error { message } = change.view else {
            panic!("expected an error view");
        };
        assert!(!message.contains(key), "{message}");
        assert!(message.contains("[key hidden]"), "{message}");
    }

    #[test]
    fn going_idle_keeps_the_error_until_it_expires() {
        let mut model = OverlayModel::default();
        model.on_event(&error());
        assert_eq!(model.on_event(&UiEvent::StateChanged(Phase::Idle)), None);
        assert_eq!(model.view(), &error_view());
        assert_eq!(model.on_error_expired(), shown(OverlayView::Hidden));
    }

    #[test]
    fn a_new_recording_replaces_the_error() {
        let mut model = OverlayModel::default();
        model.on_event(&error());
        assert_eq!(
            model.on_event(&UiEvent::StateChanged(Phase::Recording)),
            shown(OverlayView::Recording)
        );
        // The old error's timer no longer hides anything.
        assert_eq!(model.on_error_expired(), None);
        assert_eq!(model.view(), &OverlayView::Recording);
    }

    #[test]
    fn the_same_error_again_restarts_the_timer() {
        let mut model = OverlayModel::default();
        model.on_event(&error());
        assert!(
            model
                .on_event(&error())
                .is_some_and(|change| change.expires)
        );
    }

    /// Records what the overlay was told to show, and when.
    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<(OverlayView, Instant)>>>);

    impl Recorder {
        fn views(&self) -> Vec<OverlayView> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|(view, _)| view.clone())
                .collect()
        }

        fn times(&self) -> Vec<Instant> {
            self.0.lock().unwrap().iter().map(|(_, at)| *at).collect()
        }
    }

    impl OverlaySurface for Recorder {
        fn show(&mut self, view: &OverlayView) {
            self.0.lock().unwrap().push((view.clone(), Instant::now()));
        }
    }

    const VISIBLE: Duration = Duration::from_secs(3);

    #[tokio::test(start_paused = true)]
    async fn run_shows_each_change_and_stops_when_the_events_close() {
        let (tx, rx) = mpsc::unbounded_channel();
        let recorder = Recorder::default();
        for phase in [Phase::Recording, Phase::Transcribing, Phase::Idle] {
            tx.send(UiEvent::StateChanged(phase)).unwrap();
        }
        drop(tx);
        run(rx, recorder.clone(), VISIBLE).await;
        assert_eq!(
            recorder.views(),
            [
                OverlayView::Recording,
                OverlayView::Transcribing,
                OverlayView::Hidden
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn run_hides_an_error_after_it_has_been_visible_long_enough() {
        let (tx, rx) = mpsc::unbounded_channel();
        let recorder = Recorder::default();
        let task = tokio::spawn(run(rx, recorder.clone(), VISIBLE));
        tx.send(error()).unwrap();
        tx.send(UiEvent::StateChanged(Phase::Idle)).unwrap();
        tokio::time::sleep(VISIBLE * 2).await;
        drop(tx);
        task.await.unwrap();

        assert_eq!(recorder.views(), [error_view(), OverlayView::Hidden]);
        let times = recorder.times();
        assert_eq!(times[1] - times[0], VISIBLE);
    }

    #[tokio::test(start_paused = true)]
    async fn run_does_not_hide_a_recording_that_replaced_an_error() {
        let (tx, rx) = mpsc::unbounded_channel();
        let recorder = Recorder::default();
        let task = tokio::spawn(run(rx, recorder.clone(), VISIBLE));
        tx.send(error()).unwrap();
        tokio::time::sleep(VISIBLE / 2).await;
        tx.send(UiEvent::StateChanged(Phase::Recording)).unwrap();
        tokio::time::sleep(VISIBLE * 2).await;
        drop(tx);
        task.await.unwrap();

        assert_eq!(recorder.views(), [error_view(), OverlayView::Recording]);
    }
}
