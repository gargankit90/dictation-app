//! Launching as a menu-bar app: no Dock icon, no app menu, and no focus
//! taken from the app the user is in (#59).
//!
//! `LSUIElement` in the bundle's `Info.plist` isn't enough on its own: tao
//! applies the activation policy it was given in
//! `applicationDidFinishLaunching:` (Regular by default, and it activates
//! the app), so a policy set later, in Tauri's `setup`, demotes an app
//! that was already Regular and active: the Dock icon flashes. Setting
//! Accessory on the [`App`] before [`App::run`] hands it to tao before
//! launch finishes, so the app is never Regular. The two halves:
//! [`builder`] before `build`, [`app`] between `build` and `run`.

use tauri::{App, Builder, Runtime};

/// Don't activate at launch while another app is active (macOS).
#[must_use]
pub fn builder<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    #[cfg(target_os = "macos")]
    {
        builder.activate_ignoring_other_apps(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder
    }
}

/// The Accessory activation policy (macOS). Call after `build`, before
/// `run`: once the event loop runs it is a runtime demotion again.
pub fn app<R: Runtime>(app: &mut App<R>) {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}
