//! What the overlay shows (M5.1, U4), driven by the pipeline's [`UiEvent`]s
//! and input levels.
//!
//! [`OverlayModel`] is the pure part: it turns each event and level into the
//! view the overlay should show. [`run`] drives it against an
//! [`OverlaySurface`] (the shell's overlay window), hides an error after
//! [`ERROR_VISIBLE_FOR`] and "Paste sent" after [`PASTE_SENT_VISIBLE_FOR`];
//! [`run_with_levels`] also passes each input level to the surface for the
//! meter, and says "No sound" after [`NO_SOUND_AFTER`] of quiet.
//!
//! "Paste sent" follows the pipeline's paste result through the
//! coordinator: it goes Inserting → Idle only when the inserter returned
//! success (`InsertDone`); a failed or blocked paste shows its error first,
//! and a transcript with nothing to paste never reaches Inserting. It says
//! "sent", not "pasted": a successful insert means the paste keystroke was
//! sent to the focused app, not that the app took the text (U4 review).
//! Its events come through the same bounded queue as the main UI's
//! ([`crate::ui_events`], #84, #112): an overlay webview that stops reading
//! can't make the app grow, and the overlay still ends on the current phase.
//!
//! The overlay never shows the transcript: an error shows only the
//! [`DictationError`](dictation_core::error::DictationError)'s user-facing
//! text, which never contains the transcript or an API key.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use dictation_core::coordinator::Phase;
use dictation_core::diagnostics::ErrorCode;
use dictation_core::error::DictationError;
pub use dictation_core::level::NO_SOUND_AFTER;
use dictation_core::level::{LEVEL_FLOOR_DBFS, SilenceWatch};
use tokio::time::Instant;

use crate::pipeline::{LevelReceiver, UiEvent};
use crate::ui_events::UiReceiver;

/// How long an error stays on screen unless something newer replaces it.
pub const ERROR_VISIBLE_FOR: Duration = Duration::from_secs(4);

/// How long "Paste sent" stays on screen after a paste was sent (U4).
pub const PASTE_SENT_VISIBLE_FOR: Duration = Duration::from_millis(800);

/// What the overlay shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OverlayView {
    /// Nothing: the overlay window is hidden.
    #[default]
    Hidden,
    /// The microphone is recording and hears sound: "Listening", with the
    /// meter.
    Recording,
    /// The microphone is recording but has been quiet for
    /// [`NO_SOUND_AFTER`] (U4): "No sound", with the device's name if known.
    NoSound {
        /// The input device's name, if the platform knows it.
        device: Option<String>,
    },
    /// The recording is being transcribed (and then pasted).
    Transcribing,
    /// The paste was sent: the inserter returned success (U4). Not proof
    /// the text landed. Hides after [`PASTE_SENT_VISIBLE_FOR`].
    PasteSent,
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
    /// Whether this view hides by itself: an error after
    /// [`ERROR_VISIBLE_FOR`], "Paste sent" after [`PASTE_SENT_VISIBLE_FOR`]. A later
    /// change cancels the timer.
    pub expires: bool,
}

