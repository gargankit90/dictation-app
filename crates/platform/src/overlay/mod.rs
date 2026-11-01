//! The overlay window (M5.1): a small always-on-top window at the bottom
//! centre of the screen the cursor is on, which shows the dictation state
//! and never takes focus, so the paste still lands in the app the user is
//! dictating into (docs/ENGINEERING.md, "Overlay").
//!
//! - macOS: a non-activating `NSPanel` (`tauri-nspanel`) that can't become
//!   the key or main window, is shown with `orderFrontRegardless` (never
//!   `makeKeyAndOrderFront`), floats above full-screen apps on every Space and
//!   lets clicks through.
//! - Elsewhere: a plain window that isn't focusable, stays on top and lets
//!   clicks through (Windows support comes later).
//!
//! The shell registers [`plugin`] before creating the window with
//! [`Overlay::create`].

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(not(target_os = "macos"))]
use other as imp;

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Monitor, Runtime, WebviewUrl, WebviewWindow};

/// What the overlay window looks like and where it sits.
#[derive(Debug, Clone)]
pub struct OverlaySpec {
    /// The window's label (its capability names it).
    pub label: &'static str,
    /// The page it loads, relative to the app's frontend.
    pub page: &'static str,
    /// Its size, in logical pixels (points on macOS).
    pub size: LogicalSize<f64>,
    /// The gap between its bottom edge and the bottom of the screen's work
    /// area (above the Dock or taskbar), in logical pixels.
    pub bottom_margin: f64,
}

/// The overlay's native window, read back after creation (macOS), so a
/// regression that turns the panel back into a plain window shows in the log
/// and fails the e2e check (#102 review, M1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeWindow {
    /// Its Objective-C class name.
    pub class: String,
    /// Whether it's an `NSPanel` (or a subclass).
    pub is_panel: bool,
    /// `canBecomeKeyWindow`.
    pub can_become_key: bool,
    /// `canBecomeMainWindow`.
    pub can_become_main: bool,
    /// Whether its style mask has `NSWindowStyleMaskNonactivatingPanel`.
    pub non_activating: bool,
    /// Whether the overlay's webview window is backed by this object.
    pub backs_webview: bool,
}

impl NativeWindow {
    /// Whether it's what the overlay needs: a non-activating panel that
    /// can't become the key or main window, behind the webview window.
    #[must_use]
    pub fn is_non_activating_panel(&self) -> bool {
        self.is_panel
            && !self.can_become_key
            && !self.can_become_main
            && self.non_activating
            && self.backs_webview
    }
}

/// Makes `builder`'s app launch without taking focus from the app the user
/// is in (T2). Tauri activates an app when it finishes launching, even with
/// another app active; a menu-bar dictation app that does so, at login or
/// when started again after a Quit, takes the key window away from the
/// user's document, and the next paste lands nowhere. macOS: only activate if
/// no other app is active. Elsewhere: unchanged.
#[must_use]
pub fn launch_without_focus<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    #[cfg(target_os = "macos")]
    {
        builder.activate_ignoring_other_apps(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder
    }
}

/// The plugin the overlay needs: `tauri-nspanel`'s on macOS, a no-op
/// elsewhere. Register it before calling [`Overlay::create`].
#[must_use]
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    imp::plugin()
}

/// The overlay window, created hidden.
pub struct Overlay<R: Runtime> {
    app: AppHandle<R>,
    spec: OverlaySpec,
    inner: imp::Window<R>,
}

impl<R: Runtime> Overlay<R> {
    /// Creates the overlay window, hidden. Call on the main thread (in the
    /// app's `setup`), after registering [`plugin`].
    ///
    /// # Errors
    ///
    /// If Tauri can't create the window (or, on macOS, turn it into a panel).
    pub fn create(app: &AppHandle<R>, spec: OverlaySpec) -> tauri::Result<Self> {
        let inner = imp::Window::create(app, &spec, WebviewUrl::App(spec.page.into()))?;
        Ok(Self {
            app: app.clone(),
            spec,
            inner,
        })
    }

    /// The overlay's webview window, for sending it events.
    #[must_use]
    pub fn window(&self) -> &WebviewWindow<R> {
        self.inner.window()
    }

