//! M5.1 Tester: overlay sequences the unit tests don't cover: the error
//! timeout constant, rapid dictations, an error followed by a success, and
//! an error that arrives while a newer recording is already showing.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_core::coordinator::Phase;
use dictation_core::diagnostics::ErrorCode;
use dictation_core::error::DictationError;
use dictation_pipeline::overlay::{
    ERROR_VISIBLE_FOR, OverlayModel, OverlaySurface, OverlayView, run,
};
use dictation_pipeline::pipeline::UiEvent;
use tokio::sync::mpsc;
use tokio::time::Instant;

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<(OverlayView, Instant)>>>);

impl Recorder {
    fn views(&self) -> Vec<OverlayView> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|(v, _)| v.clone())
            .collect()
    }
}

impl OverlaySurface for Recorder {
    fn show(&mut self, view: &OverlayView) {
        self.0.lock().unwrap().push((view.clone(), Instant::now()));
    }
}

fn phase(p: Phase) -> UiEvent {
    UiEvent::StateChanged(p)
}

fn failed() -> UiEvent {
    UiEvent::Error(DictationError::EmptyTranscript)
}

fn failed_view() -> OverlayView {
    OverlayView::Error {
        message: ErrorCode::NoSpeech.user_text(),
    }
}

#[test]
fn the_error_stays_up_for_six_seconds() {
    assert_eq!(ERROR_VISIBLE_FOR, Duration::from_secs(6));
}

// U4 criterion 5 changed the end of a successful dictation: Inserting then
// Idle shows "Done" (which hides by itself), not Hidden.
#[test]
fn rapid_dictations_each_show_recording_then_paste_sent() {
    let mut model = OverlayModel::default();
    let mut shown = Vec::new();
    for _ in 0..5 {
        for p in [
            Phase::Recording,
            Phase::Transcribing,
            Phase::Inserting,
            Phase::Idle,
        ] {
            if let Some(change) = model.on_event(&phase(p)) {
                shown.push(change.view);
            }
        }
    }
    let one = [
        OverlayView::Recording,
        OverlayView::Transcribing,
        OverlayView::PasteSent,
    ];
    let expected: Vec<OverlayView> = (0..5).flat_map(|_| one.clone()).collect();
    assert_eq!(shown, expected);
    assert_eq!(model.view(), &OverlayView::PasteSent);
}

#[test]
fn recording_straight_after_transcribing_skips_no_state() {
    // A new press before the previous paste finished (Idle never came).
    let mut model = OverlayModel::default();
    model.on_event(&phase(Phase::Recording));
    model.on_event(&phase(Phase::Transcribing));
    let change = model.on_event(&phase(Phase::Recording)).unwrap();
    assert_eq!(change.view, OverlayView::Recording);
    assert!(!change.expires);
}

#[test]
fn an_error_then_a_successful_dictation_ends_hidden() {
    let mut model = OverlayModel::default();
    model.on_event(&phase(Phase::Recording));
    model.on_event(&phase(Phase::Transcribing));
    model.on_event(&failed());
    assert_eq!(model.on_event(&phase(Phase::Idle)), None);
    assert_eq!(model.view(), &failed_view());
    // The next dictation succeeds.
    assert_eq!(
        model.on_event(&phase(Phase::Recording)).unwrap().view,
        OverlayView::Recording
    );
    model.on_event(&phase(Phase::Transcribing));
    assert_eq!(
        model.on_event(&phase(Phase::Idle)).unwrap().view,
        OverlayView::Hidden
    );
    // A stale timer from the error does nothing now.
    assert_eq!(model.on_error_expired(), None);
}

#[test]
fn an_error_during_a_newer_recording_replaces_it_and_expires() {
    let mut model = OverlayModel::default();
    model.on_event(&phase(Phase::Recording));
    let change = model.on_event(&failed()).unwrap();
    assert_eq!(change.view, failed_view());
    assert!(change.expires);
}

#[tokio::test(start_paused = true)]
async fn run_error_then_success_never_hides_the_new_dictation_late() {
    let (tx, rx) = mpsc::unbounded_channel();
    let recorder = Recorder::default();
    let task = tokio::spawn(run(rx, recorder.clone(), ERROR_VISIBLE_FOR));
    tx.send(phase(Phase::Recording)).unwrap();
    tx.send(failed()).unwrap();
    tx.send(phase(Phase::Idle)).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    // Second dictation, still inside the error's 6 s, runs past its deadline.
    tx.send(phase(Phase::Recording)).unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    tx.send(phase(Phase::Transcribing)).unwrap();
    tx.send(phase(Phase::Idle)).unwrap();
    tokio::time::sleep(Duration::from_secs(10)).await;
    drop(tx);
    task.await.unwrap();
    assert_eq!(
        recorder.views(),
        [
            OverlayView::Recording,
            failed_view(),
            OverlayView::Recording,
            OverlayView::Transcribing,
            OverlayView::Hidden,
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn run_two_errors_in_a_row_hide_once_six_seconds_after_the_last() {
    let (tx, rx) = mpsc::unbounded_channel();
    let recorder = Recorder::default();
    let task = tokio::spawn(run(rx, recorder.clone(), ERROR_VISIBLE_FOR));
    tx.send(failed()).unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    tx.send(failed()).unwrap();
    tokio::time::sleep(Duration::from_millis(5999)).await;
    assert_eq!(recorder.views().last(), Some(&failed_view()));
    tokio::time::sleep(Duration::from_millis(2)).await;
    drop(tx);
    task.await.unwrap();
    assert_eq!(recorder.views().last(), Some(&OverlayView::Hidden));
}
