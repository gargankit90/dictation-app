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
//!
//! One more part is in `src-tauri/tauri.conf.json`: the settings window has
//! `"focus": false`, or wry activates the app when it creates the window's
//! webview, however hidden the window is (#266).

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

#[cfg(test)]
mod tests {
    /// The production part of this file (everything before the tests).
    fn production_source() -> &'static str {
        let source = include_str!("menu_bar.rs");
        source.split("#[cfg(test)]").next().unwrap_or(source)
    }

    /// #138: Tauri's `Builder` keeps the flag in a private field and its
    /// mock runtime ignores it, so the launch can't be inspected here; the
    /// behaviour is guarded by the VM scenario `e2e_overlay_focus`
    /// (`tests/e2e/README.md`). What this pins, in the gate, is that the call
    /// is in `builder` and says `false`: dropping it or flipping it, the way
    /// the bug was fixed in T2, fails here and not only in a VM run.
    #[test]
    fn the_builder_does_not_activate_the_app_at_launch() {
        let source = production_source();
        let builder_fn = source
            .split("pub fn builder")
            .nth(1)
            .and_then(|rest| rest.split("pub fn app").next())
            .expect("menu_bar::builder is in the source");
        assert!(
            builder_fn.contains(".activate_ignoring_other_apps(false)"),
            "menu_bar::builder must call activate_ignoring_other_apps(false) on macOS: \
             without it the app takes focus from the app the user is in (#59, #138)"
        );
        assert!(
            !builder_fn.contains("activate_ignoring_other_apps(true)"),
            "menu_bar::builder must not activate the app at launch"
        );
    }
}
