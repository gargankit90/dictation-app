//! H2 Tester: the "No sound" message with odd microphone names, a key press
//! while it shows, and a silent hold followed by a normal dictation.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::time::Duration;

use dictation_core::coordinator::Phase;
use dictation_pipeline::overlay::{OverlayModel, OverlayView};
use dictation_pipeline::pipeline::UiEvent;

fn silent_hold(model: &mut OverlayModel, device: Option<&str>) -> OverlayView {
    model.on_event(&UiEvent::StateChanged(Phase::Recording));
    model.on_level(-90.0, device, Duration::ZERO);
    model.on_event(&UiEvent::StateChanged(Phase::Transcribing));
    let change = model.on_event(&UiEvent::NoSound).unwrap();
    assert!(change.expires, "hides by itself");
    model.on_event(&UiEvent::StateChanged(Phase::Idle));
    model.view().clone()
}

fn message(view: &OverlayView) -> &str {
    let OverlayView::Error { message } = view else {
        unreachable!("expected the error look, got {view:?}");
    };
    message
}

#[test]
fn odd_microphone_names_render_verbatim() {
    let long = "M".repeat(5_000);
    for name in [
        "ÉTÉ マイク 🎤 مايك",
        "line\nbreak\ttab",
        "<b>html</b> & \"quotes\"",
        long.as_str(),
    ] {
        let mut model = OverlayModel::default();
        let view = silent_hold(&mut model, Some(name));
        assert_eq!(
            message(&view),
            format!("No sound · Check your microphone ({name})")
        );
    }
}

#[test]
fn a_press_while_it_shows_records_and_the_old_timer_hides_nothing() {
    let mut model = OverlayModel::default();
    silent_hold(&mut model, Some("USB Mic"));
    let change = model
        .on_event(&UiEvent::StateChanged(Phase::Recording))
        .unwrap();
    assert_eq!(change.view, OverlayView::Recording);
    assert!(!change.expires);
    assert_eq!(model.on_error_expired(), None);
    assert_eq!(model.view(), &OverlayView::Recording);
}

#[test]
fn a_silent_hold_then_a_normal_dictation_ends_on_done_not_the_old_message() {
    let mut model = OverlayModel::default();
    silent_hold(&mut model, Some("USB Mic"));
    for phase in [Phase::Recording, Phase::Transcribing, Phase::Inserting] {
        model.on_event(&UiEvent::StateChanged(phase));
    }
    model.on_event(&UiEvent::StateChanged(Phase::Idle));
    assert_eq!(model.view(), &OverlayView::PasteSent);
}

#[test]
fn a_cancelled_or_tapped_recording_shows_nothing() {
    let mut model = OverlayModel::default();
    silent_hold(&mut model, Some("USB Mic"));
    model.on_event(&UiEvent::StateChanged(Phase::Recording));
    model.on_level(-90.0, Some("USB Mic"), Duration::ZERO);
    // Esc: straight to Idle.
    model.on_event(&UiEvent::StateChanged(Phase::Idle));
    assert_eq!(model.view(), &OverlayView::Hidden);
}

#[test]
fn blank_names_have_no_brackets_and_a_stale_message_does_not_return() {
    for name in ["", "   "] {
        let mut model = OverlayModel::default();
        let view = silent_hold(&mut model, Some(name));
        assert_eq!(message(&view), "No sound · Check your microphone");
    }
    // Silent hold, then a tap (Recording → Idle) and then Esc: stays hidden.
    let mut model = OverlayModel::default();
    silent_hold(&mut model, Some("USB Mic"));
    model.on_event(&UiEvent::StateChanged(Phase::Recording));
    model.on_event(&UiEvent::StateChanged(Phase::Transcribing));
    model.on_event(&UiEvent::StateChanged(Phase::Idle));
    assert_eq!(model.view(), &OverlayView::Hidden);
    assert_eq!(model.on_error_expired(), None);
}
