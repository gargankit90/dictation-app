//! The shell side of the menu-bar menu (U5): builds the native menu from the
//! pure [`MenuModel`] in `dictation-pipeline`, swaps the template icon, and
//! runs what each item does.
//!
//! The menu is rebuilt whenever its model changes: after a pipeline event, a
//! click, and once a second from what the app reads (settings, engine,
//! downloads, permissions), so a changed key, engine or error shows within a
//! second. The tray never has a text title.
//!
//! The last dictation's text is in [`LastDictation`] (memory only). Nothing
//! here logs it; the log line for a refresh holds the status text, which is
//! made of fixed sentences.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use dictation_core::diagnostics::ErrorCode;
use dictation_core::local_models;
use dictation_core::settings::{Provider, Transcription};
use dictation_pipeline::last_dictation::LastDictation;
use dictation_pipeline::tray::{
    Ambient, Download, Entry, Fix, MenuModel, TrayIcon, TrayStatus, id,
};
use dictation_ports::{Permission, PermissionStatus};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, Wry};

use crate::diagnostics::WindowRequest;
use crate::models::{ModelDownloads, ModelState};
use crate::{commands, diagnostics, engine, onboarding, updater};

/// The tray icon's id.
pub const TRAY_ID: &str = "main";

/// How often the menu re-reads what the app knows.
const READ_EVERY: Duration = Duration::from_secs(1);

// Black-plus-alpha template images (the Design Lead's, `icons/tray/`); the
// @2x files are for Retina menu bars, which every supported Mac has.
const NORMAL_PNG: &[u8] = include_bytes!("../icons/tray/trayTemplate@2x.png");
const ATTENTION_PNG: &[u8] = include_bytes!("../icons/tray/tray-attentionTemplate@2x.png");
const RECORDING_PNG: &[u8] = include_bytes!("../icons/tray/tray-recordingTemplate@2x.png");

/// The template image for `icon`.
fn image(icon: TrayIcon) -> tauri::Result<Image<'static>> {
    Image::from_bytes(match icon {
        TrayIcon::Normal => NORMAL_PNG,
        TrayIcon::Attention => ATTENTION_PNG,
        TrayIcon::Recording => RECORDING_PNG,
    })
}

/// What the tray is showing and why. Managed Tauri state.
pub struct TrayHub {
    status: Mutex<TrayStatus>,
    applied: Mutex<Option<MenuModel>>,
    last: LastDictation,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl TrayHub {
    /// A hub whose "Copy last dictation" follows `last`.
    #[must_use]
    pub fn new(last: LastDictation) -> Self {
        Self {
            status: Mutex::new(TrayStatus::default()),
            applied: Mutex::new(None),
            last,
        }
    }

    /// Runs `change` on the status; `true` if the menu may differ now.
    pub fn update(&self, change: impl FnOnce(&mut TrayStatus) -> bool) -> bool {
        change(&mut lock(&self.status))
    }
}

/// What an item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Settings,
    Troubleshoot,
    Report,
    ResumeSetup,
    CheckForUpdates,
    CopyLast,
    Fix(Fix),
}

/// The action for a menu item's id.
#[must_use]
pub fn action_for(item: &str) -> Option<Action> {
    Some(match item {
        id::QUIT => Action::Quit,
        id::SETTINGS => Action::Settings,
        id::TROUBLESHOOT => Action::Troubleshoot,
        id::REPORT => Action::Report,
        id::RESUME_SETUP => Action::ResumeSetup,
        id::CHECK_UPDATES => Action::CheckForUpdates,
        id::COPY_LAST => Action::CopyLast,
        other => Action::Fix(Fix::from_id(other)?),
    })
}

/// What a fix does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Show the window with this request.
    Window(WindowRequest),
    /// Open System Settings at this permission's page.
    SystemSettings(Permission),
    /// Restart the app.
    Relaunch,
}

/// The effect of `fix`.
#[must_use]
pub const fn effect_of(fix: Fix) -> Effect {
    match fix {
        Fix::OpenSpeechSettings | Fix::DownloadModel => Effect::Window(WindowRequest::Speech),
        Fix::ChooseAnotherKey => Effect::Window(WindowRequest::Shortcut),
        Fix::Troubleshoot => Effect::Window(WindowRequest::Troubleshoot),
        Fix::OpenMicrophoneSettings => Effect::SystemSettings(Permission::Microphone),
        Fix::OpenAccessibilitySettings => Effect::SystemSettings(Permission::Accessibility),
        Fix::Relaunch => Effect::Relaunch,
    }
}

/// Creates the tray with the idle menu; [`start`] fills in the real one.
///
/// # Errors
///
/// If the tray icon or its menu can't be created.
pub fn build(app: &App, last: LastDictation) -> tauri::Result<()> {
    let hub = TrayHub::new(last);
    let model = lock(&hub.status).menu();
    let menu = native_menu(app.handle(), &model)?;
    let tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(model.tooltip.clone())
        .icon(image(model.icon)?)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(|app, event| click(app, event.id().as_ref()));
    tray.build(app)?;
    *lock(&hub.applied) = Some(model);
    app.manage(hub);
    Ok(())
}

