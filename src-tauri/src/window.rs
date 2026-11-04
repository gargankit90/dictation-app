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
        View::Settings => "dictation-app Settings",
        View::Onboarding => "Set up dictation-app",
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
        assert_eq!(title(View::Settings), "dictation-app Settings");
        assert_eq!(title(View::Onboarding), "Set up dictation-app");
    }

    /// U6 criterion 1: Settings opens at about 720×520, resizable down to
    /// 640×460.
    #[test]
    fn criterion_u6_1_the_window_opens_at_720_by_520() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let window = &config["app"]["windows"][0];
        assert_eq!(window["label"], MAIN);
        assert_eq!(
            (window["width"].as_u64(), window["height"].as_u64()),
            (Some(720), Some(520))
        );
        assert_eq!(
            (window["minWidth"].as_u64(), window["minHeight"].as_u64()),
            (Some(640), Some(460))
        );
        assert_eq!(window["resizable"], true);
    }
}
