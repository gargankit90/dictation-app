//! Tester's property test for the coordinator (M1.1, criterion 8), with
//! realistic gaps between events: mostly human-scale (taps under 200 ms,
//! holds of a few seconds) plus some long enough to hit the 5-minute limit.
//! The unit proptest draws gaps uniformly up to 400 s, so it almost never
//! produces a tap shorter than `MIN_HOLD`.

use std::time::Duration;

use dictation_core::coordinator::{MAX_RECORDING, MIN_HOLD, State, step};
use dictation_core::error::Permission;
use dictation_core::events::{Command, Event};
use proptest::prelude::*;

fn gap_ms() -> impl Strategy<Value = u64> {
    prop_oneof![
        4 => 0u64..400,
        3 => 0u64..10_000,
        1 => 290_000u64..310_000,
    ]
}

fn event_at(kind: u8, now: Duration) -> Event {
    match kind {
        0 | 1 => Event::HotkeyPressed(now),
        2 | 3 => Event::HotkeyReleased(now),
        4 => Event::CancelPressed,
        5 => Event::Tick(now),
        6 => Event::AudioCaptured,
        7 => Event::TranscriptReady("hello".into()),
        8 => Event::TranscriptReady(" ".into()),
        9 => Event::TranscriptionFailed("failed".into()),
        10 => Event::InsertDone,
        11 => Event::InsertFailed("failed".into()),
        _ => Event::PermissionMissing(Permission::Microphone),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn recorder_tracks_state_and_holds_obey_min_hold(
        events in prop::collection::vec((gap_ms(), 0u8..13), 0..48)
    ) {
        let mut state = State::Idle;
        let mut recording = false;
        let mut now = Duration::ZERO;
        let mut pressed_at = None;
        let mut steps = events
            .into_iter()
            .map(|(gap, kind)| {
                now += Duration::from_millis(gap);
                event_at(kind, now)
            })
            .collect::<Vec<_>>();
        // Finish whatever is in flight.
        now += MAX_RECORDING;
        steps.extend([
            Event::HotkeyReleased(now),
            Event::TranscriptReady("done".into()),
            Event::InsertDone,
        ]);

        for event in steps {
            let before = state;
            let (next, commands) = step(state, event.clone());
            for command in &commands {
                match command {
                    Command::StartRecording => {
                        prop_assert!(!recording, "second StartRecording");
                        recording = true;
                        if let Event::HotkeyPressed(t) = event {
                            pressed_at = Some(t);
                        }
                    }
                    Command::StopRecording | Command::DiscardRecording => {
                        prop_assert!(recording, "stopped a recording that wasn't running");
                        recording = false;
                    }
                    _ => {}
                }
            }
            prop_assert_eq!(recording, matches!(next, State::Recording { .. }));

            // A release from Recording transcribes iff the hold was long enough.
            if let (State::Recording { .. }, Event::HotkeyReleased(t)) = (before, &event) {
                let held = t.saturating_sub(pressed_at.unwrap());
                let transcribes = commands.contains(&Command::Transcribe);
                prop_assert_eq!(transcribes, held >= MIN_HOLD, "held {:?}", held);
                prop_assert_eq!(commands.contains(&Command::DiscardRecording), !transcribes);
            }
            state = next;
        }
        prop_assert_eq!(state, State::Idle);
        prop_assert!(!recording);
    }
}
