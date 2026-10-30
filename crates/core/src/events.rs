//! What goes into the coordinator ([`Event`]) and what comes out
//! ([`Command`]).
//!
//! Times are [`Duration`]s since an arbitrary monotonic origin chosen by the
//! app layer (its `Clock`), so the coordinator never reads a clock itself.

use std::time::Duration;

use crate::coordinator::Phase;
use crate::error::{DictationError, Permission};

/// Something that happened, reported to the coordinator by the app layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The dictation hotkey went down at this time.
    HotkeyPressed(Duration),
    /// The dictation hotkey came up at this time.
    HotkeyReleased(Duration),
    /// The user pressed the cancel key (Esc).
    CancelPressed,
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
    /// A hotkey press found a permission missing, so recording must not start.
    PermissionMissing(Permission),
}

/// Something the app layer must do, emitted by the coordinator.
#[derive(Debug, Clone, PartialEq, Eq)]
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