    /// The native window behind the overlay: on macOS, read on the main
    /// thread (`None` on any other thread); elsewhere always `None`.
    #[must_use]
    pub fn native_window(&self) -> Option<NativeWindow> {
        self.inner.native()
    }

    /// Moves the overlay to the bottom centre of the screen the cursor is on
    /// and shows it, without taking focus. Safe to call from any thread.
    ///
    /// # Errors
    ///
    /// If the window can't be moved or shown.
    pub fn show(&self) -> tauri::Result<()> {
        if let Some(monitor) = self.cursor_monitor() {
            let position = bottom_centre(
                &WorkArea::of(&monitor),
                self.spec.size,
                self.spec.bottom_margin,
            );
            self.window().set_position(position)?;
        }
        self.inner.show()
    }

    /// Hides the overlay. Safe to call from any thread.
    ///
    /// # Errors
    ///
    /// If the window can't be hidden.
    pub fn hide(&self) -> tauri::Result<()> {
        self.inner.hide()
    }

    /// The monitor under the cursor, or the primary one.
    fn cursor_monitor(&self) -> Option<Monitor> {
        let under_cursor = self.app.cursor_position().ok().and_then(|cursor| {
            self.app
                .monitor_from_point(cursor.x, cursor.y)
                .ok()
                .flatten()
        });
        under_cursor.or_else(|| self.app.primary_monitor().ok().flatten())
    }
}

/// A monitor's work area (without the menu bar and Dock or taskbar), in
/// logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkArea {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl WorkArea {
    /// `monitor`'s work area. Its physical position and size are in the same
    /// global space, scaled by the monitor's own factor, so dividing by that
    /// factor gives logical pixels.
    #[must_use]
    pub fn of(monitor: &Monitor) -> Self {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        Self {
            x: f64::from(area.position.x) / scale,
            y: f64::from(area.position.y) / scale,
            width: f64::from(area.size.width) / scale,
            height: f64::from(area.size.height) / scale,
        }
    }
}

/// Where a window of `size` sits centred horizontally in `area`, its bottom
/// edge `margin` above the bottom of the area.
#[must_use]
pub fn bottom_centre(area: &WorkArea, size: LogicalSize<f64>, margin: f64) -> LogicalPosition<f64> {
    LogicalPosition::new(
        area.x + (area.width - size.width) / 2.0,
        area.y + area.height - size.height - margin,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> NativeWindow {
        NativeWindow {
            class: "RawOverlayPanel".into(),
            is_panel: true,
            can_become_key: false,
            can_become_main: false,
            non_activating: true,
            backs_webview: true,
        }
    }

    #[test]
    fn a_non_activating_panel_is_what_the_overlay_needs() {
        assert!(panel().is_non_activating_panel());
    }

    /// What `Panel::to_window` leaves behind, and every other way off.
    #[test]
    fn anything_else_is_not() {
        let window_again = NativeWindow {
            class: "TaoWindow".into(),
            is_panel: false,
            can_become_key: true,
            non_activating: false,
            ..panel()
        };
        assert!(!window_again.is_non_activating_panel());
        for broken in [
            NativeWindow {
                is_panel: false,
                ..panel()
            },
            NativeWindow {
                can_become_key: true,
                ..panel()
            },
            NativeWindow {
                can_become_main: true,
                ..panel()
            },
            NativeWindow {
                non_activating: false,
                ..panel()
            },
            NativeWindow {
                backs_webview: false,
                ..panel()
            },
        ] {
            assert!(!broken.is_non_activating_panel(), "{broken:?}");
        }
    }

    #[test]
    fn bottom_centre_of_a_monitor_at_the_origin() {
        let area = WorkArea {
            x: 0.0,
            y: 25.0,
            width: 1440.0,
            height: 875.0,
        };
        let position = bottom_centre(&area, LogicalSize::new(400.0, 80.0), 24.0);
        assert_eq!(position, LogicalPosition::new(520.0, 796.0));
    }

    #[test]
    fn bottom_centre_of_a_second_monitor_includes_its_offset() {
        let area = WorkArea {
            x: -1920.0,
            y: -200.0,
            width: 1920.0,
            height: 1080.0,
        };
        let position = bottom_centre(&area, LogicalSize::new(400.0, 80.0), 0.0);
        assert_eq!(position, LogicalPosition::new(-1160.0, 800.0));
    }
}
