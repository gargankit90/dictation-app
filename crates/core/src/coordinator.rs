//! The coordinator state machine: [`step`] turns a [`State`] and an
//! [`Event`] into the next state and the [`Command`]s the app layer runs.
//!
//! It is a pure function: no clock, I/O, async or threads. Every time it
//! needs arrives inside an event.

use std::time::Duration;

use crate::error::DictationError;
use crate::events::{Command, Event};

/// Releases sooner than this after the press are accidental taps.
pub const MIN_HOLD: Duration = Duration::from_millis(200);

/// A recording stops by itself once it has run this long.
pub const MAX_RECORDING: Duration = Duration::from_mins(5);

/// Where the coordinator is, with what it must remember there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    /// Waiting for the hotkey.
    #[default]
    Idle,
    /// Capturing audio since `started_at`.
    Recording {
        /// When the hotkey went down.
        started_at: Duration,
    },
    /// Waiting for the transcriber.
    Transcribing,
    /// Waiting for the text to be inserted.
    Inserting,
}

/// [`State`] without its data, for the UI ([`Command::StateChanged`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// See [`State::Idle`].
    Idle,
    /// See [`State::Recording`].
    Recording,
    /// See [`State::Transcribing`].
    Transcribing,
    /// See [`State::Inserting`].
    Inserting,
}

impl State {
    /// This state's [`Phase`].
    #[must_use]
    pub fn phase(self) -> Phase {
        match self {
            State::Idle => Phase::Idle,
            State::Recording { .. } => Phase::Recording,
            State::Transcribing => Phase::Transcribing,
            State::Inserting => Phase::Inserting,
        }
    }
}

/// Advances the coordinator by one event.
///
/// Returns the next state and the commands to run, in order. A
/// [`Command::StateChanged`] comes last whenever the phase changes. Events
/// that don't apply to the current state (a press while busy, a late result
/// after a cancel) are ignored: same state, no commands.
#[must_use]
pub fn step(state: State, event: Event) -> (State, Vec<Command>) {
    let (next, mut commands) = transition(state, event);
    if next.phase() != state.phase() {
        commands.push(Command::StateChanged(next.phase()));
    }
    (next, commands)
}

fn transition(state: State, event: Event) -> (State, Vec<Command>) {
    match (state, event) {
        (State::Idle, Event::HotkeyPressed(now)) => (
            State::Recording { started_at: now },
            vec![Command::StartRecording],
        ),
        (State::Idle, Event::PermissionMissing(permission)) => (
            State::Idle,
            vec![Command::ShowError(DictationError::PermissionMissing(
                permission,
            ))],
        ),
        (State::Recording { started_at }, Event::HotkeyReleased(now)) => {
            if now.saturating_sub(started_at) < MIN_HOLD {
                (State::Idle, vec![Command::DiscardRecording])
            } else {
                stop_and_transcribe()
            }
        }
        (State::Recording { started_at }, Event::Tick(now))
            if now.saturating_sub(started_at) >= MAX_RECORDING =>
        {
            stop_and_transcribe()
        }
        (State::Recording { .. }, Event::CancelPressed) => {
            (State::Idle, vec![Command::DiscardRecording])
        }
        (State::Recording { .. }, Event::PermissionMissing(permission)) => (
            State::Idle,
            vec![
                Command::DiscardRecording,
                Command::ShowError(DictationError::PermissionMissing(permission)),
            ],
        ),
        (State::Transcribing, Event::TranscriptReady(text)) => {
            if text.trim().is_empty() {
                fail(DictationError::EmptyTranscript)
            } else {
                (State::Inserting, vec![Command::Insert(text)])
            }
        }
        (State::Transcribing, Event::TranscriptionFailed(reason)) => {
            fail(DictationError::TranscriptionFailed { reason })
        }
        (State::Inserting, Event::InsertDone) => (State::Idle, Vec::new()),
        (State::Inserting, Event::InsertFailed(reason)) => {
            fail(DictationError::InsertFailed { reason })
        }
        // Everything else doesn't apply here: key repeats while recording,
        // presses while busy, Ticks before the limit, AudioCaptured (the app
        // layer routes the audio itself) and results that arrive late.
        (state, _) => (state, Vec::new()),
    }
}

