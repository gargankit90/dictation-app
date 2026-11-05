//! Anonymous usage events in the shell (A1, `docs/privacy.md`): starts the
//! PostHog adapter, and records the events that happen in commands (settings,
//! onboarding, the engine) and at launch.
//!
//! The project key is read at build time from `DICTATION_POSTHOG_KEY`. A
//! build without it (dev, CI, forks) sends nothing and writes no install ID.
//!
//! Sending follows the stored choice from the moment the app starts (U10):
//! setup is measured from its first screen, and a stored "off" sends nothing
//! at any step.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use dictation_core::diagnostics::ErrorCode;
use dictation_core::error::DictationError;
use dictation_core::onboarding::{Readiness, Step, TryOutcome};
use dictation_core::settings::{Provider, Settings, Transcription};
use dictation_pipeline::config::Config as AppSettingsFile;
use dictation_pipeline::usage::SharingGate;
use dictation_ports::{
    Arch, EngineChoice, EngineKind, Event, OnboardingStep, Permission, PermissionStatus,
    Permissions, SecretStore, SettingName, SetupProblem, Telemetry, Version,
};
use dictation_telemetry::{Config, PostHogTelemetry, SentryReporter, errors};
use dictation_transcribe::engine::Engine;
use dictation_transcribe::models::DownloadError;

use crate::commands::KeyTestKind;
use crate::onboarding::{Onboarding, OnboardingStatus, PermissionState, View};

/// The PostHog project key this build was made with, if any.
const BUILD_KEY: Option<&str> = option_env!("DICTATION_POSTHOG_KEY");

/// The Sentry DSN this build was made with, if any (A2).
const BUILD_DSN: Option<&str> = option_env!("DICTATION_SENTRY_DSN");

/// The running adapters, as Tauri state: PostHog events (`.0`) and Sentry
/// error reports (`.1`), both governed by the one sharing switch, and the
/// [`SharingGate`] every event goes through.
#[derive(Clone)]
pub struct Usage(
    pub Arc<PostHogTelemetry>,
    pub Arc<SentryReporter>,
    Arc<SharingGate>,
);

impl Usage {
    /// Adapters that do nothing (tests).
    #[must_use]
    pub fn disabled() -> Self {
        let posthog = PostHogTelemetry::disabled();
        let gate = SharingGate::new(posthog.clone(), false);
        Self(posthog, SentryReporter::disabled(), Arc::new(gate))
    }

    /// These adapters, with the gate in front of the events (tests of the real
    /// adapters).
    #[cfg(test)]
    #[must_use]
    pub fn of(posthog: Arc<PostHogTelemetry>, errors: Arc<SentryReporter>, share: bool) -> Self {
        let gate = SharingGate::new(posthog.clone(), share);
        Self(posthog, errors, Arc::new(gate))
    }

    /// Adapters that do nothing, with the gate in front of `telemetry`.
    #[cfg(test)]
    #[must_use]
    pub fn recording(telemetry: Arc<dyn Telemetry>, share: bool) -> Self {
        let gate = SharingGate::new(telemetry, share);
        Self(
            PostHogTelemetry::disabled(),
            SentryReporter::disabled(),
            Arc::new(gate),
        )
    }

    /// The one sharing switch: the gate first, so nothing passes it while the
    /// adapters clear what they hold.
    pub fn set_enabled(&self, enabled: bool) {
        self.2.set_share(enabled);
        self.0.set_enabled(enabled);
        self.1.set_enabled(enabled);
    }

    /// Records `event`, if sharing is on.
    pub fn record(&self, event: Event) {
        self.2.record(event);
    }

    /// What dictation events are recorded through.
    #[must_use]
    pub fn events(&self) -> Arc<SharingGate> {
        Arc::clone(&self.2)
    }
}

/// The stored sharing choice, for a launch: on for a fresh install (no file
/// yet), and off if the file can't be read, since it may hold an opt-out.
pub fn stored_choice<S: SecretStore>(config: &AppSettingsFile<S>) -> bool {
    config
        .settings()
        .is_ok_and(|settings| settings.share_usage_events)
}

