//! The overlay window (M5.1, U4): the recording pill, which shows
//! Listening with a live level meter, "No sound", Transcribing, "Pasted"
//! and errors without taking focus. [`start`] creates the window
//! (`dictation_platform::overlay`) and runs `dictation_pipeline::overlay`
//! against it; this module only connects the two and sends each view to the
//! page (`src/overlay/`) as [`EVENT`], and each input level as
//! [`LEVEL_EVENT`] (about 20 a second while recording; never logged).
//!
//! An event sent before the page's listener is registered is dropped (Tauri
//! only delivers to registered listeners), so a dictation started right
//! after launch showed an empty overlay (#119). The page can't call
//! commands or emit events, so once it listens it says so through its
//! document title ([`LISTENING_TITLE`]), and the [`Relay`] sends it the
//! current view again.

use std::sync::{Arc, Mutex, PoisonError};

use dictation_pipeline::overlay::{self, ERROR_VISIBLE_FOR, OverlaySurface, OverlayView};
use dictation_pipeline::pipeline::LevelReceiver;
use dictation_pipeline::ui_events::{UiSender, ui_channel};
use dictation_platform::overlay::{Overlay, OverlaySpec};
use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, LogicalSize, Runtime};

/// The overlay window's label; `capabilities/overlay.json` names it.
pub const LABEL: &str = "overlay";
/// The one event the overlay page listens to (`src/overlay/view.ts`).
pub const EVENT: &str = "overlay-view";
/// The input level for the pill's meter while recording (U4):
/// `{"dbfs": -32.5}` (`OVERLAY_LEVEL_EVENT` in `src/overlay/view.ts`).
pub const LEVEL_EVENT: &str = "overlay-level";
/// The start of the document title the overlay page sets once its listener
/// is registered (`LISTENING_TITLE` in `src/overlay/view.ts`; a count
/// follows, so each new listener changes the title).
pub const LISTENING_TITLE: &str = "dictation-app overlay listening";

fn spec() -> OverlaySpec {
    OverlaySpec {
        label: LABEL,
        page: "overlay.html",
        // Room for a two-line error; the window is transparent around the
        // pill the page draws.
        size: LogicalSize::new(420.0, 72.0),
        bottom_margin: 16.0,
    }
}

/// Creates the overlay window, hidden, and starts showing the pipeline's
/// events and input `levels` on it. Returns where to send those events. Call on the main
/// thread, inside the Tauri runtime, after registering
/// `dictation_platform::overlay::plugin`.
///
/// # Errors
///
/// If the window can't be created.
pub fn start<R: Runtime>(app: &AppHandle<R>, levels: LevelReceiver) -> tauri::Result<UiSender> {
    let relay = Relay::default();
    let on_title = {
        let app = app.clone();
        let relay = relay.clone();
        Box::new(move |title: String| {
            relay.page_title(&title, |view| {
                tracing::info!(
                    state = Payload::from(view).name(),
                    "overlay page listening; view sent again"
                );
                send(&app, view);
            });
        })
    };
    let window = Overlay::create(app, spec(), on_title)?;
    log_native_window(&window);
    // Bounded, like the main UI's queue (#112).
    let (events, receiver) = ui_channel();
    tauri::async_runtime::spawn(overlay::run_with_levels(
        receiver,
        levels,
        Surface {
            app: app.clone(),
            window,
            relay,
        },
        ERROR_VISIBLE_FOR,
    ));
    Ok(events)
}

/// Logs what the native window is (macOS, read on the main thread), so a
/// change that turns the panel back into a plain window shows: the e2e
/// scenario `e2e_overlay_focus` checks for `non_activating_panel=true`.
fn log_native_window<R: Runtime>(window: &Overlay<R>) {
    let Some(native) = window.native_window() else {
        return;
    };
    let non_activating_panel = native.is_non_activating_panel();
    let fields = format!(
        "class={} is_panel={} can_become_key={} can_become_main={} non_activating={} backs_webview={}",
        native.class,
        native.is_panel,
        native.can_become_key,
        native.can_become_main,
        native.non_activating,
        native.backs_webview
    );
    if non_activating_panel {
        tracing::info!(non_activating_panel, native = %fields, "overlay native window");
    } else {
        tracing::warn!(
            non_activating_panel,
            native = %fields,
            "overlay native window: not a non-activating panel, so it may take focus"
        );
    }
}