/// The overlay's state: which view is showing.
#[derive(Debug, Default)]
pub struct OverlayModel {
    view: OverlayView,
    /// Whether the error showing is [`DictationError::HotkeyFailed`], which
    /// [`UiEvent::HotkeyRestored`] hides (#155).
    hotkey_error: bool,
    /// Whether the current dictation reached Inserting (and no error came
    /// since): its Idle means the paste succeeded.
    inserting: bool,
    /// Watches the current recording's levels for "No sound".
    silence: SilenceWatch,
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
            UiEvent::StateChanged(Phase::Recording) => {
                self.inserting = false;
                self.silence = SilenceWatch::new();
                OverlayView::Recording
            }
            UiEvent::StateChanged(Phase::Transcribing) => {
                self.inserting = false;
                OverlayView::Transcribing
            }
            UiEvent::StateChanged(Phase::Inserting) => {
                self.inserting = true;
                OverlayView::Transcribing
            }
            UiEvent::StateChanged(Phase::Idle) => {
                let pasted = std::mem::take(&mut self.inserting);
                match self.view {
                    OverlayView::Error { .. } => return None,
                    // Inserting, then Idle with no error: the paste
                    // succeeded (U4).
                    OverlayView::Transcribing if pasted => {
                        let change = self.set(OverlayView::PasteSent);
                        return change.map(|change| Change {
                            expires: true,
                            ..change
                        });
                    }
                    _ => OverlayView::Hidden,
                }
            }
            UiEvent::HotkeyRestored if self.hotkey_error => OverlayView::Hidden,
            UiEvent::HotkeyRestored => return None,
            UiEvent::Error(error) => {
                // Always a change, even with the same text: it restarts the
                // timer.
                // The code table's message and next step (E1), not the
                // error's `Display` text: it is the same short text in the
                // tray, and it never carries a reason.
                let code = ErrorCode::of(error);
                self.inserting = false;
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

    /// An input level, in dBFS, read at `now` (any monotonic clock) while
    /// recording on `device` (U4): switches between Listening and "No sound"
    /// ([`SilenceWatch`]). Ignored unless recording.
    pub fn on_level(&mut self, dbfs: f32, device: Option<&str>, now: Duration) -> Option<Change> {
        if !self.is_recording() {
            return None;
        }
        let view = if self.silence.on_level(dbfs, now) {
            OverlayView::NoSound {
                device: device.map(str::to_owned),
            }
        } else {
            OverlayView::Recording
        };
        self.set(view)
    }

    /// Whether the view is a recording's (Listening or "No sound"), which
    /// shows the meter.
    #[must_use]
    pub fn is_recording(&self) -> bool {
        matches!(
            self.view,
            OverlayView::Recording | OverlayView::NoSound { .. }
        )
    }

    /// The timer of an error or "Paste sent" ran out: hides it if it still
    /// shows.
    pub fn on_error_expired(&mut self) -> Option<Change> {
        match self.view {
            OverlayView::Error { .. } | OverlayView::PasteSent => self.set(OverlayView::Hidden),
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

/// The newest input level, for the overlay page to read (U4 round 2).
///
/// The page asks for the level (the `get_overlay_level` command) about 20
/// times a second while the pill shows Listening or No sound, instead of
/// the app pushing each level into the webview: a stalled page then asks
/// for nothing, and nothing piles up anywhere. This holds only the newest
/// value, in one atomic: [`set`](Self::set) and [`get`](Self::get) never
/// block and never queue. A level is a number only; it is never logged,
/// stored or sent.
///
/// Clones share the value.
#[derive(Debug, Clone)]
pub struct LatestLevel(Arc<AtomicU32>);

impl Default for LatestLevel {
    fn default() -> Self {
        Self(Arc::new(AtomicU32::new(LEVEL_FLOOR_DBFS.to_bits())))
    }
}

impl LatestLevel {
    /// Silence ([`LEVEL_FLOOR_DBFS`]) until the first level.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the level with `dbfs`.
    pub fn set(&self, dbfs: f32) {
        self.0.store(dbfs.to_bits(), Ordering::Relaxed);
    }

    /// Back to silence: the recording ended, so the next one starts flat.
    pub fn clear(&self) {
        self.set(LEVEL_FLOOR_DBFS);
    }

    /// The newest level, in dBFS.
    #[must_use]
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// The shell's overlay window.
pub trait OverlaySurface {
    /// Shows `view`, or hides the window for [`OverlayView::Hidden`]. Must
    /// not take focus from the app the user is dictating into.
    fn show(&mut self, view: &OverlayView);

    /// Shows an input level, in dBFS, on the meter of the recording view
    /// showing now (U4). Called about 20 times a second while recording,
    /// so it must not log the level at info level, block or queue: the
    /// shell stores it in a [`LatestLevel`] that the page reads. Does
    /// nothing by default.
    fn level(&mut self, dbfs: f32) {
        let _ = dbfs;
    }
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
/// after `error_visible_for`, "Paste sent" after [`PASTE_SENT_VISIBLE_FOR`]. No
/// levels: see [`run_with_levels`].
pub async fn run<S: OverlaySurface>(
    events: impl OverlayEvents,
    surface: S,
    error_visible_for: Duration,
) {
    let (_no_levels, levels) = tokio::sync::watch::channel(None);
    run_with_levels(events, levels, surface, error_visible_for).await;
}

/// [`run`], and each input level from `levels` goes to the surface's meter
/// while recording, and switches the view between Listening and "No sound"
/// (U4). Levels are never logged here.
pub async fn run_with_levels<S: OverlaySurface>(
    mut events: impl OverlayEvents,
    mut levels: LevelReceiver,
    mut surface: S,
    error_visible_for: Duration,
) {
    let mut model = OverlayModel::default();
    let mut deadline: Option<Instant> = None;
    let mut levels_open = true;
    let epoch = Instant::now();
    loop {
        let mut level = None;
        let change = tokio::select! {
            event = events.next() => {
                let Some(event) = event else { break };
                model.on_event(&event)
            }
            () = expiry(deadline) => {
                deadline = None;
                model.on_error_expired()
            }
            changed = levels.changed(), if levels_open => {
                if changed.is_err() {
                    levels_open = false;
                    None
                } else {
                    let reading = levels.borrow_and_update().clone();
                    reading.and_then(|reading| {
                        level = Some(reading.dbfs);
                        model.on_level(reading.dbfs, reading.device.as_deref(), epoch.elapsed())
                    })
                }
            }
        };
        if let Some(change) = change {
            deadline = change.expires.then(|| {
                let visible_for = if change.view == OverlayView::PasteSent {
                    PASTE_SENT_VISIBLE_FOR
                } else {
                    error_visible_for
                };
                Instant::now() + visible_for
            });
            surface.show(&change.view);
        }
        // After the view, so the page draws the level on the right state.
        if let Some(dbfs) = level
            && model.is_recording()
        {
            surface.level(dbfs);
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
    use std::sync::Mutex;

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

    /// U4 criterion 5: a successful paste ends on "Paste sent", which expires.
    #[test]
    fn criterion_u4_5_a_dictation_shows_recording_then_transcribing_then_paste_sent() {
        let mut model = OverlayModel::default();
        let steps = [
            (Phase::Recording, shown(OverlayView::Recording)),
            (Phase::Transcribing, shown(OverlayView::Transcribing)),
            (Phase::Inserting, None),
            (
                Phase::Idle,
                Some(Change {
                    view: OverlayView::PasteSent,
                    expires: true,
                }),
            ),
        ];
        for (phase, expected) in steps {
            assert_eq!(
                model.on_event(&UiEvent::StateChanged(phase)),
                expected,
                "{phase:?}"
            );
        }
    }

    /// U4 criterion 5: nothing pasted (the transcript was dropped, so no
    /// Inserting) just hides.
    #[test]
    fn criterion_u4_5_nothing_pasted_hides_without_paste_sent() {
        let mut model = OverlayModel::default();
        model.on_event(&UiEvent::StateChanged(Phase::Recording));
        model.on_event(&UiEvent::StateChanged(Phase::Transcribing));
        assert_eq!(
            model.on_event(&UiEvent::StateChanged(Phase::Idle)),
            shown(OverlayView::Hidden)
        );
    }

    /// U4 criterion 5: a failed or blocked paste shows its error, never
    /// "Paste sent".
    #[test]
    fn criterion_u4_5_a_failed_paste_shows_the_error_not_paste_sent() {
        for failure in [
            DictationError::InsertFailed {
                reason: "no focused app".into(),
            },
            DictationError::SecureInput,
        ] {
            let mut model = OverlayModel::default();
            for phase in [Phase::Recording, Phase::Transcribing, Phase::Inserting] {
                model.on_event(&UiEvent::StateChanged(phase));
            }
            let change = model.on_event(&UiEvent::Error(failure.clone())).unwrap();
            assert!(matches!(change.view, OverlayView::Error { .. }));
            assert_eq!(model.on_event(&UiEvent::StateChanged(Phase::Idle)), None);
            assert!(
                matches!(model.view(), OverlayView::Error { .. }),
                "{failure:?}"
            );
            // The next dictation that pastes nothing doesn't inherit it.
            model.on_event(&UiEvent::StateChanged(Phase::Recording));
            model.on_event(&UiEvent::StateChanged(Phase::Transcribing));
            assert_eq!(
                model.on_event(&UiEvent::StateChanged(Phase::Idle)),
                shown(OverlayView::Hidden)
            );
        }
    }

    /// U4 criterion 5: a new recording replaces "Paste sent", and its timer
    /// then hides nothing.
    #[test]
    fn criterion_u4_5_a_new_recording_replaces_paste_sent() {
        let mut model = OverlayModel::default();
        for phase in [
            Phase::Recording,
            Phase::Transcribing,
            Phase::Inserting,
            Phase::Idle,
        ] {
            model.on_event(&UiEvent::StateChanged(phase));
        }
        assert_eq!(model.view(), &OverlayView::PasteSent);
        assert_eq!(model.on_error_expired(), shown(OverlayView::Hidden));

        for phase in [
            Phase::Recording,
            Phase::Transcribing,
            Phase::Inserting,
            Phase::Idle,
        ] {
            model.on_event(&UiEvent::StateChanged(phase));
        }
        assert_eq!(
            model.on_event(&UiEvent::StateChanged(Phase::Recording)),
            shown(OverlayView::Recording)
        );
        assert_eq!(model.on_error_expired(), None);
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    /// U4 criterion 4: 1.5 s of quiet while recording says "No sound" with
    /// the device's name; sound brings Listening back at once.
    #[test]
    fn criterion_u4_4_quiet_shows_no_sound_and_sound_brings_listening_back() {
        let mut model = OverlayModel::default();
        model.on_event(&UiEvent::StateChanged(Phase::Recording));
        let mic = Some("MacBook Pro Microphone");
        let mut at = 0;
        while at < 1_500 {
            assert_eq!(model.on_level(-90.0, mic, ms(at)), None, "{at} ms");
            at += 50;
        }
        assert_eq!(
            model.on_level(-90.0, mic, ms(1_500)),
            shown(OverlayView::NoSound {
                device: Some("MacBook Pro Microphone".into())
            })
        );
        assert_eq!(model.on_level(-90.0, mic, ms(1_550)), None);
        assert!(model.is_recording());
        assert_eq!(
            model.on_level(-25.0, mic, ms(1_600)),
            shown(OverlayView::Recording)
        );
        // Quiet again: another full 1.5 s.
        assert_eq!(model.on_level(-90.0, mic, ms(1_650)), None);
        assert_eq!(model.on_level(-90.0, mic, ms(3_100)), None);
        assert!(model.on_level(-90.0, mic, ms(3_150)).is_some());
    }

    #[test]
    fn criterion_u4_4_no_sound_without_a_device_name() {
        let mut model = OverlayModel::default();
        model.on_event(&UiEvent::StateChanged(Phase::Recording));
        model.on_level(-90.0, None, ms(0));
        assert_eq!(
            model.on_level(-90.0, None, ms(2_000)),
            shown(OverlayView::NoSound { device: None })
        );
        // Released while "No sound": Transcribing as usual.
        assert_eq!(
            model.on_event(&UiEvent::StateChanged(Phase::Transcribing)),
            shown(OverlayView::Transcribing)
        );
    }

    /// Levels outside a recording change nothing, and each recording
    /// starts its quiet wait afresh.
    #[test]
    fn criterion_u4_4_levels_count_only_while_recording() {
        let mut model = OverlayModel::default();
        assert_eq!(model.on_level(-90.0, None, ms(0)), None);
        assert_eq!(model.on_level(-90.0, None, ms(5_000)), None);
        assert_eq!(model.view(), &OverlayView::Hidden);

        model.on_event(&UiEvent::StateChanged(Phase::Recording));
        model.on_level(-90.0, None, ms(6_000));
        assert!(model.on_level(-90.0, None, ms(7_500)).is_some());
        model.on_event(&UiEvent::StateChanged(Phase::Idle));
        model.on_event(&UiEvent::StateChanged(Phase::Recording));
        // A new recording: the old quiet run doesn't carry over.
        assert_eq!(model.on_level(-90.0, None, ms(7_600)), None);
        assert_eq!(model.view(), &OverlayView::Recording);
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
                "It's on the clipboard: press ⌘V. If it keeps happening, check Accessibility.",
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

    /// U4 round 2: the reader gets the newest level, silence before any
    /// and after a clear.
    #[test]
    fn the_latest_level_is_the_newest_one() {
        let level = LatestLevel::new();
        assert_eq!(level.get(), LEVEL_FLOOR_DBFS);
        let writer = level.clone();
        for dbfs in [-60.0, -35.5, -12.25] {
            writer.set(dbfs);
        }
        assert_eq!(level.get(), -12.25);
        writer.clear();
        assert_eq!(level.get(), LEVEL_FLOOR_DBFS);
    }

    /// The reader never waits on the writer: a writer storing as fast as
    /// it can doesn't hold up a thousand reads, and every read is a level
    /// that was written (never a torn value).
    #[test]
    fn reading_the_level_never_blocks() {
        let level = LatestLevel::new();
        let writer = level.clone();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writing = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut n = 0_u16;
                while !stop.load(Ordering::Relaxed) {
                    writer.set(-f32::from(n % 60));
                    n = n.wrapping_add(1);
                }
            })
        };
        let started = std::time::Instant::now();
        for _ in 0..1_000 {
            let dbfs = level.get();
            assert!(
                dbfs == LEVEL_FLOOR_DBFS || (-59.0..=0.0).contains(&dbfs),
                "{dbfs}"
            );
        }
        assert!(started.elapsed() < Duration::from_secs(1));
        stop.store(true, Ordering::Relaxed);
        writing.join().unwrap();
    }

    /// Records views and levels in the order the surface got them.
    #[derive(Clone, Default)]
    struct MeterRecorder(Arc<Mutex<Vec<Shown>>>);

    #[derive(Debug, Clone, PartialEq)]
    enum Shown {
        View(OverlayView, Instant),
        Level(f32),
    }

    impl MeterRecorder {
        fn shown(&self) -> Vec<Shown> {
            self.0.lock().unwrap().clone()
        }

        fn views(&self) -> Vec<OverlayView> {
            self.shown()
                .into_iter()
                .filter_map(|shown| match shown {
                    Shown::View(view, _) => Some(view),
                    Shown::Level(_) => None,
                })
                .collect()
        }

        fn levels(&self) -> Vec<f32> {
            self.shown()
                .into_iter()
                .filter_map(|shown| match shown {
                    Shown::Level(level) => Some(level),
                    Shown::View(..) => None,
                })
                .collect()
        }
    }

    impl OverlaySurface for MeterRecorder {
        fn show(&mut self, view: &OverlayView) {
            self.0
                .lock()
                .unwrap()
                .push(Shown::View(view.clone(), Instant::now()));
        }

        fn level(&mut self, dbfs: f32) {
            self.0.lock().unwrap().push(Shown::Level(dbfs));
        }
    }

    fn reading(dbfs: f32) -> Option<crate::pipeline::LevelReading> {
        Some(crate::pipeline::LevelReading {
            dbfs,
            device: Some("Test Microphone".into()),
        })
    }

    /// U4 criterion 5: "Paste sent" shows for 800 ms after a paste, then hides.
    #[tokio::test(start_paused = true)]
    async fn criterion_u4_5_run_shows_paste_sent_for_800_ms() {
        assert_eq!(PASTE_SENT_VISIBLE_FOR, Duration::from_millis(800));
        let (tx, rx) = ui_channel();
        let recorder = Recorder::default();
        let task = tokio::spawn(run(rx, recorder.clone(), VISIBLE));
        for phase in [
            Phase::Recording,
            Phase::Transcribing,
            Phase::Inserting,
            Phase::Idle,
        ] {
            tx.send(UiEvent::StateChanged(phase));
        }
        tokio::time::sleep(VISIBLE * 2).await;
        drop(tx);
        task.await.unwrap();
        assert_eq!(
            recorder.views(),
            [
                OverlayView::Recording,
                OverlayView::Transcribing,
                OverlayView::PasteSent,
                OverlayView::Hidden
            ]
        );
        let times = recorder.times();
        assert_eq!(times[3] - times[2], PASTE_SENT_VISIBLE_FOR);
    }

    /// U4 criteria 2 and 4: levels reach the surface while recording, after
    /// the view; 1.5 s of quiet (on the paused clock) says "No sound" and
    /// sound brings Listening back; nothing reaches the meter once the
    /// recording ends.
    #[tokio::test(start_paused = true)]
    async fn criterion_u4_4_run_with_levels_meters_and_says_no_sound() {
        let (tx, rx) = ui_channel();
        let (levels_tx, levels) = tokio::sync::watch::channel(None);
        let recorder = MeterRecorder::default();
        let task = tokio::spawn(run_with_levels(rx, levels, recorder.clone(), VISIBLE));

        // A level before the recording shows is ignored.
        levels_tx.send_replace(reading(-30.0));
        tokio::time::sleep(ms(10)).await;
        tx.send(UiEvent::StateChanged(Phase::Recording));
        tokio::time::sleep(ms(10)).await;
        levels_tx.send_replace(reading(-30.0));
        tokio::time::sleep(ms(50)).await;
        // Quiet, a reading every 50 ms, for 1.6 s.
        for _ in 0..32 {
            levels_tx.send_replace(reading(-90.0));
            tokio::time::sleep(ms(50)).await;
        }
        levels_tx.send_replace(reading(-20.0));
        tokio::time::sleep(ms(50)).await;
        // Released: the pipeline clears the level.
        tx.send(UiEvent::StateChanged(Phase::Transcribing));
        levels_tx.send_replace(None);
        tokio::time::sleep(ms(50)).await;
        drop(tx);
        task.await.unwrap();

        let no_sound = OverlayView::NoSound {
            device: Some("Test Microphone".into()),
        };
        assert_eq!(
            recorder.views(),
            [
                OverlayView::Recording,
                no_sound.clone(),
                OverlayView::Recording,
                OverlayView::Transcribing
            ]
        );
        let levels = recorder.levels();
        assert_eq!(levels.len(), 34, "{levels:?}");
        assert_eq!(levels.first(), Some(&-30.0));
        assert_eq!(levels.last(), Some(&-20.0));
        // "No sound" came 1.5 s (±one reading) after the quiet began.
        let shown = recorder.shown();
        let at = |wanted: &OverlayView| {
            shown.iter().find_map(|shown| match shown {
                Shown::View(view, at) if view == wanted => Some(*at),
                _ => None,
            })
        };
        let recording = at(&OverlayView::Recording).unwrap();
        let silent = at(&no_sound).unwrap() - recording;
        assert!(
            silent >= NO_SOUND_AFTER && silent <= NO_SOUND_AFTER + ms(120),
            "{silent:?}"
        );
        // The view always comes before the level drawn on it.
        let first_view = shown
            .iter()
            .position(|shown| matches!(shown, Shown::View(..)))
            .unwrap();
        let first_level = shown
            .iter()
            .position(|shown| matches!(shown, Shown::Level(_)))
            .unwrap();
        assert!(first_view < first_level);
    }

    /// The level channel closing (the pipeline gone) doesn't stop the
    /// overlay or make it spin.
    #[tokio::test(start_paused = true)]
    async fn run_with_levels_survives_the_levels_closing() {
        let (tx, rx) = ui_channel();
        let (levels_tx, levels) = tokio::sync::watch::channel(None);
        drop(levels_tx);
        let recorder = MeterRecorder::default();
        let task = tokio::spawn(run_with_levels(rx, levels, recorder.clone(), VISIBLE));
        tx.send(UiEvent::StateChanged(Phase::Recording));
        tokio::time::sleep(ms(10)).await;
        drop(tx);
        tokio::time::timeout(VISIBLE, task).await.unwrap().unwrap();
        assert_eq!(recorder.views(), [OverlayView::Recording]);
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
