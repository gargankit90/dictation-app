//! What goes into the coordinator ([`Event`]) and what comes out
//! ([`Command`]).
//!
//! Times are [`Duration`]s since an arbitrary monotonic origin chosen by the
//! app layer (its `Clock`), so the coordinator never reads a clock itself.

use std::fmt;
use std::time::Duration;

use crate::coordinator::Phase;
use crate::error::{DictationError, Permission};

/// Something that happened, reported to the coordinator by the app layer.
///
/// `Debug` shows a transcript's length, never its text, so events can be
/// logged (`docs/ENGINEERING.md`).
#[derive(Clone, PartialEq, Eq)]
pub enum Event {
    /// The dictation hotkey went down at this time.
    HotkeyPressed(Duration),
    /// The dictation hotkey came up at this time.
    HotkeyReleased(Duration),
    /// The user pressed the cancel key (Esc).
    CancelPressed,
    /// The hotkey source lost track of the keyboard, so the hotkey's
    /// release may never arrive: a recording in progress is dropped.
    HotkeyInterrupted,
    /// The hotkey source stopped listening after an error: no more presses
    /// will arrive until it is started again.
    HotkeyFailed,
    /// The current time, sent periodically while recording.
    Tick(Duration),
    /// The audio source delivered captured audio. The coordinator ignores
    /// it for now: a long enough release already emits `StopRecording` and
    /// `Transcribe`, and the app layer hands the audio to the transcriber
    /// itself. It stays in the enum so the app layer can report it (and a
    /// later milestone can gate `Transcribe` on it) without an API change.
    AudioCaptured,
    /// The transcriber returned text (possibly empty).
    TranscriptReady(String),
    /// The transcriber failed; `reason` is a short, user-safe summary.
    TranscriptionFailed(String),
    /// The text was inserted into the focused app.
    InsertDone,
    /// Inserting failed; `reason` is a short, user-safe summary.
    InsertFailed(String),
    /// A required permission is missing: on a hotkey press, so recording
    /// must not start, or revoked while recording, which discards it.
    PermissionMissing(Permission),
}

/// Something the app layer must do, emitted by the coordinator.
///
/// `Debug` shows the length of text to insert, never the text.
#[derive(Clone, PartialEq, Eq)]
pub enum Command {
    /// Start capturing audio from the microphone.
    StartRecording,
    /// Stop capturing and keep the audio for transcription.
    StopRecording,
    /// Stop capturing (if still running) and drop the audio.
    DiscardRecording,
    /// Send the kept audio to the transcriber.
    Transcribe,
    /// Insert this text into the focused app.
    Insert(String),
    /// Tell the user something went wrong.
    ShowError(DictationError),
    /// The coordinator moved to this phase (for the UI and tray).
    StateChanged(Phase),
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::HotkeyPressed(at) => f.debug_tuple("HotkeyPressed").field(at).finish(),
            Event::HotkeyReleased(at) => f.debug_tuple("HotkeyReleased").field(at).finish(),
            Event::CancelPressed => f.write_str("CancelPressed"),
            Event::HotkeyInterrupted => f.write_str("HotkeyInterrupted"),
            Event::HotkeyFailed => f.write_str("HotkeyFailed"),
            Event::Tick(now) => f.debug_tuple("Tick").field(now).finish(),
            Event::AudioCaptured => f.write_str("AudioCaptured"),
            Event::TranscriptReady(text) => f
                .debug_struct("TranscriptReady")
                .field("len", &text.len())
                .finish(),
            Event::TranscriptionFailed(reason) => {
                f.debug_tuple("TranscriptionFailed").field(reason).finish()
            }
            Event::InsertDone => f.write_str("InsertDone"),
            Event::InsertFailed(reason) => f.debug_tuple("InsertFailed").field(reason).finish(),
            Event::PermissionMissing(permission) => f
                .debug_tuple("PermissionMissing")
                .field(permission)
                .finish(),
        }
    }
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::StartRecording => f.write_str("StartRecording"),
            Command::StopRecording => f.write_str("StopRecording"),
            Command::DiscardRecording => f.write_str("DiscardRecording"),
            Command::Transcribe => f.write_str("Transcribe"),
            Command::Insert(text) => f.debug_struct("Insert").field("len", &text.len()).finish(),
            Command::ShowError(error) => f.debug_tuple("ShowError").field(error).finish(),
            Command::StateChanged(phase) => f.debug_tuple("StateChanged").field(phase).finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_shows_transcript_length_not_text() {
        let event = format!("{:?}", Event::TranscriptReady("secret words".into()));
        let command = format!("{:?}", Command::Insert("secret words".into()));
        assert_eq!(event, "TranscriptReady { len: 12 }");
        assert_eq!(command, "Insert { len: 12 }");
    }

    #[test]
    fn debug_of_other_variants_is_unchanged() {
        let ms = Duration::from_millis(5);
        let events = [
            (Event::HotkeyPressed(ms), "HotkeyPressed(5ms)"),
            (Event::HotkeyReleased(ms), "HotkeyReleased(5ms)"),
            (Event::CancelPressed, "CancelPressed"),
            (Event::HotkeyInterrupted, "HotkeyInterrupted"),
            (Event::HotkeyFailed, "HotkeyFailed"),
            (Event::Tick(ms), "Tick(5ms)"),
            (Event::AudioCaptured, "AudioCaptured"),
            (
                Event::TranscriptionFailed("x".into()),
                "TranscriptionFailed(\"x\")",
            ),
            (Event::InsertDone, "InsertDone"),
            (Event::InsertFailed("x".into()), "InsertFailed(\"x\")"),
            (
                Event::PermissionMissing(Permission::Microphone),
                "PermissionMissing(Microphone)",
            ),
        ];
        for (event, debug) in events {
            assert_eq!(format!("{event:?}"), debug);
        }
        let commands = [
            (Command::StartRecording, "StartRecording"),
            (Command::StopRecording, "StopRecording"),
            (Command::DiscardRecording, "DiscardRecording"),
            (Command::Transcribe, "Transcribe"),
            (
                Command::ShowError(DictationError::EmptyTranscript),
                "ShowError(EmptyTranscript)",
            ),
            (Command::StateChanged(Phase::Idle), "StateChanged(Idle)"),
        ];
        for (command, debug) in commands {
            assert_eq!(format!("{command:?}"), debug);
        }
    }
}
