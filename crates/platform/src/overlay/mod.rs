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
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Monitor, PhysicalRect, Runtime, WebviewUrl,
    WebviewWindow,
};

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

/// The plugin the overlay needs: `tauri-nspanel`'s on macOS, a no-op
/// elsewhere. Register it before calling [`Overlay::create`].
#[must_use]
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    imp::plugin()
}

/// Called with the overlay page's document title whenever it changes. The
/// page has no commands and can't emit events (its capability only lets it
/// listen), so its title is the one thing it can tell the app: the shell
/// uses it to learn when the page's event listener is registered (#119).
pub type OnTitle = Box<dyn Fn(String) + Send + 'static>;

/// The overlay window, created hidden.
pub struct Overlay<R: Runtime> {
    app: AppHandle<R>,
    spec: OverlaySpec,
    inner: imp::Window<R>,
}

impl<R: Runtime> Overlay<R> {
    /// Creates the overlay window, hidden. Call on the main thread (in the
    /// app's `setup`), after registering [`plugin`]. `on_title` gets the
    /// page's document title each time it changes ([`OnTitle`]).
    ///
    /// # Errors
    ///
    /// If Tauri can't create the window (or, on macOS, turn it into a panel).
    pub fn create(app: &AppHandle<R>, spec: OverlaySpec, on_title: OnTitle) -> tauri::Result<Self> {
        let inner = imp::Window::create(app, &spec, WebviewUrl::App(spec.page.into()), on_title)?;
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
        let position = self.cursor_monitor().map(|monitor| {
            bottom_centre(
                &WorkArea::of(&monitor),
                self.spec.size,
                self.spec.bottom_margin,
            )
        });
        place_and_show(
            position,
            |position| self.window().set_position(position),
            || self.inner.show(),
        )
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
        pick_monitor(
            || self.app.cursor_position(),
            |cursor| self.app.monitor_from_point(cursor.x, cursor.y),
            || self.app.primary_monitor(),
        )
    }
}

/// The monitor under the cursor; if the cursor or its monitor can't be
/// found, the primary one; `None` if there is none either.
fn pick_monitor<P, M, E>(
    cursor: impl FnOnce() -> Result<P, E>,
    monitor_at: impl FnOnce(P) -> Result<Option<M>, E>,
    primary: impl FnOnce() -> Result<Option<M>, E>,
) -> Option<M> {
    cursor()
        .ok()
        .and_then(|point| monitor_at(point).ok().flatten())
        .or_else(|| primary().ok().flatten())
}

