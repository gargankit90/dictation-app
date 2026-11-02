//! The overlay window (M5.1): shows Recording, Transcribing and errors
//! without taking focus. [`start`] creates the window
//! (`dictation_platform::overlay`) and runs `dictation_pipeline::overlay`
//! against it; this module only connects the two and sends each view to the
//! page (`src/overlay/`) as [`EVENT`].

use dictation_pipeline::overlay::{self, ERROR_VISIBLE_FOR, OverlaySurface, OverlayView};
use dictation_pipeline::ui_events::{UiSender, ui_channel};
use dictation_platform::overlay::{Overlay, OverlaySpec};
use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, LogicalSize, Runtime};

/// The overlay window's label; `capabilities/overlay.json` names it.
pub const LABEL: &str = "overlay";
/// The one event the overlay page listens to (`src/overlay/view.ts`).
pub const EVENT: &str = "overlay-view";

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
/// events on it. Returns where to send those events. Call on the main
/// thread, inside the Tauri runtime, after registering
/// `dictation_platform::overlay::plugin`.
///
/// # Errors
///
/// If the window can't be created.
pub fn start<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<UiSender> {
    let window = Overlay::create(app, spec())?;
    log_native_window(&window);
    // Bounded, like the main UI's queue (#112).
    let (events, receiver) = ui_channel();
    tauri::async_runtime::spawn(overlay::run(
        receiver,
        Surface {
            app: app.clone(),
            window,
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

/// The overlay window as `dictation_pipeline::overlay` sees it.
struct Surface<R: Runtime> {
    app: AppHandle<R>,
    window: Overlay<R>,
}

impl<R: Runtime> OverlaySurface for Surface<R> {
    fn show(&mut self, view: &OverlayView) {
        // The page first, so the window never shows the previous state.
        if let Err(error) = self.app.emit_to(
            EventTarget::webview_window(LABEL),
            EVENT,
            Payload::from(view),
        ) {
            tracing::warn!(%error, "could not update the overlay");
        }
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

/// [`EVENT`]'s payload: `OverlayView` in `src/overlay/view.ts`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
enum Payload<'a> {
    Hidden,
    Recording,
    Transcribing,
    Error { message: &'a str },
}

impl<'a> From<&'a OverlayView> for Payload<'a> {
    fn from(view: &'a OverlayView) -> Self {
        match view {
            OverlayView::Hidden => Self::Hidden,
            OverlayView::Recording => Self::Recording,
            OverlayView::Transcribing => Self::Transcribing,
            OverlayView::Error { message } => Self::Error { message },
        }
    }
}

impl Payload<'_> {
    /// The `state` it's sent with, for the log.
    fn name(&self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Recording => "recording",
            Self::Transcribing => "transcribing",
            Self::Error { .. } => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The JSON the page's `parseView` reads (`src/overlay/Overlay.test.tsx`).
    #[test]
    fn payloads_match_the_page() {
        let error = OverlayView::Error {
            message: "No speech was recognised.".to_owned(),
        };
        let cases = [
            (OverlayView::Hidden, r#"{"state":"hidden"}"#),
            (OverlayView::Recording, r#"{"state":"recording"}"#),
            (OverlayView::Transcribing, r#"{"state":"transcribing"}"#),
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
}
