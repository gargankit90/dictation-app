//! Anonymous usage events in the shell (A1, `docs/privacy.md`): starts the
//! PostHog adapter, and records the events that happen in commands (settings,
//! onboarding, the engine) and at launch.
//!
//! The project key is read at build time from `DICTATION_POSTHOG_KEY`. A
//! build without it (dev, CI, forks) sends nothing and writes no install ID.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dictation_core::onboarding::Step;
use dictation_core::settings::{Provider, Settings};
use dictation_ports::{Arch, EngineChoice, Event, OnboardingStep, SettingName, Telemetry, Version};
use dictation_telemetry::{Config, PostHogTelemetry};

/// The PostHog project key this build was made with, if any.
const BUILD_KEY: Option<&str> = option_env!("DICTATION_POSTHOG_KEY");

/// The running adapter, as Tauri state.
#[derive(Clone)]
pub struct Usage(pub Arc<PostHogTelemetry>);

impl Usage {
    /// An adapter that does nothing (tests).
    #[must_use]
    pub fn disabled() -> Self {
        Self(PostHogTelemetry::disabled())
    }

    /// Records `event`.
    pub fn record(&self, event: Event) {
        self.0.record(event);
    }
}

/// Starts the adapter for a launch. `scripted` launches (the e2e test
/// hooks) send nothing, whatever the build key.
pub fn start(data_dir: &Path, settings: &Settings, scripted: bool) -> Usage {
    let key = if scripted { None } else { BUILD_KEY };
    let config = Config::new(key, data_dir);
    let handle = tauri::async_runtime::handle();
    Usage(PostHogTelemetry::start(
        config,
        settings.share_usage_events,
        &handle.inner().clone(),
    ))
}

/// Records `app_opened`, and `update_installed` if the version changed
/// since the last launch. Reads the macOS version, so it runs on the
/// blocking pool.
pub fn launched(usage: Usage, data_dir: &Path, version: &str) {
    let Some(version) = Version::new(version) else {
        return;
    };
    let data_dir = data_dir.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(from_version) = dictation_telemetry::updated_from(&data_dir, &version) {
            usage.record(Event::UpdateInstalled {
                from_version,
                to_version: version.clone(),
            });
        }
        usage.record(Event::AppOpened {
            version,
            macos_major: macos_major(),
            arch: Arch::current(),
        });
    });
}

/// The macOS major version (`15`), or 0 if it can't be read.
fn macos_major() -> u16 {
    std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|text| major_of(&text))
        .unwrap_or(0)
}

fn major_of(version: &str) -> Option<u16> {
    version.trim().split('.').next()?.parse().ok()
}

/// What a save changed: the events to record, and the new sharing choice if
/// it changed. Names only, never values.
#[derive(Debug, PartialEq, Eq)]
pub struct SettingsChange {
    /// `setting_changed` and `engine_selected` events for the other settings.
    pub events: Vec<Event>,
    /// The new "share usage events" choice, if it changed.
    pub sharing: Option<bool>,
}

/// Compares `before` and `after`.
#[must_use]
pub fn settings_change(before: &Settings, after: &Settings) -> SettingsChange {
    let mut events = Vec::new();
    let mut changed = |setting| events.push(Event::SettingChanged { setting });
    if before.provider != after.provider {
        changed(SettingName::Provider);
    }
    if before.model != after.model {
        changed(SettingName::Model);
    }
    if before.language != after.language {
        changed(SettingName::Language);
    }
    if before.hotkey != after.hotkey {
        changed(SettingName::Hotkey);
    }
    if before.launch_at_login != after.launch_at_login {
        changed(SettingName::LaunchAtLogin);
    }
    if before.provider != after.provider {
        events.push(engine_selected(after.provider));
    }
    SettingsChange {
        events,
        sharing: (before.share_usage_events != after.share_usage_events)
            .then_some(after.share_usage_events),
    }
}

/// The `engine_selected` event for `provider`.
#[must_use]
pub fn engine_selected(provider: Provider) -> Event {
    Event::EngineSelected {
        engine: EngineChoice::Cloud(provider),
    }
}

/// Applies a save's change. Turning sharing off first stops everything and
/// records nothing (not even that it was turned off); turning it on starts
/// a new install ID and records that it was.
pub fn settings_saved(usage: &Usage, change: SettingsChange) {
    match change.sharing {
        Some(false) => usage.0.set_enabled(false),
        Some(true) => {
            usage.0.set_enabled(true);
            usage.record(Event::SettingChanged {
                setting: SettingName::ShareUsageEvents,
            });
        }
        None => {}
    }
    for event in change.events {
        usage.record(event);
    }
}

/// Records the user reaching onboarding step `step`.
pub fn onboarding_step(usage: &Usage, step: Step) {
    usage.record(Event::OnboardingStepReached {
        step: OnboardingStep::from(step),
    });
}

/// Records onboarding finished.
pub fn onboarding_completed(usage: &Usage) {
    usage.record(Event::OnboardingCompleted {
        step: OnboardingStep::from(Step::Try),
    });
}

/// Sends what is queued, for at most `limit`, when the app quits.
pub fn flush_on_quit(usage: &Usage, limit: Duration) {
    let usage = usage.0.clone();
    let _ =
        tauri::async_runtime::block_on(async { tokio::time::timeout(limit, usage.flush()).await });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_macos_major_is_the_first_number() {
        assert_eq!(major_of("15.6.1\n"), Some(15));
        assert_eq!(major_of("26.0"), Some(26));
        assert_eq!(major_of("junk"), None);
    }

    #[test]
    fn a_save_reports_the_names_of_what_changed() {
        let before = Settings::default();
        let after = Settings {
            provider: Provider::OpenAi,
            model: "whisper-1".into(),
            hotkey: "Ctrl+Space".into(),
            ..Settings::default()
        };
        let change = settings_change(&before, &after);
        assert_eq!(
            change.events,
            [
                Event::SettingChanged {
                    setting: SettingName::Provider
                },
                Event::SettingChanged {
                    setting: SettingName::Model
                },
                Event::SettingChanged {
                    setting: SettingName::Hotkey
                },
                engine_selected(Provider::OpenAi),
            ]
        );
        assert_eq!(change.sharing, None);
        assert!(settings_change(&before, &before).events.is_empty());
    }

    #[test]
    fn the_sharing_choice_is_reported_apart_from_the_events() {
        let on = Settings::default();
        let off = Settings {
            share_usage_events: false,
            ..Settings::default()
        };
        let change = settings_change(&on, &off);
        assert_eq!(change.sharing, Some(false));
        assert!(change.events.is_empty());
        assert_eq!(settings_change(&off, &on).sharing, Some(true));
    }

    #[test]
    fn a_build_without_a_key_sends_nothing() {
        // CI and forks build without the key; a key in the environment of
        // this test run would make the build send, so it must be absent.
        if BUILD_KEY.is_none() {
            let usage = Usage::disabled();
            usage.record(Event::OnboardingCompleted {
                step: OnboardingStep::Try,
            });
            assert!(!usage.0.is_enabled());
            assert_eq!(usage.0.queued(), 0);
        }
    }
}