/// Makes the menu follow the app: refreshes at once and then every second.
pub fn start(app: &AppHandle) {
    refresh(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(READ_EVERY);
        loop {
            tick.tick().await;
            refresh(&app);
        }
    });
}

fn native_menu(app: &AppHandle, model: &MenuModel) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for entry in &model.entries {
        match entry {
            Entry::Item {
                id,
                label,
                enabled,
                accelerator,
            } => menu.append(&MenuItem::with_id(
                app,
                *id,
                label,
                *enabled,
                accelerator.as_deref(),
            )?)?,
            Entry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }
    Ok(menu)
}

/// Reads what the menu depends on besides the pipeline's events.
fn read_ambient(app: &AppHandle) -> Ambient {
    let settings = app
        .try_state::<commands::AppConfig>()
        .and_then(|config| config.0.settings().ok())
        .unwrap_or_default();
    let engine_name = match settings.transcription {
        Transcription::Cloud => match settings.provider {
            Provider::Groq => "Groq".to_owned(),
            Provider::OpenAi => "OpenAI".to_owned(),
        },
        Transcription::Local => local_models::find(&settings.local_model).map_or_else(
            || "On this Mac".to_owned(),
            |model| format!("On this Mac ({})", model.name),
        ),
    };
    let live = app.try_state::<engine::LiveEngine>();
    let download = app
        .try_state::<ModelDownloads>()
        .zip(live.as_ref())
        .and_then(|(downloads, live)| {
            downloads
                .list(live)
                .into_iter()
                .find(|model| model.state == ModelState::Downloading)
                .map(|model| Download {
                    percent: percent(model.downloaded_bytes, model.size_bytes),
                    name: model.name,
                })
        });
    let (mic, accessibility) = app.try_state::<onboarding::OsPermissions>().map_or(
        (PermissionStatus::Granted, PermissionStatus::Granted),
        |os| {
            (
                os.0.check(Permission::Microphone),
                os.0.check(Permission::Accessibility),
            )
        },
    );
    let engine_problem = live
        .and_then(|live| live.current_engine())
        .and_then(|engine| match &*engine {
            dictation_transcribe::engine::Engine::Unavailable(
                dictation_ports::TranscribeError::KeyPending,
            ) => None,
            dictation_transcribe::engine::Engine::Unavailable(error) => {
                Some(ErrorCode::from_reason(&error.user_message()))
            }
            _ => None,
        });
    let problem = if mic == PermissionStatus::Denied {
        Some(ErrorCode::MicDenied)
    } else if accessibility != PermissionStatus::Granted {
        Some(ErrorCode::AccessibilityMissing)
    } else {
        engine_problem
    };
    Ambient {
        hotkey: settings.hotkey,
        engine: engine_name,
        download,
        problem,
        setup_incomplete: mic != PermissionStatus::Granted
            || accessibility != PermissionStatus::Granted
            || engine_problem == Some(ErrorCode::KeyMissing),
    }
}

fn percent(done: f64, total: f64) -> u8 {
    if total <= 0.0 {
        return 0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=100"
    )]
    {
        (done / total * 100.0).clamp(0.0, 100.0) as u8
    }
}

/// Brings the tray in line with the app's state. Does nothing if the model
/// is what is already shown.
pub fn refresh(app: &AppHandle) {
    let Some(hub) = app.try_state::<TrayHub>() else {
        return;
    };
    let ambient = read_ambient(app);
    let model = {
        let mut status = lock(&hub.status);
        status.set_ambient(ambient);
        status.set_has_last(hub.last.is_held());
        status.menu()
    };
    let mut applied = lock(&hub.applied);
    if applied.as_ref() == Some(&model) {
        return;
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let previous_icon = applied.as_ref().map(|shown| shown.icon);
    let result = native_menu(app, &model)
        .and_then(|menu| tray.set_menu(Some(menu)))
        .and_then(|()| tray.set_tooltip(Some(&model.tooltip)))
        .and_then(|()| {
            if previous_icon == Some(model.icon) {
                Ok(())
            } else {
                tray.set_icon(Some(image(model.icon)?))
            }
        });
    match result {
        Ok(()) => {
            // This line's text is what the e2e scenarios wait for. The status
            // is one of the app's fixed sentences, never a transcript.
            let outline = model.outline();
            tracing::info!(
                status = %outline.first().map(|(_, label)| label.as_str()).unwrap_or_default(),
                icon = ?model.icon,
                items = %outline.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>().join(","),
                "tray menu"
            );
            *applied = Some(model);
        }
        Err(error) => tracing::warn!(%error, "could not update the tray"),
    }
}

fn click(app: &AppHandle, item: &str) {
    let Some(action) = action_for(item) else {
        return;
    };
    match action {
        Action::Quit => {
            tracing::info!("quit from the tray");
            app.exit(0);
        }
        Action::Settings => crate::show_settings(app),
        Action::Troubleshoot => ask_window(app, WindowRequest::Troubleshoot),
        Action::Report => ask_window(app, WindowRequest::Report),
        Action::ResumeSetup => crate::show_onboarding(app),
        Action::CheckForUpdates => updater::check_from_tray(app),
        Action::CopyLast => copy_last(app),
        Action::Fix(fix) => run_fix(app, fix),
    }
}

/// Shows the window on the page `request` names.
fn ask_window(app: &AppHandle, request: WindowRequest) {
    if let Some(diagnostics) = app.try_state::<diagnostics::Diagnostics>() {
        diagnostics.ask_for(request);
    }
    crate::show_settings(app);
}

fn run_fix(app: &AppHandle, fix: Fix) {
    match effect_of(fix) {
        Effect::Window(request) => ask_window(app, request),
        Effect::SystemSettings(permission) => {
            if let Err(error) =
                dictation_platform::system_settings::open_privacy_settings(permission)
            {
                tracing::warn!(%error, ?permission, "could not open System Settings");
            }
            // Only a relaunch picks up the grant: offer it.
            if let Some(hub) = app.try_state::<TrayHub>() {
                lock(&hub.status).grant_opened();
            }
            refresh(app);
        }
        Effect::Relaunch => {
            if let Some(shell) = app.try_state::<commands::AppShell>() {
                commands::relaunch_from_tray(&shell);
            }
        }
    }
}

/// Puts the last dictation on the clipboard. The text goes nowhere else and
/// is not logged.
fn copy_last(app: &AppHandle) {
    let Some(hub) = app.try_state::<TrayHub>() else {
        return;
    };
    let Some(text) = hub.last.text() else {
        return;
    };
    let spawned = std::thread::Builder::new()
        .name("copy-last".into())
        .spawn(
            move || match dictation_platform::insert::SystemClipboard::new().copy(&text) {
                Ok(()) => tracing::info!("copied the last dictation"),
                Err(error) => tracing::warn!(%error, "could not copy the last dictation"),
            },
        );
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start the copy");
    }
}

