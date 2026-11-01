//! Tester's edge cases for the coordinator (M1.1), driven through the public
//! API only: boundary times, repeated or out-of-order keys, late results
//! after a cancel, and `PermissionMissing` in every state.

use std::time::Duration;

use dictation_core::coordinator::{MAX_RECORDING, MIN_HOLD, Phase, State, step};
use dictation_core::error::{DictationError, Permission};
use dictation_core::events::{Command, Event};

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

/// Runs `events` from `state`, returning the final state and every command.
fn run(mut state: State, events: impl IntoIterator<Item = Event>) -> (State, Vec<Command>) {
    let mut all = Vec::new();
    for event in events {
        let (next, commands) = step(state, event);
        state = next;
        all.extend(commands);
    }
    (state, all)
}

#[test]
fn hold_of_exactly_min_hold_transcribes_and_one_ms_less_discards() {
    let start = ms(10_000);
    let (state, commands) = run(
        State::Idle,
        [
            Event::HotkeyPressed(start),
            Event::HotkeyReleased(start + MIN_HOLD),
        ],
    );
    assert_eq!(state, State::Transcribing);
    assert_eq!(
        commands,
        [
            Command::StartRecording,
            Command::StateChanged(Phase::Recording),
            Command::StopRecording,
            Command::Transcribe,
            Command::StateChanged(Phase::Transcribing),
        ]
    );

    let (state, commands) = run(
        State::Idle,
        [
            Event::HotkeyPressed(start),
            Event::HotkeyReleased(start + MIN_HOLD - ms(1)),
        ],
    );
    assert_eq!(state, State::Idle);
    assert_eq!(
        commands,
        [
            Command::StartRecording,
            Command::StateChanged(Phase::Recording),
            Command::DiscardRecording,
            Command::StateChanged(Phase::Idle),
        ]
    );
}

#[test]
fn zero_length_hold_is_a_tap() {
    let (state, commands) = run(
        State::Idle,
        [Event::HotkeyPressed(ms(0)), Event::HotkeyReleased(ms(0))],
    );
    assert_eq!(state, State::Idle);
    assert!(commands.contains(&Command::DiscardRecording));
    assert!(!commands.contains(&Command::Transcribe));
}

#[test]
fn ticks_before_the_limit_do_nothing_and_the_limit_is_measured_from_the_press() {
    let start = ms(7_000);
    let recording = State::Recording { started_at: start };
    for elapsed in [ms(0), ms(1), MIN_HOLD, ms(60_000), MAX_RECORDING - ms(1)] {
        assert_eq!(
            step(recording, Event::Tick(start + elapsed)),
            (recording, Vec::new()),
            "Tick after {elapsed:?} must not stop the recording"
        );
    }
    // A Tick 5 minutes after the clock origin, but before 5 minutes of
    // recording, must not stop it.
    let late_start = State::Recording {
        started_at: ms(60_000),
    };
    assert_eq!(
        step(late_start, Event::Tick(MAX_RECORDING)),
        (late_start, Vec::new())
    );
}

#[test]
fn tick_exactly_at_and_past_the_limit_stops_and_transcribes() {
    let start = ms(7_000);
    for elapsed in [MAX_RECORDING, MAX_RECORDING + ms(1), MAX_RECORDING * 3] {
        let (state, commands) = step(
            State::Recording { started_at: start },
            Event::Tick(start + elapsed),
        );
        assert_eq!(state, State::Transcribing);
        assert_eq!(
            commands,
            [
                Command::StopRecording,
                Command::Transcribe,
                Command::StateChanged(Phase::Transcribing),
            ]
        );
    }
}

#[test]
fn a_tick_that_stopped_the_recording_is_not_stopped_twice() {
    let start = ms(0);
    let (state, commands) = run(
        State::Recording { started_at: start },
        [
            Event::Tick(start + MAX_RECORDING),
            Event::Tick(start + MAX_RECORDING + ms(100)),
            Event::HotkeyReleased(start + MAX_RECORDING + ms(200)),
            Event::CancelPressed,
        ],
    );
    assert_eq!(state, State::Transcribing);
    assert_eq!(
        commands
            .iter()
            .filter(|c| matches!(c, Command::StopRecording | Command::DiscardRecording))
            .count(),
        1
    );
}

#[test]
fn repeated_press_without_release_keeps_the_first_press_time() {
    let (state, commands) = run(
        State::Idle,
        [
            Event::HotkeyPressed(ms(1_000)),
            Event::HotkeyPressed(ms(1_100)),
            Event::HotkeyPressed(ms(1_150)),
        ],
    );
    assert_eq!(
        state,
        State::Recording {
            started_at: ms(1_000)
        }
    );
    assert_eq!(
        commands,
        [
            Command::StartRecording,
            Command::StateChanged(Phase::Recording)
        ]
    );
    // The hold counts from the first press: 1_000 -> 1_200 is 200 ms.
    let (state, _) = step(state, Event::HotkeyReleased(ms(1_200)));
    assert_eq!(state, State::Transcribing);
}

#[test]
fn key_repeat_does_not_extend_the_max_recording_limit() {
    let (state, _) = run(
        State::Idle,
        [
            Event::HotkeyPressed(ms(0)),
            Event::HotkeyPressed(MAX_RECORDING - ms(10)),
        ],
    );
    let (state, commands) = step(state, Event::Tick(MAX_RECORDING));
    assert_eq!(state, State::Transcribing);
    assert_eq!(commands[..2], [Command::StopRecording, Command::Transcribe]);
}

