//! Not macOS: a plain window that isn't focusable, stays on top and lets
//! clicks through. Windows gets a proper no-activate overlay later (M-W).

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{AppHandle, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::{NativeWindow, OverlaySpec};

pub(super) fn plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("dictation-overlay").build()
}

pub(super) struct Window<R: Runtime> {
    window: WebviewWindow<R>,
}

impl<R: Runtime> Window<R> {
    pub(super) fn create(
        app: &AppHandle<R>,
        spec: &OverlaySpec,
        url: WebviewUrl,
    ) -> tauri::Result<Self> {
        let window = WebviewWindowBuilder::new(app, spec.label, url)
            .title(spec.label)
            .inner_size(spec.size.width, spec.size.height)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .visible(false)
            .build()?;
        window.set_ignore_cursor_events(true)?;
        Ok(Self { window })
    }

    pub(super) fn window(&self) -> &WebviewWindow<R> {
        &self.window
    }

    /// Nothing to read back: there's no panel here.
    #[allow(clippy::unused_self)] // Same signature as the macOS one.
    pub(super) fn native(&self) -> Option<NativeWindow> {
        None
    }

    pub(super) fn show(&self) -> tauri::Result<()> {
        self.window.show()
    }

    pub(super) fn hide(&self) -> tauri::Result<()> {
        self.window.hide()
    }
}