/// Starts the adapter for a launch. `scripted` launches (the e2e test hooks)
/// send nothing, whatever the build key. With `share` on, sending starts now,
/// from the first setup screen (U10); with it off, nothing is sent and no
/// install ID is made.
pub fn start(data_dir: &Path, share: bool, scripted: bool, version: &str) -> Usage {
    let key = if scripted { None } else { BUILD_KEY };
    let config = Config::new(key, data_dir);
    let handle = tauri::async_runtime::handle();
    let runtime = handle.inner().clone();
    let posthog = PostHogTelemetry::start(config, share, &runtime);
    let dsn = if scripted { None } else { BUILD_DSN };
    let errors = match Version::new(version) {
        Some(release) if dsn.is_some() => {
            let environment = if cfg!(debug_assertions) {
                "development"
            } else {
                "production"
            };
            let config = errors::Config::new(dsn, &release, environment, macos_major(), data_dir);
            let reporter = SentryReporter::start(config, share, &runtime);
            reporter.install_panic_hook();
            reporter
        }
        _ => SentryReporter::disabled(),
    };
    let gate = SharingGate::new(posthog.clone(), share);
    Usage(posthog, errors, Arc::new(gate))
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

/// Opens setup at the step the user was on, and records that they are there:
/// the one way setup comes on screen, so even someone who stops on the first
/// screen shows up (U10).
pub fn setup_opened(usage: &Usage, onboarding: &Onboarding, readiness: Readiness) -> Step {
    let step = onboarding.open(readiness);
    if !usage.2.allows() {
        onboarding.note_sharing_off();
    }
    onboarding_step(usage, step);
    step
}

/// Records `problem`, if setup is what the window shows: the same commands
/// also serve Settings, whose problems aren't setup's.
pub fn setup_problem(usage: &Usage, onboarding: &Onboarding, problem: SetupProblem) {
    if onboarding.view() == View::Onboarding {
        usage.record(Event::SetupProblem { reason: problem });
    }
}

/// [`setup_problem`] once per launch, for what is read over and over.
fn setup_problem_once(usage: &Usage, onboarding: &Onboarding, problem: SetupProblem) {
    if onboarding.view() == View::Onboarding && onboarding.first_time(problem) {
        usage.record(Event::SetupProblem { reason: problem });
    }
}

/// The reason for a key test that failed.
#[must_use]
pub fn key_test_problem(kind: KeyTestKind) -> SetupProblem {
    match kind {
        KeyTestKind::NoKey => SetupProblem::KeyTestFailedNoKey,
        KeyTestKind::InvalidKey => SetupProblem::KeyTestFailedRejected,
        KeyTestKind::RateLimited => SetupProblem::KeyTestFailedRateLimited,
        KeyTestKind::Network => SetupProblem::KeyTestFailedNetwork,
        KeyTestKind::Server => SetupProblem::KeyTestFailedServer,
        KeyTestKind::Keychain => SetupProblem::KeyTestFailedKeychain,
    }
}

/// The reason for a download that stopped by itself. A cancel is the user's
/// own (`download_cancelled`, kept by `set_download_paused`), and a cancel
/// also happens when another model takes its place, so it isn't one here.
#[must_use]
pub fn download_problem(error: &DownloadError) -> Option<SetupProblem> {
    match error {
        DownloadError::NoSpace { .. } => Some(SetupProblem::DownloadFailedNoSpace),
        DownloadError::Network(_) => Some(SetupProblem::DownloadFailedNetwork),
        DownloadError::Checksum => Some(SetupProblem::DownloadFailedChecksum),
        // U9 split the file vanishing out of `Disk`; to the reason, it's the same.
        DownloadError::Disk(_) | DownloadError::Removed => Some(SetupProblem::DownloadFailedDisk),
        DownloadError::Cancelled => None,
    }
}

/// The reason for a practice outcome: only skipping is one.
#[must_use]
pub fn skipped_practice(outcome: TryOutcome) -> Option<SetupProblem> {
    (outcome == TryOutcome::Skipped).then_some(SetupProblem::PracticeSkipped)
}

/// The reason for a permission's request that couldn't be shown.
#[must_use]
pub fn request_problem(permission: Permission) -> SetupProblem {
    match permission {
        Permission::Microphone => SetupProblem::MicrophoneRequestFailed,
        Permission::Accessibility => SetupProblem::AccessibilityRequestFailed,
    }
}

/// What the window just read about setup: a refused microphone counts once it
/// is the step in the way. macOS reports no refusal of Accessibility; see
/// [`setup_left`].
pub fn setup_status_read(usage: &Usage, onboarding: &Onboarding, status: &OnboardingStatus) {
    if status.step == Step::Permissions && status.microphone == PermissionState::Denied {
        setup_problem_once(usage, onboarding, SetupProblem::MicrophoneDenied);
    }
}

/// What the window just read about the engine: a model that can't load.
pub fn engine_status_read(usage: &Usage, onboarding: &Onboarding, engine: &Engine) {
    if let Engine::Unavailable(error) = engine {
        let shown = DictationError::TranscriptionFailed {
            reason: error.user_message(),
        };
        if ErrorCode::of(&shown) == ErrorCode::ModelLoadingFailed {
            setup_problem_once(usage, onboarding, SetupProblem::ModelLoadFailed);
        }
    }
}

/// Setup's window was closed or the app is quitting, with setup on the window:
/// where the user stopped. Accessibility they asked for and didn't get counts
/// as denied here, since macOS never says so itself.
pub fn setup_left(usage: &Usage, onboarding: &Onboarding, permissions: &dyn Permissions) {
    let Some((step, accessibility_asked)) = onboarding.take_left() else {
        return;
    };
    if accessibility_asked
        && permissions.check(Permission::Accessibility) != PermissionStatus::Granted
    {
        setup_problem_once(usage, onboarding, SetupProblem::AccessibilityDenied);
    }
    usage.record(Event::OnboardingLeft {
        step: OnboardingStep::from(step),
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

    mod setup {
        use dictation_platform::fakes::{FakePermissions, FakeTelemetry, MemSecretStore};
        use dictation_ports::SetupProblem as P;
        use dictation_ports::TranscribeError;

        use super::*;

        fn walking(share: bool) -> (Usage, FakeTelemetry, Onboarding) {
            let fake = FakeTelemetry::new();
            let usage = Usage::recording(Arc::new(fake.clone()), share);
            (usage, fake, Onboarding::new())
        }

        fn reached(step: OnboardingStep) -> Event {
            Event::OnboardingStepReached { step }
        }

        fn walk_to(usage: &Usage, onboarding: &Onboarding, step: Step) {
            let mut at = setup_opened(usage, onboarding, Readiness::READY);
            onboarding.pass_welcome();
            while at < step {
                at = at.next().unwrap_or(at);
                onboarding.go_to(at, Readiness::READY).unwrap();
                onboarding_step(usage, at);
            }
        }

        /// U10 criterion 2: the first screen counts, and each step after it.
        #[test]
        fn u10_every_step_shows_from_the_first_screen() {
            let (usage, fake, onboarding) = walking(true);
            walk_to(&usage, &onboarding, Step::Ready);
            onboarding_completed(&usage);
            assert_eq!(
                fake.events(),
                [
                    reached(OnboardingStep::Welcome),
                    reached(OnboardingStep::Speech),
                    reached(OnboardingStep::Permissions),
                    reached(OnboardingStep::Try),
                    reached(OnboardingStep::Ready),
                    Event::OnboardingCompleted {
                        step: OnboardingStep::Ready
                    },
                ]
            );
        }

        /// U10 criterion 2: someone who quits on the first screen shows up.
        #[test]
        fn u10_quitting_on_welcome_shows_where_they_stopped() {
            let (usage, fake, onboarding) = walking(true);
            setup_opened(&usage, &onboarding, Readiness::default());
            setup_left(&usage, &onboarding, &FakePermissions::new());
            assert_eq!(
                fake.events(),
                [
                    reached(OnboardingStep::Welcome),
                    Event::OnboardingLeft {
                        step: OnboardingStep::Welcome
                    },
                ]
            );
        }

        /// U10 criterion 2: closing the window and then quitting is one stop,
        /// and opening setup again can stop again.
        #[test]
        fn u10_a_stop_before_ready_counts_once_until_setup_opens_again() {
            let (usage, fake, onboarding) = walking(true);
            walk_to(&usage, &onboarding, Step::Try);
            let permissions = FakePermissions::new();
            setup_left(&usage, &onboarding, &permissions);
            setup_left(&usage, &onboarding, &permissions);
            let left = Event::OnboardingLeft {
                step: OnboardingStep::Try,
            };
            assert_eq!(fake.events().iter().filter(|e| **e == left).count(), 1);
            setup_opened(&usage, &onboarding, Readiness::READY);
            setup_left(&usage, &onboarding, &permissions);
            assert_eq!(fake.events().iter().filter(|e| **e == left).count(), 2);
        }

        #[test]
        fn u10_finishing_is_not_a_stop() {
            let (usage, fake, onboarding) = walking(true);
            walk_to(&usage, &onboarding, Step::Ready);
            onboarding.finish();
            setup_left(&usage, &onboarding, &FakePermissions::new());
            assert!(
                !fake
                    .events()
                    .iter()
                    .any(|e| matches!(e, Event::OnboardingLeft { .. }))
            );
        }

        /// U10 criterion 3: everything setup can record, with the choice
        /// stored off. Nothing, at any step.
        #[test]
        fn u10_stored_off_records_nothing_at_any_step() {
            let (usage, fake, onboarding) = walking(false);
            walk_to(&usage, &onboarding, Step::Ready);
            for problem in P::ALL {
                setup_problem(&usage, &onboarding, problem);
            }
            onboarding.note_accessibility_asked();
            setup_left(&usage, &onboarding, &FakePermissions::new());
            onboarding_completed(&usage);
            assert_eq!(fake.events(), []);
        }

        /// Round 2: Ready's caption reads this. Stored off, it can't say sharing
        /// has been on since setup began, even once it is turned on.
        #[test]
        fn u10_a_stored_off_is_noted_when_setup_opens() {
            let permissions = FakePermissions::new();
            let (usage, _, onboarding) = walking(false);
            setup_opened(&usage, &onboarding, Readiness::default());
            assert!(!onboarding.status(&permissions, false).sharing_on_throughout);
            usage.set_enabled(true);
            assert!(!onboarding.status(&permissions, false).sharing_on_throughout);

            let (usage, _, onboarding) = walking(true);
            setup_opened(&usage, &onboarding, Readiness::default());
            assert!(onboarding.status(&permissions, false).sharing_on_throughout);
        }

        /// U10 criterion 4: a problem in Settings isn't a setup problem.
        #[test]
        fn u10_problems_count_only_while_setup_is_on_the_window() {
            let (usage, fake, onboarding) = walking(true);
            setup_problem(&usage, &onboarding, P::KeyTestFailedRejected);
            assert_eq!(fake.events(), []);
            setup_opened(&usage, &onboarding, Readiness::READY);
            setup_problem(&usage, &onboarding, P::KeyTestFailedRejected);
            onboarding.close();
            setup_problem(&usage, &onboarding, P::DownloadFailedNetwork);
            assert_eq!(
                fake.events().last(),
                Some(&Event::SetupProblem {
                    reason: P::KeyTestFailedRejected
                })
            );
        }

        /// U10 criterion 4: named for the permission, and once for a window
        /// that reads the status every second.
        #[test]
        fn u10_a_refused_microphone_counts_once_on_its_step() {
            let (usage, fake, onboarding) = walking(true);
            let permissions = FakePermissions::new();
            permissions.set(Permission::Microphone, PermissionStatus::Denied);
            setup_opened(&usage, &onboarding, Readiness::default());
            let status = onboarding.status(&permissions, false);
            setup_status_read(&usage, &onboarding, &status);
            assert_eq!(fake.events().len(), 1, "not on Welcome: only the step");

            walk_to(&usage, &onboarding, Step::Permissions);
            for _ in 0..3 {
                let status = onboarding.status(&permissions, false);
                setup_status_read(&usage, &onboarding, &status);
            }
            let denied = Event::SetupProblem {
                reason: P::MicrophoneDenied,
            };
            assert_eq!(fake.events().iter().filter(|e| **e == denied).count(), 1);
        }

        /// U10 criterion 4: macOS can't say Accessibility was refused.
        #[test]
        fn u10_accessibility_asked_for_and_not_given_counts_when_setup_is_left() {
            let denied = Event::SetupProblem {
                reason: P::AccessibilityDenied,
            };
            let permissions = FakePermissions::new();
            permissions.set(Permission::Accessibility, PermissionStatus::NotDetermined);

            let (usage, fake, onboarding) = walking(true);
            setup_opened(&usage, &onboarding, Readiness::READY);
            setup_left(&usage, &onboarding, &permissions);
            assert!(!fake.events().contains(&denied), "they never asked");

            let (usage, fake, onboarding) = walking(true);
            setup_opened(&usage, &onboarding, Readiness::READY);
            onboarding.note_accessibility_asked();
            setup_left(&usage, &onboarding, &permissions);
            assert!(fake.events().contains(&denied));

            let (usage, fake, onboarding) = walking(true);
            permissions.set(Permission::Accessibility, PermissionStatus::Granted);
            setup_opened(&usage, &onboarding, Readiness::READY);
            onboarding.note_accessibility_asked();
            setup_left(&usage, &onboarding, &permissions);
            assert!(!fake.events().contains(&denied), "they got it");
        }

        /// U10 criterion 4: what each failure maps to, and no text goes along:
        /// the details planted here are in the errors, not in the event.
        #[test]
        fn u10_reasons_are_fixed_values_whatever_the_error_says() {
            const PLANTED: &str = "/Users/someone/secret gsk_key hunter2 words";
            let cases = [
                (
                    DownloadError::NoSpace {
                        needed: 1,
                        available: 0,
                    },
                    Some(P::DownloadFailedNoSpace),
                ),
                (
                    DownloadError::Network(PLANTED.into()),
                    Some(P::DownloadFailedNetwork),
                ),
                (DownloadError::Checksum, Some(P::DownloadFailedChecksum)),
                (
                    DownloadError::Disk(PLANTED.into()),
                    Some(P::DownloadFailedDisk),
                ),
                (DownloadError::Removed, Some(P::DownloadFailedDisk)),
                (DownloadError::Cancelled, None),
            ];
            for (error, expected) in cases {
                let problem = download_problem(&error);
                assert_eq!(problem, expected, "{error:?}");
                if let Some(reason) = problem {
                    let wire = serde_json::to_string(&Event::SetupProblem { reason }).unwrap();
                    assert!(!wire.contains("secret"), "{wire}");
                    assert!(!wire.contains("Users"), "{wire}");
                    assert!(!wire.contains("hunter2"), "{wire}");
                }
            }
            assert_eq!(
                [
                    KeyTestKind::NoKey,
                    KeyTestKind::InvalidKey,
                    KeyTestKind::RateLimited,
                    KeyTestKind::Network,
                    KeyTestKind::Server,
                    KeyTestKind::Keychain,
                ]
                .map(key_test_problem),
                [
                    P::KeyTestFailedNoKey,
                    P::KeyTestFailedRejected,
                    P::KeyTestFailedRateLimited,
                    P::KeyTestFailedNetwork,
                    P::KeyTestFailedServer,
                    P::KeyTestFailedKeychain,
                ]
            );
            assert_eq!(
                request_problem(Permission::Microphone),
                P::MicrophoneRequestFailed
            );
            assert_eq!(
                request_problem(Permission::Accessibility),
                P::AccessibilityRequestFailed
            );
        }

        /// U10 criterion 4: only a model that can't load, not a missing key,
        /// and once for a window that keeps asking.
        #[test]
        fn u10_a_model_that_cannot_load_counts_once() {
            let (usage, fake, onboarding) = walking(true);
            setup_opened(&usage, &onboarding, Readiness::READY);
            let before = fake.events().len();
            let no_key = Engine::Unavailable(TranscribeError::ModelMissing);
            engine_status_read(&usage, &onboarding, &no_key);
            assert_eq!(fake.events().len(), before);
            let broken = Engine::Unavailable(TranscribeError::Engine(
                "couldn't load the on-device model: bad file".into(),
            ));
            engine_status_read(&usage, &onboarding, &broken);
            engine_status_read(&usage, &onboarding, &broken);
            assert_eq!(
                fake.events()[before..],
                [Event::SetupProblem {
                    reason: P::ModelLoadFailed
                }]
            );
        }

        /// U10 criterion 3: on for a fresh install; off if stored off; off if
        /// the file can't be read, since it may hold an opt-out.
        #[test]
        fn u10_the_stored_choice_decides_at_launch() {
            let dir =
                std::env::temp_dir().join(format!("dictation-u10-stored-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let config = AppSettingsFile::new(&dir, MemSecretStore::new());
            assert!(stored_choice(&config), "a fresh install");
            config
                .save_settings(&Settings {
                    share_usage_events: false,
                    ..Settings::default()
                })
                .unwrap();
            assert!(!stored_choice(&config), "stored off");
            std::fs::write(config.path(), "{ not json").unwrap();
            assert!(!stored_choice(&config), "unreadable");
            let _ = std::fs::remove_dir_all(&dir);
        }

        /// The practice outcome that isn't a skip stays silent.
        #[test]
        fn try_outcomes_other_than_skipping_are_not_problems() {
            assert_eq!(
                skipped_practice(TryOutcome::Skipped),
                Some(P::PracticeSkipped)
            );
            assert_eq!(skipped_practice(TryOutcome::Passed), None);
        }
    }
}
