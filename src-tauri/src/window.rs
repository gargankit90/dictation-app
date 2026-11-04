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
    if window.title()? == title(view) {
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
pub fn show<R: Runtime>(app: &AppHandle<R>, view: View) {
    let Some(window) = app.get_webview_window(MAIN) else {
        tracing::warn!("the settings window doesn't exist");
        return;
    };
    if let Err(error) = fit(&window, view) {
        tracing::warn!(%error, "could not size the settings window");
    }
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
}