/// Shared by [`crate`]'s event loop: applies a pipeline event, then
/// refreshes.
pub fn on_event(app: &AppHandle, event: &dictation_pipeline::pipeline::UiEvent) {
    if let Some(hub) = app.try_state::<TrayHub>() {
        hub.update(|status| status.on_event(event));
    }
    refresh(app);
}

/// The hub's last-dictation holder, for the event loop's success path.
#[must_use]
pub fn last_of(app: &AppHandle) -> Option<LastDictation> {
    app.try_state::<TrayHub>().map(|hub| hub.last.clone())
}

#[cfg(test)]
mod tests {
    use dictation_pipeline::tray::TrayStatus;

    use super::*;

    /// U5 criterion 3: every fix item in any menu has an action, and its
    /// effect is the table's.
    #[test]
    fn criterion_3_each_fix_has_the_effect_from_the_table() {
        let table = [
            (
                Fix::OpenSpeechSettings,
                Effect::Window(WindowRequest::Speech),
            ),
            (Fix::DownloadModel, Effect::Window(WindowRequest::Speech)),
            (
                Fix::ChooseAnotherKey,
                Effect::Window(WindowRequest::Shortcut),
            ),
            (
                Fix::Troubleshoot,
                Effect::Window(WindowRequest::Troubleshoot),
            ),
            (
                Fix::OpenMicrophoneSettings,
                Effect::SystemSettings(Permission::Microphone),
            ),
            (
                Fix::OpenAccessibilitySettings,
                Effect::SystemSettings(Permission::Accessibility),
            ),
            (Fix::Relaunch, Effect::Relaunch),
        ];
        for (fix, effect) in table {
            assert_eq!(effect_of(fix), effect, "{fix:?}");
            assert_eq!(action_for(fix.id()), Some(Action::Fix(fix)), "{fix:?}");
        }
    }

    #[test]
    fn every_item_a_menu_can_hold_has_an_action() {
        let mut ids = std::collections::BTreeSet::new();
        for code in ErrorCode::ALL {
            let mut status = TrayStatus::default();
            status.set_ambient(Ambient {
                problem: Some(code),
                setup_incomplete: true,
                ..Ambient::default()
            });
            status.set_has_last(true);
            status.grant_opened();
            for (item, _) in status.menu().outline() {
                ids.insert(item);
            }
            for (item, _) in TrayStatus::default().menu().outline() {
                ids.insert(item);
            }
        }
        for item in ids {
            if item == "-" || item == id::STATUS || item == id::ENGINE {
                continue;
            }
            assert!(action_for(&item).is_some(), "{item} does nothing");
        }
        assert_eq!(action_for("reveal-log"), None);
        assert_eq!(action_for("relaunch"), None);
    }

    #[test]
    fn criterion_4_the_three_template_images_decode() {
        for icon in [TrayIcon::Normal, TrayIcon::Attention, TrayIcon::Recording] {
            let image = image(icon).unwrap();
            assert!(image.width() > 0 && image.height() > 0, "{icon:?}");
        }
    }

    #[test]
    fn the_download_percent_is_clamped() {
        assert_eq!(percent(0.0, 0.0), 0);
        assert_eq!(percent(42.0, 100.0), 42);
        assert_eq!(percent(500.0, 100.0), 100);
    }
}
