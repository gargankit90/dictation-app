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
use dictation_core::settings::{Provider, Settings, Transcription};
use dictation_pipeline::usage::ConsentGate;
use dictation_ports::{
    Arch, EngineChoice, EngineKind, Event, OnboardingStep, SettingName, Telemetry, Version,
};
use dictation_telemetry::{Config, PostHogTelemetry, SentryReporter, errors};
use dictation_transcribe::engine::Engine;

/// The PostHog project key this build was made with, if any.
const BUILD_KEY: Option<&str> = option_env!("DICTATION_POSTHOG_KEY");

/// The Sentry DSN this build was made with, if any (A2).
const BUILD_DSN: Option<&str> = option_env!("DICTATION_SENTRY_DSN");

/// The running adapters, as Tauri state: PostHog events (`.0`) and Sentry
/// error reports (`.1`), both governed by the one sharing switch and by
/// [`ConsentGate`] (U8): until the user leaves Welcome both stay off, and
/// every event goes through the gate. `.3` is the app's version, for the
/// `app_opened` that waited for Welcome.
#[derive(Clone)]
pub struct Usage(
    pub Arc<PostHogTelemetry>,
    pub Arc<SentryReporter>,
    Arc<ConsentGate>,
    Option<Version>,
);

impl Usage {
    /// Adapters that do nothing (tests).
    #[must_use]
    pub fn disabled() -> Self {
        let posthog = PostHogTelemetry::disabled();
        let gate = ConsentGate::new(posthog.clone(), true, false);
        Self(posthog, SentryReporter::disabled(), Arc::new(gate), None)
    }

    /// Adapters that do nothing, with the gate in front of `telemetry`.
    #[cfg(test)]
    #[must_use]
    pub fn at_welcome(telemetry: Arc<dyn Telemetry>) -> Self {
        let gate = ConsentGate::new(telemetry, false, true);
        Self(
            PostHogTelemetry::disabled(),
            SentryReporter::disabled(),
            Arc::new(gate),
            None,
        )
    }

    /// The one sharing switch. The adapters stay off while the gate is shut,
    /// so they write no install ID and send no error report either.
    pub fn set_enabled(&self, enabled: bool) {
        self.2.set_share(enabled);
        self.apply();
    }

    fn apply(&self) {
        let allowed = self.2.allows();
        self.0.set_enabled(allowed);
        self.1.set_enabled(allowed);
    }

    /// Records `event`, unless the gate holds it back.
    pub fn record(&self, event: Event) {
        self.2.record(event);
    }

    /// What dictation events are recorded through.
    #[must_use]
    pub fn events(&self) -> Arc<ConsentGate> {
        Arc::clone(&self.2)
    }

    /// The user continued past Welcome: if sharing is on, sending starts,
    /// with the `app_opened` that waited for this. Reads the macOS version,
    /// so that part runs on the blocking pool.
    pub fn left_welcome(&self) {
        self.2.pass_welcome();
        self.apply();
        let Some(version) = self.3.clone() else {
            return;
        };
        let usage = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            usage.record(Event::AppOpened {
                version,
                macos_major: macos_major(),
                arch: Arch::current(),
            });
        });
    }
}