/// Sends `view` to the overlay page.
fn send<R: Runtime>(app: &AppHandle<R>, view: &OverlayView) {
    if let Err(error) = app.emit_to(
        EventTarget::webview_window(LABEL),
        EVENT,
        Payload::from(view),
    ) {
        tracing::warn!(%error, "could not update the overlay");
    }
}

/// The view last sent to the page, sent again when the page says it has
/// started listening. One lock around storing and sending (and around
/// re-sending), so a re-send can't overtake a newer view and leave the
/// page on an old one.
#[derive(Clone, Default)]
struct Relay(Arc<Mutex<OverlayView>>);

impl Relay {
    /// Remembers `view` and sends it with `send`.
    fn send(&self, view: &OverlayView, send: impl FnOnce(&OverlayView)) {
        let mut current = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        current.clone_from(view);
        send(&current);
    }

    /// The page's document title changed to `title`: if it says the page
    /// listens now, sends it the current view with `send`. Returns whether
    /// it did.
    fn page_title(&self, title: &str, send: impl FnOnce(&OverlayView)) -> bool {
        if !title.starts_with(LISTENING_TITLE) {
            return false;
        }
        let current = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        send(&current);
        true
    }
}

/// The overlay window as `dictation_pipeline::overlay` sees it.
struct Surface<R: Runtime> {
    app: AppHandle<R>,
    window: Overlay<R>,
    relay: Relay,
}

impl<R: Runtime> OverlaySurface for Surface<R> {
    /// The meter's level. Not logged: 20 a second, and a level stays out
    /// of the logs at info level (privacy rule).
    fn level(&mut self, dbfs: f32) {
        if let Err(error) = self.app.emit_to(
            EventTarget::webview_window(LABEL),
            LEVEL_EVENT,
            LevelPayload { dbfs },
        ) {
            tracing::debug!(%error, "could not update the overlay's meter");
        }
    }

    fn show(&mut self, view: &OverlayView) {
        // The page first, so the window never shows the previous state.
        self.relay.send(view, |view| send(&self.app, view));
        // The state's name only, never an error's text.
        let shown = match view {
            OverlayView::Hidden => {
                tracing::info!("overlay hidden");
                self.window.hide()
            }
            _ => {
                tracing::info!(state = Payload::from(view).name(), "overlay shown");
                self.window.show()
            }
        };
        if let Err(error) = shown {
            tracing::warn!(%error, "could not show or hide the overlay");
        }
    }
}

/// [`LEVEL_EVENT`]'s payload.
#[derive(Debug, Clone, Copy, Serialize)]
struct LevelPayload {
    dbfs: f32,
}

/// [`EVENT`]'s payload: `OverlayView` in `src/overlay/view.ts`. An error
/// carries only its user-facing message, no code (U4 criterion 6).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Payload<'a> {
    Hidden,
    Recording,
    NoSound {
        #[serde(skip_serializing_if = "Option::is_none")]
        device: Option<&'a str>,
    },
    Transcribing,
    Pasted,
    Error {
        message: &'a str,
    },
}

impl<'a> From<&'a OverlayView> for Payload<'a> {
    fn from(view: &'a OverlayView) -> Self {
        match view {
            OverlayView::Hidden => Self::Hidden,
            OverlayView::Recording => Self::Recording,
            OverlayView::NoSound { device } => Self::NoSound {
                device: device.as_deref(),
            },
            OverlayView::Transcribing => Self::Transcribing,
            OverlayView::Pasted => Self::Pasted,
            OverlayView::Error { message, .. } => Self::Error { message },
        }
    }
}

