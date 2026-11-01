//! macOS: the overlay is a non-activating `NSPanel` (`tauri-nspanel`).
//!
//! No `unsafe` here: `tauri-nspanel` wraps the Objective-C calls. It must
//! run them on the main thread, so [`Window::show`] and [`Window::hide`]
//! hand them to Tauri's main-thread queue.
//!
//! The webview window comes from Tauri's window list, never from
//! `Panel::to_window`: that converts the panel back into a plain window (it
//! unregisters the panel and restores the window's original class), which
//! would undo the panel on every creation (#102 review, M1).
//! [`Window::native`] reads the native window back so the shell can log
//! (and the e2e scenario check) that it's still the panel.

use objc2::ClassType as _;
use objc2::runtime::NSObjectProtocol as _;
use objc2_app_kit::NSWindowStyleMask;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager as _, Runtime, WebviewUrl, WebviewWindow};
use tauri_nspanel::{
    CollectionBehavior, PanelBuilder, PanelHandle, PanelLevel, StyleMask, tauri_panel,
};

use super::{NativeWindow, OverlaySpec};

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
        // Not `panel.to_window()`, which turns the panel back into a window.
        let window = app
            .get_webview_window(spec.label)
            .ok_or(tauri::Error::WindowNotFound)?;
        Ok(Self {
            app: app.clone(),
            window,
            panel,
        })
    }

    pub(super) fn window(&self) -> &WebviewWindow<R> {
        &self.window
    }

    /// The native window behind the overlay, read on the main thread
    /// (`None` on any other thread).
    pub(super) fn native(&self) -> Option<NativeWindow> {
        objc2_foundation::MainThreadMarker::new()?;
        let panel: &objc2_app_kit::NSPanel = self.panel.as_panel();
        // The webview window must be backed by this very object.
        let backs_webview = self.window.ns_window().is_ok_and(|ns_window| {
            std::ptr::eq(ns_window.cast_const(), std::ptr::from_ref(panel).cast())
        });
        Some(NativeWindow {
            class: panel.class().name().to_string_lossy().into_owned(),
            is_panel: panel.isKindOfClass(objc2_app_kit::NSPanel::class()),
            can_become_key: panel.canBecomeKeyWindow(),
            can_become_main: panel.canBecomeMainWindow(),
            non_activating: panel
                .styleMask()
                .contains(NSWindowStyleMask::NonactivatingPanel),
            backs_webview,
        })
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
