//! What the last dictation got through, for onboarding's Try it (U1).
//!
//! Try it shows success only after the app itself reports a completed
//! dictation: recorded, then transcribed, then pasted. [`DictationProgress`]
//! is fed the pipeline's [`UiEvent`]s and answers with a [`ProgressView`]
//! that holds counts and the stages seen, plus the latest error's sentence.
//! It keeps no transcript, no audio and no error code.

use dictation_core::coordinator::Phase;
use dictation_core::diagnostics::ErrorCode;

use crate::pipeline::UiEvent;

/// A stage of one dictation, in the order it happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// The microphone recorded while the hotkey was held.
    Recorded,
    /// The engine turned the recording into text.
    Transcribed,
    /// The text was pasted into the focused app.
    Pasted,
}

/// What a window can ask for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProgressView {
    /// How many dictations started since the app launched.
    pub started: u32,
    /// How many ended pasted, with no error.
    pub completed: u32,
    /// How many errors were shown.
    pub errors: u32,
    /// The stages the current (or last) dictation got through, in order.
    pub stages: Vec<Stage>,
    /// The latest error's sentence (`ErrorCode::user_text`), until the next
    /// recording starts.
    pub error: Option<String>,
}

/// Folds the pipeline's events into a [`ProgressView`].
#[derive(Debug, Default)]
pub struct DictationProgress {
    view: ProgressView,
    /// An error arrived during the current dictation.
    failed: bool,
}

impl DictationProgress {
    /// What the last dictation got through.
    #[must_use]
    pub fn view(&self) -> &ProgressView {
        &self.view
    }

    /// Applies a pipeline event.
    ///
    /// A recording starts a new dictation and clears the error. The engine
    /// has produced text once the pipeline pastes; the dictation is
    /// complete when it goes idle after pasting with no error. An error
    /// (always sent before the idle that follows it) keeps it from
    /// completing.
    pub fn on_event(&mut self, event: &UiEvent) {
        match event {
            UiEvent::StateChanged(Phase::Recording) => {
                self.view.started = self.view.started.saturating_add(1);
                self.view.stages = vec![Stage::Recorded];
                self.view.error = None;
                self.failed = false;
            }
            UiEvent::StateChanged(Phase::Transcribing) => {}
            UiEvent::StateChanged(Phase::Inserting) => {
                if self.view.stages.last() == Some(&Stage::Recorded) {
                    self.view.stages.push(Stage::Transcribed);
                }
            }
            UiEvent::StateChanged(Phase::Idle) => {
                if !self.failed && self.view.stages.last() == Some(&Stage::Transcribed) {
                    self.view.stages.push(Stage::Pasted);
                    self.view.completed = self.view.completed.saturating_add(1);
                }
            }
            UiEvent::Error(error) => {
                self.failed = true;
                self.view.errors = self.view.errors.saturating_add(1);
                self.view.error = Some(ErrorCode::of(error).user_text());
            }
            UiEvent::HotkeyRestored => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dictation_core::error::DictationError;

    fn phase(phase: Phase) -> UiEvent {
        UiEvent::StateChanged(phase)
    }

    fn run(events: &[UiEvent]) -> DictationProgress {
        let mut progress = DictationProgress::default();
        for event in events {
            progress.on_event(event);
        }
        progress
    }

    #[test]
    fn a_full_dictation_completes_with_all_three_stages() {
        let progress = run(&[
            phase(Phase::Recording),
            phase(Phase::Transcribing),
            phase(Phase::Inserting),
            phase(Phase::Idle),
        ]);
        let view = progress.view();
        assert_eq!(view.started, 1);
        assert_eq!(view.completed, 1);
        assert_eq!(
            view.stages,
            [Stage::Recorded, Stage::Transcribed, Stage::Pasted]
        );
        assert_eq!(view.error, None);
    }

    #[test]
    fn nothing_completes_before_the_paste_is_done() {
        let progress = run(&[
            phase(Phase::Recording),
            phase(Phase::Transcribing),
            phase(Phase::Inserting),
        ]);
        assert_eq!(progress.view().completed, 0);
        assert_eq!(
            progress.view().stages,
            [Stage::Recorded, Stage::Transcribed]
        );
    }

    #[test]
    fn a_failed_transcription_is_not_a_completed_dictation() {
        let progress = run(&[
            phase(Phase::Recording),
            phase(Phase::Transcribing),
            UiEvent::Error(DictationError::TranscriptionFailed {
                reason: "The provider rejected your API key.".into(),
            }),
            phase(Phase::Idle),
        ]);
        let view = progress.view();
        assert_eq!(view.completed, 0);
        assert_eq!(view.errors, 1);
        assert_eq!(view.stages, [Stage::Recorded]);
        assert_eq!(view.error, Some(ErrorCode::KeyRejected.user_text()));
    }

    #[test]
    fn a_failed_paste_is_not_a_completed_dictation() {
        let progress = run(&[
            phase(Phase::Recording),
            phase(Phase::Transcribing),
            phase(Phase::Inserting),
            UiEvent::Error(DictationError::InsertFailed {
                reason: "no focus".into(),
            }),
            phase(Phase::Idle),
        ]);
        assert_eq!(progress.view().completed, 0);
        assert_eq!(
            progress.view().stages,
            [Stage::Recorded, Stage::Transcribed]
        );
    }

    #[test]
    fn a_dropped_recording_is_not_a_completed_dictation() {
        let progress = run(&[
            phase(Phase::Recording),
            phase(Phase::Transcribing),
            phase(Phase::Idle),
        ]);
        assert_eq!(progress.view().completed, 0);
    }

    #[test]
    fn the_next_recording_clears_the_error_but_keeps_the_counts() {
        let mut progress = run(&[
            phase(Phase::Recording),
            UiEvent::Error(DictationError::EmptyTranscript),
            phase(Phase::Idle),
        ]);
        assert!(progress.view().error.is_some());
        progress.on_event(&phase(Phase::Recording));
        let view = progress.view();
        assert_eq!(view.error, None);
        assert_eq!(view.errors, 1);
        assert_eq!(view.started, 2);
    }

    #[test]
    fn the_error_is_a_sentence_without_its_code() {
        let progress = run(&[UiEvent::Error(DictationError::HotkeyFailed)]);
        let text = progress.view().error.clone().unwrap();
        assert!(
            !text.contains(ErrorCode::HotkeyUnregistered.as_str()),
            "{text}"
        );
        assert!(text.contains("Vocet"), "{text}");
    }
}
