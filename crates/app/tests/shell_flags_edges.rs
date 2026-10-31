//! M1.5 edge cases for the `--press`/`--release` test flags (Tester, run 1):
//! a stray `--release` with nothing recording, and a double `--press`.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::time::Duration;

use dictation_core::coordinator::{MIN_HOLD, Phase};
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{
    DiscardingInserter, FixedTranscriber, GrantedPermissions, SilentAudio,
};
use dictation_pipeline::test_hooks::{Injector, SyntheticHotkey, hook_events};
use dictation_platform::fakes::{FakeClock, FakeHotkey};
use dictation_ports::HotkeyError;
use tokio::sync::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(5);
const QUIET: Duration = Duration::from_millis(300);

type Ui = UnboundedReceiver<UiEvent>;

fn spawn() -> (Pipeline, Ui, Injector, FakeClock) {
    let real = FakeHotkey::new();
    real.fail_next_start(HotkeyError::PermissionDenied);
    let (hotkey, injector) = SyntheticHotkey::new(real);
    let clock = FakeClock::new();
    let ports = Ports {
        hotkey,
        audio: SilentAudio::default(),
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

async fn next(ui: &mut Ui) -> UiEvent {
    tokio::time::timeout(WAIT, ui.recv())
        .await
        .unwrap()
        .unwrap()
}

async fn assert_quiet(ui: &mut Ui) {
    let got = tokio::time::timeout(QUIET, ui.recv()).await;
    assert!(got.is_err(), "expected no UI event, got {got:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn release_without_press_changes_nothing_and_a_later_press_works() {
    let (pipeline, mut ui, injector, clock) = spawn();

    launch(&injector, "--release");
    assert_quiet(&mut ui).await;

    launch(&injector, "--press");
    assert_eq!(next(&mut ui).await, UiEvent::StateChanged(Phase::Recording));
    clock.advance(MIN_HOLD * 2);
    launch(&injector, "--release");
    for phase in [Phase::Transcribing, Phase::Inserting, Phase::Idle] {
        assert_eq!(next(&mut ui).await, UiEvent::StateChanged(phase));
    }
    pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn double_press_records_once_and_one_release_finishes() {
    let (pipeline, mut ui, injector, clock) = spawn();

    launch(&injector, "--press");
    assert_eq!(next(&mut ui).await, UiEvent::StateChanged(Phase::Recording));
    launch(&injector, "--press");
    assert_quiet(&mut ui).await;

    clock.advance(MIN_HOLD * 2);
    launch(&injector, "--release");
    for phase in [Phase::Transcribing, Phase::Inserting, Phase::Idle] {
        assert_eq!(next(&mut ui).await, UiEvent::StateChanged(phase));
    }
    assert_quiet(&mut ui).await;
    pipeline.shutdown().await;
}