/// Starts the adapter for a launch. `scripted` launches (the e2e test
/// hooks) send nothing, whatever the build key. Until `welcome_passed`
/// nothing starts sending, whatever the saved choice (U8).
pub fn start(
    data_dir: &Path,
    settings: &Settings,
    scripted: bool,
    version: &str,
    welcome_passed: bool,
) -> Usage {
    let key = if scripted { None } else { BUILD_KEY };
    let config = Config::new(key, data_dir);
    let handle = tauri::async_runtime::handle();
    let runtime = handle.inner().clone();
    let allowed = settings.share_usage_events && welcome_passed;
    let posthog = PostHogTelemetry::start(config, allowed, &runtime);
    let dsn = if scripted { None } else { BUILD_DSN };
    let errors = match Version::new(version) {
        Some(release) if dsn.is_some() => {
            let environment = if cfg!(debug_assertions) {
                "development"
            } else {
                "production"
            };
            let config = errors::Config::new(dsn, &release, environment, macos_major(), data_dir);
            let reporter = SentryReporter::start(config, allowed, &runtime);
            reporter.install_panic_hook();
            reporter
        }
        _ => SentryReporter::disabled(),
    };
    let gate = ConsentGate::new(posthog.clone(), welcome_passed, settings.share_usage_events);
    Usage(posthog, errors, Arc::new(gate), Version::new(version))
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
    if before.history_limit != after.history_limit {
        changed(SettingName::HistoryLimit);
    }
    // The engine: on-device or cloud (L1), or another cloud provider.
    let engine_changed = before.transcription != after.transcription
        || (after.transcription == Transcription::Cloud && before.provider != after.provider);
    if engine_changed {
        events.push(Event::EngineSelected {
            engine: match after.transcription {
                Transcription::Local => EngineChoice::Local,
                Transcription::Cloud => EngineChoice::Cloud(after.provider),
            },
        });
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

/// Where `engine` transcribes, for `dictation_finished` (L1): on this
/// computer for the local engine, else in the cloud (an engine that can't
/// run never finishes a dictation).
#[must_use]
pub fn engine_kind(engine: &Engine) -> EngineKind {
    match engine {
        Engine::Local(_) => EngineKind::Local,
        _ => EngineKind::Cloud,
    }
}

/// Sharing was turned on: a new install ID, and the choice is recorded.
pub fn sharing_turned_on(usage: &Usage) {
    usage.set_enabled(true);
    usage.record(Event::SettingChanged {
        setting: SettingName::ShareUsageEvents,
    });
}

/// Applies a save's change. Turning sharing off first stops everything and
/// records nothing (not even that it was turned off); turning it on starts
/// a new install ID and records that it was.
pub fn settings_saved(usage: &Usage, change: SettingsChange) {
    match change.sharing {
        Some(false) => usage.set_enabled(false),
        Some(true) => {
            usage.set_enabled(true);
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
        step: OnboardingStep::from(Step::Ready),
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
    fn l1_switching_to_and_from_the_on_device_engine_is_engine_selected() {
        let cloud = Settings::default();
        let local = Settings {
            transcription: Transcription::Local,
            local_model: "qwen3-asr-0.6b-q4_k_m".into(),
            ..Settings::default()
        };
        assert_eq!(
            settings_change(&cloud, &local).events,
            [Event::EngineSelected {
                engine: EngineChoice::Local
            }]
        );
        assert_eq!(
            settings_change(&local, &cloud).events,
            [engine_selected(Provider::Groq)]
        );
        // Another local model is not an engine change, and its id isn't sent.
        let other = Settings {
            local_model: "parakeet-unified-en-0.6b-q4_k_m".into(),
            ..local.clone()
        };
        assert!(settings_change(&local, &other).events.is_empty());
    }

    #[test]
    fn l1_dictations_count_under_the_engine_that_ran_them() {
        use dictation_transcribe::local::{LocalTranscriber, SpeechModel};
        struct Silent;
        impl SpeechModel for Silent {
            fn run(&mut self, _: &[f32], _: Option<&str>) -> Result<String, String> {
                Ok(String::new())
            }
        }
        let local = Engine::Local(LocalTranscriber::new(Box::new(Silent), None));
        assert_eq!(engine_kind(&local), EngineKind::Local);
        let mock = Engine::Mock(dictation_transcribe::mock::MockTranscriber::default());
        assert_eq!(engine_kind(&mock), EngineKind::Cloud);
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
                step: OnboardingStep::Ready,
            });
            assert!(!usage.0.is_enabled());
            assert_eq!(usage.0.queued(), 0);
        }
    }

    fn walking(fake: &dictation_platform::fakes::FakeTelemetry) -> Usage {
        Usage::at_welcome(Arc::new(fake.clone()))
    }

    /// U8 criterion 2, through what the commands call: the steps' events and
    /// the sharing switch.
    #[test]
    fn u8_no_event_before_leaving_welcome_then_they_flow() {
        let fake = dictation_platform::fakes::FakeTelemetry::new();
        let usage = walking(&fake);
        onboarding_step(&usage, Step::Welcome);
        sharing_turned_on(&usage);
        assert!(fake.events().is_empty());

        usage.left_welcome();
        onboarding_step(&usage, Step::Speech);
        assert_eq!(fake.events().len(), 1);
    }

    /// U8 criterion 2.
    #[test]
    fn u8_switched_off_on_welcome_no_event_ever() {
        let fake = dictation_platform::fakes::FakeTelemetry::new();
        let usage = walking(&fake);
        usage.set_enabled(false);
        usage.left_welcome();
        onboarding_step(&usage, Step::Speech);
        onboarding_completed(&usage);
        assert!(fake.events().is_empty());
    }
}
