//! The settings window (`main`): shown in front with the settings form or
//! the onboarding steps (M5.2), and hidden instead of closed, so the tray
//! can show it again.

use tauri::{AppHandle, LogicalSize, Manager as _, Runtime};

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

/// Which Settings screen the window shows (U12): each has its own title, and
/// the page says when it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum WindowPage {
    /// The sidebar and its sections.
    Settings,
    /// Troubleshoot's checks.
    Troubleshoot,
    /// Report a problem.
    Report,
}

/// The window's title on `page`.
#[must_use]
pub const fn page_title(page: WindowPage) -> &'static str {
    match page {
        WindowPage::Settings => "Vocet Settings",
        WindowPage::Troubleshoot => "Vocet Troubleshoot",
        WindowPage::Report => "Vocet Report a Problem",
    }
}

/// Whether a window titled `current` is already on `view`: every title but
/// setup's is the Settings view.
#[must_use]
fn on_view(current: &str, view: View) -> bool {
    (current == title(View::Onboarding)) == (view == View::Onboarding)
}

/// The title to show `view` with: a window staying on its view keeps the one
/// its page gave it (Settings or Troubleshoot).
#[must_use]
fn title_for(current: &str, view: View) -> &str {
    if on_view(current, view) {
        current
    } else {
        title(view)
    }
}

/// The page says which screen it shows. Setup's title stays while setup shows.
///
/// # Errors
///
/// If the window can't be read or retitled.
pub fn set_page<R: Runtime>(app: &AppHandle<R>, page: WindowPage) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(MAIN) else {
        return Ok(());
    };
    if on_view(&window.title()?, View::Onboarding) {
        return Ok(());
    }
    window.set_title(page_title(page))
}

/// The window's size for `view`, and the smallest it may get, in points:
/// `tauri.conf.json`'s for Settings, U8's for onboarding.
#[must_use]
pub fn sizes(view: View) -> ((f64, f64), (f64, f64)) {
    match view {
        View::Settings => ((720.0, 520.0), (640.0, 460.0)),
        View::Onboarding => ((740.0, 560.0), (680.0, 520.0)),
    }
}

/// When the window changes view, gives it that view's size. A window that
/// stays on its view keeps the size the user gave it.
fn fit<R: Runtime>(window: &tauri::WebviewWindow<R>, view: View) -> tauri::Result<()> {
    if on_view(&window.title()?, view) {
        return Ok(());
    }
    let ((width, height), (min_width, min_height)) = sizes(view);
    // The smallest first: Settings' size is below onboarding's minimum.
    window.set_min_size(Some(LogicalSize::new(min_width, min_height)))?;
    window.set_size(LogicalSize::new(width, height))?;
    window.center()
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
    if let Err(error) = fit(&window, view) {
        tracing::warn!(%error, "could not size the settings window");
    }
    let current = window.title().unwrap_or_default();
    let shown = window
        .set_title(title_for(&current, view))
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

    /// U12 criterion 2: each screen's title.
    #[test]
    fn u12_criterion_2_each_page_has_its_own_title() {
        assert_eq!(page_title(WindowPage::Settings), "Vocet Settings");
        assert_eq!(page_title(WindowPage::Troubleshoot), "Vocet Troubleshoot");
        assert_eq!(page_title(WindowPage::Report), "Vocet Report a Problem");
        assert_eq!(page_title(WindowPage::Settings), title(View::Settings));
    }

    /// Troubleshoot is still the Settings view: showing the window again
    /// keeps its size and title, and setup replaces them.
    #[test]
    fn u12_criterion_2_a_screen_of_settings_stays_on_its_view() {
        let troubleshoot = page_title(WindowPage::Troubleshoot);
        assert!(on_view(troubleshoot, View::Settings));
        assert!(!on_view(troubleshoot, View::Onboarding));
        assert!(on_view("Set up Vocet", View::Onboarding));
        assert!(!on_view("Set up Vocet", View::Settings));
        assert_eq!(title_for(troubleshoot, View::Settings), troubleshoot);
        assert_eq!(title_for(troubleshoot, View::Onboarding), "Set up Vocet");
        assert_eq!(title_for("Set up Vocet", View::Settings), "Vocet Settings");
    }

    /// U8 criterion 1.
    #[test]
    fn onboarding_is_740_by_560_and_no_smaller_than_680_by_520() {
        assert_eq!(sizes(View::Onboarding), ((740.0, 560.0), (680.0, 520.0)));
    }

    #[test]
    fn settings_keeps_the_size_in_the_tauri_config() {
        let config = std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json"),
        )
        .unwrap();
        let config: serde_json::Value = serde_json::from_str(&config).unwrap();
        let main = &config["app"]["windows"][0];
        let ((width, height), (min_width, min_height)) = sizes(View::Settings);
        assert_eq!(main["width"].as_f64(), Some(width));
        assert_eq!(main["height"].as_f64(), Some(height));
        assert_eq!(main["minWidth"].as_f64(), Some(min_width));
        assert_eq!(main["minHeight"].as_f64(), Some(min_height));
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