#[test]
fn release_or_tick_while_not_recording_is_ignored() {
    for state in [State::Idle, State::Transcribing, State::Inserting] {
        for event in [
            Event::HotkeyReleased(ms(500)),
            Event::Tick(MAX_RECORDING * 2),
        ] {
            assert_eq!(step(state, event.clone()), (state, Vec::new()), "{event:?}");
        }
    }
}

#[test]
fn cancel_when_not_recording_is_ignored_even_mid_pipeline() {
    let (state, commands) = run(
        State::Idle,
        [
            Event::HotkeyPressed(ms(0)),
            Event::HotkeyReleased(ms(500)),
            Event::CancelPressed,
            Event::TranscriptReady("hello".into()),
            Event::CancelPressed,
            Event::InsertDone,
        ],
    );
    assert_eq!(state, State::Idle);
    assert!(!commands.contains(&Command::DiscardRecording));
    assert!(commands.contains(&Command::Insert("hello".into())));
}

#[test]
fn blank_transcripts_of_every_kind_show_empty_transcript() {
    for text in ["", " ", "\n", "\t\r\n ", "\u{3000}", "\u{a0}\u{2003}"] {
        let (state, commands) = step(State::Transcribing, Event::TranscriptReady(text.into()));
        assert_eq!(state, State::Idle, "{text:?}");
        assert_eq!(
            commands,
            [
                Command::ShowError(DictationError::EmptyTranscript),
                Command::StateChanged(Phase::Idle),
            ],
            "{text:?}"
        );
    }
}

#[test]
fn transcript_with_surrounding_whitespace_is_inserted_unchanged() {
    let (state, commands) = step(
        State::Transcribing,
        Event::TranscriptReady("  hi there \n".into()),
    );
    assert_eq!(state, State::Inserting);
    assert_eq!(commands[0], Command::Insert("  hi there \n".into()));
}

#[test]
fn late_results_after_a_cancel_are_ignored_and_the_next_dictation_works() {
    let (state, commands) = run(
        State::Idle,
        [
            Event::HotkeyPressed(ms(0)),
            Event::CancelPressed,
            Event::AudioCaptured,
            Event::TranscriptReady("stale".into()),
            Event::TranscriptionFailed("stale".into()),
            Event::InsertDone,
            Event::InsertFailed("stale".into()),
            Event::HotkeyReleased(ms(800)),
        ],
    );
    assert_eq!(state, State::Idle);
    assert_eq!(
        commands,
        [
            Command::StartRecording,
            Command::StateChanged(Phase::Recording),
            Command::DiscardRecording,
            Command::StateChanged(Phase::Idle),
        ]
    );

    // A late result arriving during the next recording is ignored too.
    let (state, commands) = run(
        state,
        [
            Event::HotkeyPressed(ms(1_000)),
            Event::TranscriptReady("stale".into()),
            Event::HotkeyReleased(ms(1_500)),
            Event::TranscriptReady("fresh".into()),
            Event::InsertDone,
        ],
    );
    assert_eq!(state, State::Idle);
    assert!(!commands.contains(&Command::Insert("stale".into())));
    assert!(commands.contains(&Command::Insert("fresh".into())));
}

#[test]
fn a_late_transcript_while_inserting_does_not_insert_twice() {
    let (state, commands) = run(
        State::Inserting,
        [
            Event::TranscriptReady("again".into()),
            Event::TranscriptionFailed("x".into()),
        ],
    );
    assert_eq!(state, State::Inserting);
    assert!(commands.is_empty());
}

#[test]
fn permission_missing_in_each_state() {
    for permission in [Permission::Microphone, Permission::Accessibility] {
        let error = Command::ShowError(DictationError::PermissionMissing(permission));

        assert_eq!(
            step(State::Idle, Event::PermissionMissing(permission)),
            (State::Idle, vec![error.clone()])
        );
        assert_eq!(
            step(
                State::Recording { started_at: ms(0) },
                Event::PermissionMissing(permission)
            ),
            (
                State::Idle,
                vec![
                    Command::DiscardRecording,
                    error.clone(),
                    Command::StateChanged(Phase::Idle)
                ]
            )
        );
        // Since M2.3 (#81 M2): while transcribing it means the recording's
        // start had failed for want of the permission, so the dictation fails.
        assert_eq!(
            step(State::Transcribing, Event::PermissionMissing(permission)),
            (
                State::Idle,
                vec![error.clone(), Command::StateChanged(Phase::Idle)]
            )
        );
        assert_eq!(
            step(State::Inserting, Event::PermissionMissing(permission)),
            (State::Inserting, Vec::new())
        );
    }
}

#[test]
fn back_to_back_dictations_each_start_and_finish_cleanly() {
    let mut state = State::Idle;
    let mut now = ms(0);
    for round in 0..3 {
        let text = format!("round {round}");
        let (next, commands) = run(
            state,
            [
                Event::HotkeyPressed(now),
                Event::HotkeyReleased(now + ms(300)),
                Event::TranscriptReady(text.clone()),
                Event::InsertDone,
            ],
        );
        state = next;
        now += ms(1_000);
        assert_eq!(state, State::Idle);
        assert_eq!(
            commands,
            [
                Command::StartRecording,
                Command::StateChanged(Phase::Recording),
                Command::StopRecording,
                Command::Transcribe,
                Command::StateChanged(Phase::Transcribing),
                Command::Insert(text),
                Command::StateChanged(Phase::Inserting),
                Command::StateChanged(Phase::Idle),
            ]
        );
    }
}
