//! M1.5 criterion 2 without a GUI: `--press` then `--release`, injected
//! the way the shell does, run the stand-in pipeline through every state.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

mod common;

use std::time::Duration;

use common::{DiscardingInserter, FixedTranscriber, GrantedPermissions, ToneAudio};
use dictation_core::coordinator::{MIN_HOLD, Phase};
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::test_hooks::{SyntheticHotkey, hook_events};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{FakeClock, FakeHotkey};
use dictation_ports::{HotkeyError, HotkeyEvent};

const WAIT: Duration = Duration::from_secs(5);

async fn next(ui: &mut UiReceiver) -> UiEvent {
    tokio::time::timeout(WAIT, ui.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn press_then_release_flags_run_the_full_state_sequence() {
    // As without Accessibility: the real hotkey can't start, the flags work.
    let real = FakeHotkey::new();
    real.fail_next_start(HotkeyError::PermissionDenied);
    let (hotkey, injector) = SyntheticHotkey::new(real);
    let clock = FakeClock::new();
    let ports = Ports {
        hotkey,
        audio: ToneAudio::default(),
        transcriber: FixedTranscriber,
        inserter: DiscardingInserter,
        permissions: GrantedPermissions,
        clock: clock.clone(),
    };
    let (pipeline, mut ui) = Pipeline::spawn(ports, Config::default()).unwrap();

    // First launch: `dictation-app --press`; a second one: `--release`.
    for event in hook_events(["dictation-app", "--press"]) {
        assert!(injector.inject(event));
    }
    assert_eq!(next(&mut ui).await, UiEvent::StateChanged(Phase::Recording));
    clock.advance(MIN_HOLD * 2);
    for event in hook_events(["dictation-app", "--release"]) {
        assert!(injector.inject(event));
    }

    for phase in [Phase::Transcribing, Phase::Inserting, Phase::Idle] {
        assert_eq!(next(&mut ui).await, UiEvent::StateChanged(phase));
    }
    pipeline.shutdown().await;
    assert!(!injector.inject(HotkeyEvent::Pressed));
}

/// Folded in from `shell_flags_edges.rs` (SIMP1).
mod edges {
    // M1.5 edge cases for the `--press`/`--release` test flags (Tester, run 1):
    // a stray `--release` with nothing recording, and a double `--press`.
    //
    // No wall-clock "nothing happened" timeouts (#58): the pipeline reads key
    // events in FIFO order on one task, so an ignored event queued ahead of a
    // valid one shows up as the valid one's event coming next, and after
    // `shutdown` the closed UI channel (`None`) proves nothing else was sent.

    #![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

    use std::time::Duration;

    use crate::common::{DiscardingInserter, FixedTranscriber, GrantedPermissions, ToneAudio};
    use dictation_core::coordinator::{MIN_HOLD, Phase};
    use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
    use dictation_pipeline::test_hooks::{Injector, SyntheticHotkey, hook_events};
    use dictation_pipeline::ui_events::UiReceiver;
    use dictation_platform::fakes::{FakeClock, FakeHotkey};
    use dictation_ports::HotkeyError;

    /// Only a guard against a hung pipeline; a passing test never waits on it.
    const WAIT: Duration = Duration::from_secs(5);

    type Ui = UiReceiver;

    fn spawn() -> (Pipeline, Ui, Injector, FakeClock) {
        let real = FakeHotkey::new();
        real.fail_next_start(HotkeyError::PermissionDenied);
        let (hotkey, injector) = SyntheticHotkey::new(real);
        let clock = FakeClock::new();
        let ports = Ports {
            hotkey,
            audio: ToneAudio::default(),
            transcriber: FixedTranscriber,
            inserter: DiscardingInserter,
            permissions: GrantedPermissions,
            clock: clock.clone(),
        };
        let (pipeline, ui) = Pipeline::spawn(ports, Config::default()).unwrap();
        (pipeline, ui, injector, clock)
    }

    fn launch(injector: &Injector, flag: &str) {
        for event in hook_events(["dictation-app", flag]) {
            assert!(injector.inject(event));
        }
    }

    async fn next(ui: &mut Ui) -> Option<UiEvent> {
        tokio::time::timeout(WAIT, ui.recv()).await.unwrap()
    }

    async fn finish(pipeline: Pipeline, clock: &FakeClock, injector: &Injector, ui: &mut Ui) {
        clock.advance(MIN_HOLD * 2);
        launch(injector, "--release");
        for phase in [Phase::Transcribing, Phase::Inserting, Phase::Idle] {
            assert_eq!(next(ui).await, Some(UiEvent::StateChanged(phase)));
        }
        pipeline.shutdown().await;
        assert_eq!(next(ui).await, None, "no UI event after Idle");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn release_without_press_changes_nothing_and_a_later_press_works() {
        let (pipeline, mut ui, injector, clock) = spawn();

        // Queued ahead of the press: if it changed anything, its event would
        // come before Recording.
        launch(&injector, "--release");
        launch(&injector, "--press");
        assert_eq!(
            next(&mut ui).await,
            Some(UiEvent::StateChanged(Phase::Recording))
        );
        finish(pipeline, &clock, &injector, &mut ui).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn double_press_records_once_and_one_release_finishes() {
        let (pipeline, mut ui, injector, clock) = spawn();

        launch(&injector, "--press");
        assert_eq!(
            next(&mut ui).await,
            Some(UiEvent::StateChanged(Phase::Recording))
        );
        // Queued ahead of the release: if it changed anything, its event would
        // come before Transcribing.
        launch(&injector, "--press");
        finish(pipeline, &clock, &injector, &mut ui).await;
    }
}
