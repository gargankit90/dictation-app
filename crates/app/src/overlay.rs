//! What the overlay shows (M5.1), driven by the pipeline's [`UiEvent`]s.
//!
//! [`OverlayModel`] is the pure part: it turns each event into the view the
//! overlay should show. [`run`] drives it against an [`OverlaySurface`] (the
//! shell's overlay window) and hides an error after [`ERROR_VISIBLE_FOR`].
//! Its events come through the same bounded queue as the main UI's
//! ([`crate::ui_events`], #84, #112): an overlay webview that stops reading
//! can't make the app grow, and the overlay still ends on the current phase.
//!
//! The overlay never shows the transcript: an error shows only the
//! [`DictationError`](dictation_core::error::DictationError)'s user-facing
//! text, which never contains the transcript or an API key.

use std::time::Duration;

use dictation_core::coordinator::Phase;
use dictation_core::diagnostics::ErrorCode;
use dictation_core::error::DictationError;
use tokio::time::Instant;

use crate::pipeline::UiEvent;
use crate::ui_events::UiReceiver;

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
        /// One plain sentence naming the fix. No error code: codes stay in
        /// Troubleshoot and the report (U1).
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
    /// Whether the error showing is [`DictationError::HotkeyFailed`], which
    /// [`UiEvent::HotkeyRestored`] hides (#155).
    hotkey_error: bool,
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
    /// the next recording starts; going idle doesn't hide it. The hotkey's
    /// error also hides as soon as the hotkey works again; other errors
    /// don't.
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
            UiEvent::HotkeyRestored if self.hotkey_error => OverlayView::Hidden,
            UiEvent::HotkeyRestored => return None,
            UiEvent::Error(error) => {
                // Always a change, even with the same text: it restarts the
                // timer.
                // The code table's message and next step (E1), not the
                // error's `Display` text: it is the same short text in the
                // tray, and it never carries a reason.
                let code = ErrorCode::of(error);
                self.view = OverlayView::Error {
                    message: code.user_text(),
                };
                self.hotkey_error = matches!(error, DictationError::HotkeyFailed);
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
        // Every view `set` shows is not an error.
        self.hotkey_error = false;
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

/// Where [`run`] reads its events: the app passes the bounded
/// [`UiReceiver`] (#112); tests may pass a plain channel. `next` must be
/// cancel-safe (it is raced against the error timer).
pub trait OverlayEvents: Send {
    /// The next event; `None` once no more will come.
    fn next(&mut self) -> impl Future<Output = Option<UiEvent>> + Send;
}

impl OverlayEvents for UiReceiver {
    fn next(&mut self) -> impl Future<Output = Option<UiEvent>> + Send {
        self.recv()
    }
}

impl OverlayEvents for tokio::sync::mpsc::UnboundedReceiver<UiEvent> {
    fn next(&mut self) -> impl Future<Output = Option<UiEvent>> + Send {
        self.recv()
    }
}

/// Shows each event on `surface` until `events` closes; an error hides
/// after `error_visible_for`.
pub async fn run<S: OverlaySurface>(
    mut events: impl OverlayEvents,
    mut surface: S,
    error_visible_for: Duration,
) {
    let mut model = OverlayModel::default();
    let mut error_deadline: Option<Instant> = None;
    loop {
        let change = tokio::select! {
            event = events.next() => {
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

    use super::*;
    use crate::ui_events::{UI_ERRORS_QUEUED, UI_EVENTS_QUEUED, ui_channel};

    fn error() -> UiEvent {
        UiEvent::Error(DictationError::EmptyTranscript)
    }

    fn error_view() -> OverlayView {
        OverlayView::Error {
            message: ErrorCode::NoSpeech.user_text(),
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

    /// E1 round 1: the overlay shows the code table's message and next
    /// step, not the old `Display` text.
    #[test]
    fn an_error_shows_the_tables_message_and_next_step() {
        use dictation_core::error::Permission;
        let load_failure = DictationError::TranscriptionFailed {
            reason: "Something went wrong: couldn't load the on-device model: out of memory."
                .into(),
        };
        let cases = [
            (
                load_failure,
                "Pick or download another in Settings → Transcription.",
            ),
            (
                DictationError::InsertFailed {
                    reason: "no focused app".into(),
                },
                "Check Accessibility in System Settings, then dictate again.",
            ),
            (
                DictationError::PermissionMissing(Permission::Accessibility),
                "Allow it in System Settings → Privacy & Security → Accessibility.",
            ),
            (
                DictationError::PermissionMissing(Permission::Microphone),
                "Allow it in System Settings → Privacy & Security → Microphone, then relaunch Vocet.",
            ),
            (
                DictationError::EmptyTranscript,
                "Speak closer to the microphone and try again.",
            ),
        ];
        for (error, step) in cases {
            let mut model = OverlayModel::default();
            let change = model.on_event(&UiEvent::Error(error.clone())).unwrap();
            let OverlayView::Error { message } = change.view else {
                panic!("expected an error view");
            };
            assert!(message.ends_with(step), "{error:?}: {message}");
            assert_eq!(message, ErrorCode::of(&error).user_text());
            assert!(
                !message.contains(ErrorCode::of(&error).as_str()),
                "{message}"
            );
            assert!(!message.contains("Something went wrong"), "{message}");
            assert!(!message.contains("no focused app"), "{message}");
        }
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
        assert!(!message.contains("401"), "{message}");
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

    fn hotkey_error_view() -> OverlayView {
        OverlayView::Error {
            message: ErrorCode::HotkeyUnregistered.user_text(),
        }
    }

    /// #155's reproduction: the hotkey's error hides as soon as the hotkey
    /// works again, not when its timer runs out.
    #[test]
    fn the_hotkey_error_hides_when_the_hotkey_is_restored() {
        let mut model = OverlayModel::default();
        model.on_event(&UiEvent::Error(DictationError::HotkeyFailed));
        assert_eq!(model.view(), &hotkey_error_view());
        assert_eq!(
            model.on_event(&UiEvent::HotkeyRestored),
            shown(OverlayView::Hidden)
        );
        assert_eq!(model.view(), &OverlayView::Hidden);
        // Its timer, if it still fires, hides nothing more.
        assert_eq!(model.on_error_expired(), None);
    }

    /// Other errors, and whatever replaced the hotkey's error, stay.
    #[test]
    fn a_restored_hotkey_leaves_other_views_alone() {
        let mut model = OverlayModel::default();
        model.on_event(&error());
        assert_eq!(model.on_event(&UiEvent::HotkeyRestored), None);
        assert_eq!(model.view(), &error_view());

        // A dictation error that replaced the hotkey's.
        let mut model = OverlayModel::default();
        model.on_event(&UiEvent::Error(DictationError::HotkeyFailed));
        model.on_event(&error());
        assert_eq!(model.on_event(&UiEvent::HotkeyRestored), None);
        assert_eq!(model.view(), &error_view());

        // A recording that replaced it.
        let mut model = OverlayModel::default();
        model.on_event(&UiEvent::Error(DictationError::HotkeyFailed));
        model.on_event(&UiEvent::StateChanged(Phase::Recording));
        assert_eq!(model.on_event(&UiEvent::HotkeyRestored), None);
        assert_eq!(model.view(), &OverlayView::Recording);

        // Nothing showing.
        let mut model = OverlayModel::default();
        assert_eq!(model.on_event(&UiEvent::HotkeyRestored), None);
    }

    /// The hotkey's error, still showing after the pipeline went idle,
    /// hides on the return too.
    #[test]
    fn the_hotkey_error_hides_on_restore_after_going_idle() {
        let mut model = OverlayModel::default();
        model.on_event(&UiEvent::Error(DictationError::HotkeyFailed));
        assert_eq!(model.on_event(&UiEvent::StateChanged(Phase::Idle)), None);
        assert_eq!(
            model.on_event(&UiEvent::HotkeyRestored),
            shown(OverlayView::Hidden)
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
        let (tx, rx) = ui_channel();
        let recorder = Recorder::default();
        for phase in [Phase::Recording, Phase::Transcribing, Phase::Idle] {
            tx.send(UiEvent::StateChanged(phase));
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
        let (tx, rx) = ui_channel();
        let recorder = Recorder::default();
        let task = tokio::spawn(run(rx, recorder.clone(), VISIBLE));
        tx.send(error());
        tx.send(UiEvent::StateChanged(Phase::Idle));
        tokio::time::sleep(VISIBLE * 2).await;
        drop(tx);
        task.await.unwrap();

        assert_eq!(recorder.views(), [error_view(), OverlayView::Hidden]);
        let times = recorder.times();
        assert_eq!(times[1] - times[0], VISIBLE);
    }

    /// In `run`: the restore hides the hotkey's error at once, and its
    /// timer doesn't hide anything later.
    #[tokio::test(start_paused = true)]
    async fn run_hides_the_hotkey_error_when_the_hotkey_is_restored() {
        let (tx, rx) = ui_channel();
        let recorder = Recorder::default();
        let task = tokio::spawn(run(rx, recorder.clone(), VISIBLE));
        tx.send(UiEvent::Error(DictationError::HotkeyFailed));
        tokio::time::sleep(VISIBLE / 4).await;
        tx.send(UiEvent::HotkeyRestored);
        tokio::time::sleep(VISIBLE * 2).await;
        drop(tx);
        task.await.unwrap();

        assert_eq!(recorder.views(), [hotkey_error_view(), OverlayView::Hidden]);
        let times = recorder.times();
        assert_eq!(times[1] - times[0], VISIBLE / 4);
    }

    #[tokio::test(start_paused = true)]
    async fn run_does_not_hide_a_recording_that_replaced_an_error() {
        let (tx, rx) = ui_channel();
        let recorder = Recorder::default();
        let task = tokio::spawn(run(rx, recorder.clone(), VISIBLE));
        tx.send(error());
        tokio::time::sleep(VISIBLE / 2).await;
        tx.send(UiEvent::StateChanged(Phase::Recording));
        tokio::time::sleep(VISIBLE * 2).await;
        drop(tx);
        task.await.unwrap();

        assert_eq!(recorder.views(), [error_view(), OverlayView::Recording]);
    }

    /// #112: an overlay that stops reading (a stalled webview) keeps a
    /// bounded queue under the same policy as the main UI's: phases
    /// coalesce, the oldest errors are kept, and once it reads again it ends
    /// on the pipeline's current phase.
    #[tokio::test(start_paused = true)]
    async fn a_stalled_overlay_stays_bounded_and_ends_on_the_current_phase() {
        let (tx, rx) = ui_channel();
        // Nothing reads while the pipeline runs a thousand dictations, each
        // failing.
        for _ in 0..1000 {
            for phase in [Phase::Recording, Phase::Transcribing, Phase::Idle] {
                tx.send(UiEvent::StateChanged(phase));
            }
            tx.send(error());
        }
        tx.send(UiEvent::StateChanged(Phase::Recording));
        assert!(
            rx.queued() <= UI_EVENTS_QUEUED,
            "the overlay channel is not bounded: {} events queued, at most {UI_EVENTS_QUEUED} allowed",
            rx.queued()
        );
        assert_eq!(
            rx.dropped_errors(),
            1000 - u64::try_from(UI_ERRORS_QUEUED).unwrap(),
            "the overlay channel is not bounded: errors were kept beyond {UI_ERRORS_QUEUED}"
        );

        // It reads again. `run` ends once the closed queue is empty; if the
        // queue were not bounded it would show every event, so a bound on
        // the (virtual) time it takes fails the test with a message, not a
        // CI timeout (#150). A `run` that never yields is stopped by the
        // test's own time limit in .config/nextest.toml.
        let recorder = Recorder::default();
        drop(tx);
        tokio::time::timeout(VISIBLE * 10, run(rx, recorder.clone(), VISIBLE))
            .await
            .expect("the overlay channel is not bounded: run did not end after the queue closed");
        let views = recorder.views();
        assert_eq!(views.last(), Some(&OverlayView::Recording), "{views:?}");
        let errors = views
            .iter()
            .filter(|view| matches!(view, OverlayView::Error { .. }))
            .count();
        assert_eq!(errors, UI_ERRORS_QUEUED);
    }
}
