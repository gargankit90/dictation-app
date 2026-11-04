//! The settings window (`main`): shown in front with the settings form or
//! the onboarding steps (M5.2), and hidden instead of closed, so the tray
//! can show it again.

use tauri::{AppHandle, Manager as _, Runtime};

use crate::onboarding::View;

/// The window's label in `tauri.conf.json`.
pub const MAIN: &str = "main";

/// The window's title for `view`.
#[must_use]
pub fn title(view: View) -> &'static str {
    match view {
        View::Settings => "Vocet Settings",
        View::Onboarding => "Set up Vocet",
    }
}

/// Shows the window in front, even when it's minimized or hidden, titled
/// for `view`. The page asks which view to show (`get_onboarding`) when it
/// becomes visible.
pub fn show<R: Runtime>(app: &AppHandle<R>, view: View) {
    let Some(window) = app.get_webview_window(MAIN) else {
        tracing::warn!("the settings window doesn't exist");
        return;
    };
    let shown = window
        .set_title(title(view))
        .and_then(|()| window.unminimize())
        .and_then(|()| window.show())
        .and_then(|()| window.set_focus());
    if let Err(error) = shown {
        tracing::warn!(%error, "could not show the settings window");
    }
}

/// Hides the window.
pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN) else {
        return;
    };
    if let Err(error) = window.hide() {
        tracing::warn!(%error, "could not hide the settings window");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_view_has_its_own_title() {
        assert_eq!(title(View::Settings), "Vocet Settings");
        assert_eq!(title(View::Onboarding), "Set up Vocet");
    }
}
