//! M1.5 criterion 2 without a GUI: `--press` then `--release`, injected
//! the way the shell does, run the stand-in pipeline through every state.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::time::Duration;

use dictation_core::coordinator::{MIN_HOLD, Phase};
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{
    DiscardingInserter, FixedTranscriber, GrantedPermissions, ToneAudio,
};
use dictation_pipeline::test_hooks::{SyntheticHotkey, hook_events};
use dictation_platform::fakes::{FakeClock, FakeHotkey};
use dictation_ports::{HotkeyError, HotkeyEvent};
use tokio::sync::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(5);

async fn next(ui: &mut UnboundedReceiver<UiEvent>) -> UiEvent {
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