/// Moves the window to `position`, if there is one, then shows it: without
/// a monitor to place it on, it is still shown where it was.
fn place_and_show<E>(
    position: Option<LogicalPosition<f64>>,
    move_to: impl FnOnce(LogicalPosition<f64>) -> Result<(), E>,
    show: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if let Some(position) = position {
        move_to(position)?;
    }
    show()
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
        Self::from_physical(monitor.work_area(), monitor.scale_factor())
    }

    /// A physical work area at `scale`, in logical pixels.
    #[must_use]
    pub fn from_physical(area: &PhysicalRect<i32, u32>, scale: f64) -> Self {
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

    fn physical(x: i32, y: i32, width: u32, height: u32) -> PhysicalRect<i32, u32> {
        PhysicalRect {
            position: tauri::PhysicalPosition::new(x, y),
            size: tauri::PhysicalSize::new(width, height),
        }
    }

    const PILL: LogicalSize<f64> = LogicalSize::new(400.0, 80.0);

    // The expected values below are worked out by hand (physical / scale),
    // not with `WorkArea::from_physical`, so they are an independent oracle.

    #[test]
    fn physical_work_area_converts_offsets_and_size_at_2x() {
        // A Retina display to the right of a 1440-point-wide one, below a
        // 25-point menu bar: physical (2880, 50), 2880 x 1750 at 2x.
        let area = WorkArea::from_physical(&physical(2880, 50, 2880, 1750), 2.0);
        assert_eq!(
            area,
            WorkArea {
                x: 1440.0,
                y: 25.0,
                width: 1440.0,
                height: 875.0,
            }
        );
        // 1440 + (1440 - 400) / 2, and 25 + 875 - 80 - 24.
        assert_eq!(
            bottom_centre(&area, PILL, 24.0),
            LogicalPosition::new(1960.0, 796.0)
        );
    }

    #[test]
    fn physical_work_areas_at_other_scales_and_origins() {
        // (physical x, y, width, height, scale) => logical (x, y, width, height)
        let cases = [
            // Fractional scale, to the right.
            ((2160, 0, 2880, 1620, 1.5), (1440.0, 0.0, 1920.0, 1080.0)),
            // Fractional scale, left of and above the primary display.
            (
                (-2400, -250, 2400, 1350, 1.25),
                (-1920.0, -200.0, 1920.0, 1080.0),
            ),
            // A 2x display left of and above the primary one.
            (
                (-3024, -100, 3024, 1964, 2.0),
                (-1512.0, -50.0, 1512.0, 982.0),
            ),
            // A 1x display with a negative origin.
            (
                (-1920, -1080, 1920, 1055, 1.0),
                (-1920.0, -1080.0, 1920.0, 1055.0),
            ),
        ];
        for ((x, y, width, height, scale), (lx, ly, lw, lh)) in cases {
            let area = WorkArea::from_physical(&physical(x, y, width, height), scale);
            assert_eq!(
                area,
                WorkArea {
                    x: lx,
                    y: ly,
                    width: lw,
                    height: lh,
                },
                "{x}, {y}, {width} x {height} at {scale}"
            );
        }
    }

    /// A display as macOS reports it: its physical rects are its logical
    /// ones times its own scale.
    struct Screen {
        name: &'static str,
        bounds: PhysicalRect<i32, u32>,
        work_area: PhysicalRect<i32, u32>,
        scale: f64,
    }

    /// A built-in Retina display (primary, 2x) with a 1x external display to
    /// its left and higher up.
    fn mixed_screens() -> [Screen; 2] {
        [
            Screen {
                name: "built-in",
                bounds: physical(0, 0, 2880, 1800),
                work_area: physical(0, 50, 2880, 1750),
                scale: 2.0,
            },
            Screen {
                name: "external",
                bounds: physical(-1920, -300, 1920, 1080),
                work_area: physical(-1920, -275, 1920, 1055),
                scale: 1.0,
            },
        ]
    }

    fn screen_at(screens: &[Screen], x: f64, y: f64) -> Option<&Screen> {
        screens.iter().find(|screen| {
            let b = &screen.bounds;
            x >= f64::from(b.position.x)
                && x < f64::from(b.position.x) + f64::from(b.size.width)
                && y >= f64::from(b.position.y)
                && y < f64::from(b.position.y) + f64::from(b.size.height)
        })
    }

    /// Where the pill goes with the cursor at physical (x, y), and on which
    /// screen: the same steps as `Overlay::show`.
    fn pill_for_cursor(
        screens: &[Screen],
        x: f64,
        y: f64,
    ) -> Option<(&'static str, LogicalPosition<f64>)> {
        let screen = pick_monitor(
            || Ok::<_, ()>((x, y)),
            |(x, y)| Ok(screen_at(screens, x, y)),
            || Ok(screens.first()),
        )?;
        let area = WorkArea::from_physical(&screen.work_area, screen.scale);
        Some((screen.name, bottom_centre(&area, PILL, 24.0)))
    }

    #[test]
    fn with_mixed_scales_the_pill_lands_on_the_cursors_screen() {
        let screens = mixed_screens();
        // External: logical work area (-1920, -275), 1920 x 1055, so
        // -1920 + (1920 - 400) / 2 and -275 + 1055 - 80 - 24.
        assert_eq!(
            pill_for_cursor(&screens, -100.0, 0.0),
            Some(("external", LogicalPosition::new(-1160.0, 676.0)))
        );
        // Built-in: logical work area (0, 25), 1440 x 875.
        assert_eq!(
            pill_for_cursor(&screens, 2000.0, 1000.0),
            Some(("built-in", LogicalPosition::new(520.0, 796.0)))
        );
        // Between the displays: the primary one.
        assert_eq!(
            pill_for_cursor(&screens, -100.0, 1500.0),
            Some(("built-in", LogicalPosition::new(520.0, 796.0)))
        );
    }

    #[test]
    fn cursor_lookup_failure_uses_primary_monitor() {
        // The cursor can't be read: the monitor lookup isn't even tried.
        let mut asked_at = None;
        assert_eq!(
            pick_monitor(
                || Err("no cursor"),
                |point: (f64, f64)| {
                    asked_at = Some(point);
                    Ok(Some("under cursor"))
                },
                || Ok(Some("primary")),
            ),
            Some("primary")
        );
        assert_eq!(asked_at, None);
        // The monitor under it can't be found, or there is none.
        assert_eq!(
            pick_monitor(
                || Ok((1.0, 2.0)),
                |_| Err("lookup failed"),
                || Ok(Some("primary"))
            ),
            Some("primary")
        );
        assert_eq!(
            pick_monitor(
                || Ok::<_, &str>((1.0, 2.0)),
                |_| Ok(None),
                || Ok(Some("primary"))
            ),
            Some("primary")
        );
        // Found: the primary one isn't asked for.
        let mut primary_asked = false;
        assert_eq!(
            pick_monitor(
                || Ok::<_, &str>((1.0, 2.0)),
                |point| {
                    assert_eq!(point, (1.0, 2.0));
                    Ok(Some("under cursor"))
                },
                || {
                    primary_asked = true;
                    Ok(Some("primary"))
                },
            ),
            Some("under cursor")
        );
        assert!(!primary_asked);
        // No primary either.
        assert_eq!(
            pick_monitor(
                || Err("no cursor"),
                |()| Ok(Some("under cursor")),
                || Err("none")
            ),
            None::<&str>
        );
    }

    #[test]
    fn no_monitor_still_shows_panel() {
        let calls = std::cell::RefCell::new(Vec::new());
        let shown = place_and_show::<()>(
            None,
            |position| {
                calls
                    .borrow_mut()
                    .push(format!("move {}, {}", position.x, position.y));
                Ok(())
            },
            || {
                calls.borrow_mut().push("show".to_owned());
                Ok(())
            },
        );
        assert_eq!(shown, Ok(()));
        assert_eq!(*calls.borrow(), ["show"]);
    }

    #[test]
    fn with_a_monitor_the_panel_is_moved_before_it_is_shown() {
        let calls = std::cell::RefCell::new(Vec::new());
        let shown = place_and_show::<()>(
            Some(LogicalPosition::new(520.0, 796.0)),
            |position| {
                calls
                    .borrow_mut()
                    .push(format!("move {}, {}", position.x, position.y));
                Ok(())
            },
            || {
                calls.borrow_mut().push("show".to_owned());
                Ok(())
            },
        );
        assert_eq!(shown, Ok(()));
        assert_eq!(*calls.borrow(), ["move 520, 796", "show"]);
    }
}
