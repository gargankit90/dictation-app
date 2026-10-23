//! Thin Tauri shell: wires windows, commands and events to the `app` crate.

/// Builds and runs the Tauri application.
///
/// # Panics
///
/// Panics if Tauri fails to start, which leaves the app with nothing to show.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(clippy::expect_used)] // No UI exists yet to report a startup failure to.
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running the Tauri application");
}