fn stop_and_transcribe() -> (State, Vec<Command>) {
    (
        State::Transcribing,
        vec![Command::StopRecording, Command::Transcribe],
    )
}

fn fail(error: DictationError) -> (State, Vec<Command>) {
    (State::Idle, vec![Command::ShowError(error)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Permission;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn recording_since(millis: u64) -> State {
        State::Recording {
            started_at: ms(millis),
        }
    }

    #[test]
    fn criterion_1_press_then_hold_release_starts_then_stops_and_transcribes() {
        let (state, commands) = step(State::Idle, Event::HotkeyPressed(ms(1_000)));
        assert_eq!(state, recording_since(1_000));
        assert_eq!(
            commands,
            [
                Command::StartRecording,
                Command::StateChanged(Phase::Recording)
            ]
        );

        let (state, commands) = step(state, Event::HotkeyReleased(ms(1_200)));
        assert_eq!(state, State::Transcribing);
        assert_eq!(
            commands,
            [
                Command::StopRecording,
                Command::Transcribe,
                Command::StateChanged(Phase::Transcribing)
            ]
        );
    }

    #[test]
    fn criterion_2_tap_shorter_than_min_hold_discards_and_returns_to_idle() {
        let (state, commands) = step(recording_since(1_000), Event::HotkeyReleased(ms(1_199)));
        assert_eq!(state, State::Idle);
        assert_eq!(
            commands,
            [
                Command::DiscardRecording,
                Command::StateChanged(Phase::Idle)
            ]
        );
    }

    #[test]
    fn criterion_2_release_before_press_time_counts_as_a_tap() {
        let (state, commands) = step(recording_since(1_000), Event::HotkeyReleased(ms(900)));
        assert_eq!(state, State::Idle);
        assert_eq!(commands[0], Command::DiscardRecording);
    }

    #[test]
    fn criterion_3_press_while_transcribing_or_inserting_is_ignored() {
        for state in [State::Transcribing, State::Inserting] {
            for event in [
                Event::HotkeyPressed(ms(5_000)),
                Event::PermissionMissing(Permission::Microphone),
            ] {
                assert_eq!(step(state, event), (state, Vec::new()));
            }
        }
    }

    #[test]
    fn criterion_4_cancel_during_recording_discards_and_returns_to_idle() {
        let (state, commands) = step(recording_since(1_000), Event::CancelPressed);
        assert_eq!(state, State::Idle);
        assert_eq!(
            commands,
            [
                Command::DiscardRecording,
                Command::StateChanged(Phase::Idle)
            ]
        );
    }

    #[test]
    fn criterion_4_cancel_outside_recording_is_ignored() {
        for state in [State::Idle, State::Transcribing, State::Inserting] {
            assert_eq!(step(state, Event::CancelPressed), (state, Vec::new()));
        }
    }

    #[test]
    fn criterion_5_tick_at_max_recording_stops_and_transcribes() {
        let started = recording_since(1_000);
        let limit = ms(1_000) + MAX_RECORDING;

        let just_before = step(started, Event::Tick(ms(999) + MAX_RECORDING));
        assert_eq!(just_before, (started, Vec::new()));

        let (state, commands) = step(started, Event::Tick(limit));
        assert_eq!(state, State::Transcribing);
        assert_eq!(
            commands,
            [
                Command::StopRecording,
                Command::Transcribe,
                Command::StateChanged(Phase::Transcribing)
            ]
        );

        // The release that eventually follows is ignored.
        assert_eq!(
            step(state, Event::HotkeyReleased(limit + ms(10))),
            (State::Transcribing, Vec::new())
        );
    }

    #[test]
    fn criterion_6_transcription_failed_shows_error_and_returns_to_idle() {
        let (state, commands) = step(
            State::Transcribing,
            Event::TranscriptionFailed("timed out".into()),
        );
        assert_eq!(state, State::Idle);
        assert_eq!(
            commands,
            [
                Command::ShowError(DictationError::TranscriptionFailed {
                    reason: "timed out".into()
                }),
                Command::StateChanged(Phase::Idle)
            ]
        );
    }

    #[test]
    fn criterion_6_empty_transcript_shows_error_and_returns_to_idle() {
        for text in ["", "  \n\t"] {
            let (state, commands) = step(State::Transcribing, Event::TranscriptReady(text.into()));
            assert_eq!(state, State::Idle);
            assert_eq!(
                commands,
                [
                    Command::ShowError(DictationError::EmptyTranscript),
                    Command::StateChanged(Phase::Idle)
                ]
            );
        }
    }

    #[test]
    fn criterion_6_insert_failed_shows_error_and_returns_to_idle() {
        let (state, commands) = step(State::Inserting, Event::InsertFailed("no focus".into()));
        assert_eq!(state, State::Idle);
        assert_eq!(
            commands,
            [
                Command::ShowError(DictationError::InsertFailed {
                    reason: "no focus".into()
                }),
                Command::StateChanged(Phase::Idle)
            ]
        );
    }

    #[test]
    fn criterion_7_permission_missing_on_press_shows_error_and_never_records() {
        let (state, commands) = step(
            State::Idle,
            Event::PermissionMissing(Permission::Microphone),
        );
        assert_eq!(state, State::Idle);
        assert_eq!(
            commands,
            [Command::ShowError(DictationError::PermissionMissing(
                Permission::Microphone
            ))]
        );
    }

    #[test]
    fn permission_lost_while_recording_discards_and_shows_error() {
        let (state, commands) = step(
            recording_since(0),
            Event::PermissionMissing(Permission::Microphone),
        );
        assert_eq!(state, State::Idle);
        assert_eq!(
            commands,
            [
                Command::DiscardRecording,
                Command::ShowError(DictationError::PermissionMissing(Permission::Microphone)),
                Command::StateChanged(Phase::Idle)
            ]
        );
    }

    #[test]
    fn transcript_is_inserted_then_insert_done_returns_to_idle() {
        let (state, commands) = step(State::Transcribing, Event::TranscriptReady("hello".into()));
        assert_eq!(state, State::Inserting);
        assert_eq!(
            commands,
            [
                Command::Insert("hello".into()),
                Command::StateChanged(Phase::Inserting)
            ]
        );

        let (state, commands) = step(state, Event::InsertDone);
        assert_eq!(state, State::Idle);
        assert_eq!(commands, [Command::StateChanged(Phase::Idle)]);
    }

    #[test]
    fn late_results_in_the_wrong_state_are_ignored() {
        let late = [
            Event::TranscriptReady("hello".into()),
            Event::TranscriptionFailed("timed out".into()),
            Event::InsertDone,
            Event::InsertFailed("no focus".into()),
        ];
        for state in [State::Idle, recording_since(0)] {
            for event in late.clone() {
                assert_eq!(step(state, event), (state, Vec::new()));
            }
        }
    }

    #[test]
    fn audio_captured_and_key_repeats_are_ignored() {
        let recording = recording_since(0);
        for (state, event) in [
            (recording, Event::AudioCaptured),
            (recording, Event::HotkeyPressed(ms(50))),
            (State::Idle, Event::HotkeyReleased(ms(50))),
            (State::Idle, Event::Tick(ms(50))),
            (State::Transcribing, Event::AudioCaptured),
        ] {
            assert_eq!(step(state, event), (state, Vec::new()));
        }
    }

    #[test]
    fn default_state_is_idle() {
        assert_eq!(State::default(), State::Idle);
    }
}
