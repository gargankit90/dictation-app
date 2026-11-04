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
///
/// The webview takes the keyboard here, not when it is created: the window
/// is created with `"focus": false` in `tauri.conf.json`, because a focused
/// webview activates the app as it is made. That activation came through
/// about 1.5 s after launch and took the front from the app the user was in,
/// so a paste then went nowhere (#266).
pub fn show<R: Runtime>(app: &AppHandle<R>, view: View) {
    let Some(window) = app.get_webview_window(MAIN) else {
        tracing::warn!("the settings window doesn't exist");
        return;
    };
    let shown = window
        .set_title(title(view))
        .and_then(|()| window.unminimize())
        .and_then(|()| window.show())
        .and_then(|()| window.set_focus())
        .and_then(|()| window.as_ref().set_focus());
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

    /// #266: wry activates the app when it creates a focused webview, even
    /// in a hidden window, whatever `menu_bar::builder` asked for. The
    /// window starts hidden and unfocused; `show` focuses it.
    #[test]
    fn the_hidden_window_does_not_activate_the_app_at_launch() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let window = &config["app"]["windows"][0];
        assert_eq!(window["label"], MAIN);
        assert_eq!(window["visible"], false);
        assert_eq!(window["focus"], false);
    }

    /// #266: with the window created unfocused, `show` is what gives the
    /// webview the keyboard; without it, typing in Settings needs a click
    /// first.
    #[test]
    fn showing_the_window_focuses_the_window_and_then_its_webview() {
        let source = include_str!("window.rs");
        let body = source
            .split("pub fn show")
            .nth(1)
            .and_then(|rest| rest.split("/// Hides the window.").next())
            .expect("window::show is in the source");
        let window = body
            .find("window.set_focus()")
            .expect("the window is focused");
        let webview = body
            .find("window.as_ref().set_focus()")
            .expect("the webview is focused");
        assert!(window < webview, "the window first, then its webview");
    }
}