impl Payload<'_> {
    /// The `state` it's sent with, for the log.
    fn name(&self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Recording => "recording",
            Self::NoSound { .. } => "no_sound",
            Self::Transcribing => "transcribing",
            Self::Pasted => "pasted",
            Self::Error { .. } => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title_sends(relay: &Relay, title: &str) -> (bool, Vec<OverlayView>) {
        let mut sent = Vec::new();
        let did = relay.page_title(title, |view| sent.push(view.clone()));
        (did, sent)
    }

    /// #119: a page that starts listening late still gets the current view.
    #[test]
    fn a_listening_page_gets_the_current_view_again() {
        let relay = Relay::default();
        let mut sent = Vec::new();
        relay.send(&OverlayView::Recording, |view| sent.push(view.clone()));
        assert_eq!(sent, [OverlayView::Recording]);

        let (did, again) = title_sends(&relay, &format!("{LISTENING_TITLE} 1"));
        assert!(did);
        assert_eq!(again, [OverlayView::Recording]);

        // The latest view, not the first one.
        let error = OverlayView::Error {
            message: "Microphone permission is missing.".to_owned(),
            code: dictation_core::diagnostics::ErrorCode::MicDenied,
        };
        relay.send(&error, |_| {});
        let (_, again) = title_sends(&relay, &format!("{LISTENING_TITLE} 2"));
        assert_eq!(again, [error]);
    }

    /// #155 with #119: once the restored hotkey hides its error, a page
    /// that starts listening after that gets Hidden, not the stale error.
    #[test]
    fn a_page_listening_after_the_hotkey_is_restored_gets_hidden() {
        use dictation_core::error::DictationError;
        use dictation_pipeline::overlay::OverlayModel;
        use dictation_pipeline::pipeline::UiEvent;

        let mut model = OverlayModel::default();
        let relay = Relay::default();
        let mut sent = Vec::new();
        for event in [
            UiEvent::Error(DictationError::HotkeyFailed),
            UiEvent::HotkeyRestored,
        ] {
            let change = model.on_event(&event).expect("a change");
            relay.send(&change.view, |view| sent.push(view.clone()));
        }
        assert_eq!(sent.last(), Some(&OverlayView::Hidden));

        let (did, again) = title_sends(&relay, &format!("{LISTENING_TITLE} 1"));
        assert!(did);
        assert_eq!(again, [OverlayView::Hidden]);
    }

    #[test]
    fn before_any_view_a_listening_page_gets_hidden() {
        let (did, sent) = title_sends(&Relay::default(), &format!("{LISTENING_TITLE} 1"));
        assert!(did);
        assert_eq!(sent, [OverlayView::Hidden]);
    }

    #[test]
    fn other_titles_send_nothing() {
        let relay = Relay::default();
        relay.send(&OverlayView::Transcribing, |_| {});
        for title in ["", "dictation-app overlay", "overlay", "listening"] {
            let (did, sent) = title_sends(&relay, title);
            assert!(!did, "{title}");
            assert!(sent.is_empty(), "{title}");
        }
    }

    /// The page sets this title (`src/overlay/view.ts`, pinned there too).
    #[test]
    fn the_listening_title_matches_the_page() {
        assert_eq!(LISTENING_TITLE, "dictation-app overlay listening");
    }

    /// The JSON the page's `parseView` reads (`src/overlay/Overlay.test.tsx`).
    #[test]
    fn payloads_match_the_page() {
        let error = OverlayView::Error {
            message: "No speech was recognised.".to_owned(),
            code: dictation_core::diagnostics::ErrorCode::NoSpeech,
        };
        let cases = [
            (OverlayView::Hidden, r#"{"state":"hidden"}"#),
            (OverlayView::Recording, r#"{"state":"recording"}"#),
            (OverlayView::Transcribing, r#"{"state":"transcribing"}"#),
            (OverlayView::Pasted, r#"{"state":"pasted"}"#),
            (
                OverlayView::NoSound { device: None },
                r#"{"state":"no_sound"}"#,
            ),
            (
                OverlayView::NoSound {
                    device: Some("MacBook Pro Microphone".into()),
                },
                r#"{"state":"no_sound","device":"MacBook Pro Microphone"}"#,
            ),
            // U4 criterion 6: the message only, no code.
            (
                error,
                r#"{"state":"error","message":"No speech was recognised."}"#,
            ),
        ];
        for (view, json) in cases {
            let payload = Payload::from(&view);
            assert_eq!(serde_json::to_string(&payload).unwrap(), json, "{view:?}");
            // The log names the state the page gets.
            assert!(json.starts_with(&format!(r#"{{"state":"{}""#, payload.name())));
        }
    }

    /// The level's JSON (`parseLevel` in `src/overlay/view.ts`).
    #[test]
    fn the_level_payload_matches_the_page() {
        assert_eq!(LEVEL_EVENT, "overlay-level");
        assert_eq!(
            serde_json::to_string(&LevelPayload { dbfs: -32.5 }).unwrap(),
            r#"{"dbfs":-32.5}"#
        );
    }
}
