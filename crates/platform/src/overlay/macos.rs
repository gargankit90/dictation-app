//! macOS: the overlay is a non-activating `NSPanel` (`tauri-nspanel`).
//!
//! No `unsafe` here: `tauri-nspanel` wraps the Objective-C calls. It must
//! run them on the main thread, so [`Window::show`] and [`Window::hide`]
//! hand them to Tauri's main-thread queue.

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Runtime, WebviewUrl, WebviewWindow};
use tauri_nspanel::{
    CollectionBehavior, PanelBuilder, PanelHandle, PanelLevel, StyleMask, tauri_panel,
};

use super::OverlaySpec;

tauri_panel! {
    panel!(OverlayPanel {
        config: {
            // Never the key window, so keystrokes (and the paste) keep going
            // to the app the user is dictating into; never the main window.
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

pub(super) fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri_nspanel::init()
}

pub(super) struct Window<R: Runtime> {
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    panel: PanelHandle<R>,
}

impl<R: Runtime> Window<R> {
    pub(super) fn create(
        app: &AppHandle<R>,
        spec: &OverlaySpec,
        url: WebviewUrl,
    ) -> tauri::Result<Self> {
        let panel = PanelBuilder::<R, OverlayPanel<R>>::new(app, spec.label)
            .url(url)
            .title(spec.label)
            .size(tauri::Size::Logical(spec.size))
            .resizable(false)
            // Above normal and floating windows, like other status HUDs.
            .level(PanelLevel::Status)
            .has_shadow(false)
            .transparent(true)
            .opaque(false)
            // Clicks pass through to the app underneath.
            .ignores_mouse_events(true)
            .hides_on_deactivate(false)
            // Creating the window doesn't activate the app.
            .no_activate(true)
            .style_mask(StyleMask::empty().borderless().nonactivating_panel())
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary()
                    .ignores_cycle(),
            )
            .with_window(|window| {
                window
                    .decorations(false)
                    .transparent(true)
                    .focused(false)
                    .focusable(false)
                    .skip_taskbar(true)
                    .visible(false)
            })
            .build()?;
        let window = panel.to_window().ok_or(tauri::Error::WindowNotFound)?;
        Ok(Self {
            app: app.clone(),
            window,
            panel,
        })
    }

    pub(super) fn window(&self) -> &WebviewWindow<R> {
        &self.window
    }

    pub(super) fn show(&self) -> tauri::Result<()> {
        let panel = self.panel.clone();
        // `orderFrontRegardless`: shows the panel without making it key or
        // activating the app.
        self.app.run_on_main_thread(move || panel.show())
    }

    pub(super) fn hide(&self) -> tauri::Result<()> {
        let panel = self.panel.clone();
        self.app.run_on_main_thread(move || panel.hide())
    }
}
