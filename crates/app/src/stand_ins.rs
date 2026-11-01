//! Stand-ins for the ports that have no real adapter yet, so the shell can
//! run the pipeline end to end (M1.5): transcription and paste, and audio
//! for tests. M2.2 brought the real microphone; [`MicrophoneOnly`] checks
//! its permission while paste is still a stand-in.
//! Each is replaced by its real adapter in a later milestone. Unlike the
//! `platform` fakes they can't be scripted; they just let a dictation run.

use std::time::Instant;

use dictation_ports::{
    Audio, AudioError, AudioSource, InsertError, Permission, PermissionError, PermissionStatus,
    Permissions, TextInserter, TranscribeError, Transcriber,
};

/// The sample rate of [`ToneAudio`]'s recordings.
const SAMPLE_RATE: u32 = 16_000;

/// The text [`FixedTranscriber`] returns.
pub const FIXED_TRANSCRIPT: &str = "This is a test dictation.";

/// Records a tone: [`stop`](AudioSource::stop) returns one second of a
/// 440 Hz tone at 16 kHz, however long the recording lasted. It is loud and
/// long enough for the pipeline to send it on to the transcriber; real
/// silence never gets that far ([`dictation_core::audio_math::assess`]).
#[derive(Debug, Default)]
pub struct ToneAudio {
    started: Option<Instant>,
}

impl AudioSource for ToneAudio {
    fn start(&mut self) -> Result<(), AudioError> {
        if self.started.is_some() {
            return Err(AudioError::AlreadyRecording);
        }
        self.started = Some(Instant::now());
        Ok(())
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        self.started.take().ok_or(AudioError::NotRecording)?;
        let samples = (0..SAMPLE_RATE)
            .map(|n| {
                #[allow(clippy::cast_precision_loss, reason = "n < 16_000")]
                let t = n as f32 / SAMPLE_RATE as f32;
                0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();
        Ok(Audio {
            sample_rate: SAMPLE_RATE,
            samples,
        })
    }

    fn discard(&mut self) {
        self.started = None;
    }

    fn is_recording(&self) -> bool {
        self.started.is_some()
    }
}

/// Answers every transcription with [`FIXED_TRANSCRIPT`].
#[derive(Debug, Default, Clone, Copy)]
pub struct FixedTranscriber;

impl Transcriber for FixedTranscriber {
    async fn transcribe(&self, _audio: &Audio) -> Result<String, TranscribeError> {
        Ok(FIXED_TRANSCRIPT.to_owned())
    }
}

/// Pastes nothing; logs only the text's length (at debug).
#[derive(Debug, Default, Clone, Copy)]
pub struct DiscardingInserter;

impl TextInserter for DiscardingInserter {
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        tracing::debug!(
            chars = text.chars().count(),
            "stand-in inserter: not pasting"
        );
        Ok(())
    }
}

/// Reports every permission granted: the stand-ins above need none. The
/// real hotkey still needs Accessibility and says so when it can't start.
#[derive(Debug, Default, Clone, Copy)]
pub struct GrantedPermissions;

impl Permissions for GrantedPermissions {
    fn check(&self, _permission: Permission) -> PermissionStatus {
        PermissionStatus::Granted
    }

    fn request(&self, _permission: Permission) -> Result<(), PermissionError> {
        Ok(())
    }
}

/// Checks only the microphone with the real `P`; reports Accessibility
/// granted, since the stand-in inserter doesn't paste. Replaced by the real
/// permissions once the real inserter lands.
#[derive(Debug, Default, Clone, Copy)]
pub struct MicrophoneOnly<P>(pub P);

impl<P: Permissions> Permissions for MicrophoneOnly<P> {
    fn check(&self, permission: Permission) -> PermissionStatus {
        match permission {
            Permission::Microphone => self.0.check(permission),
            Permission::Accessibility => PermissionStatus::Granted,
        }
    }

    fn request(&self, permission: Permission) -> Result<(), PermissionError> {
        match permission {
            Permission::Microphone => self.0.request(permission),
            Permission::Accessibility => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_audio_records_a_second_of_tone_once_started() {
        let mut audio = ToneAudio::default();
        assert_eq!(audio.stop(), Err(AudioError::NotRecording));
        audio.start().unwrap();
        assert!(audio.is_recording());
        assert_eq!(audio.start(), Err(AudioError::AlreadyRecording));
        let captured = audio.stop().unwrap();
        assert_eq!(captured.sample_rate, 16_000);
        assert_eq!(captured.samples.len(), 16_000);
        assert_eq!(
            dictation_core::audio_math::assess(
                &captured.samples,
                captured.sample_rate,
                dictation_core::audio_math::DEFAULT_SILENCE_THRESHOLD
            ),
            dictation_core::audio_math::Verdict::Transcribe
        );

        audio.start().unwrap();
        audio.discard();
        assert!(!audio.is_recording());
        assert_eq!(audio.stop(), Err(AudioError::NotRecording));
    }

    #[tokio::test]
    async fn transcriber_inserter_and_permissions_always_succeed() {
        let audio = Audio {
            sample_rate: 16_000,
            samples: vec![],
        };
        assert_eq!(
            FixedTranscriber.transcribe(&audio).await.unwrap(),
            FIXED_TRANSCRIPT
        );
        assert_eq!(DiscardingInserter.insert("anything"), Ok(()));
        for permission in [Permission::Microphone, Permission::Accessibility] {
            assert_eq!(
                GrantedPermissions.check(permission),
                PermissionStatus::Granted
            );
            assert_eq!(GrantedPermissions.request(permission), Ok(()));
        }
    }

    #[test]
    fn microphone_only_checks_and_requests_just_the_microphone() {
        use dictation_platform::fakes::FakePermissions;

        let real = FakePermissions::new();
        real.set(Permission::Microphone, PermissionStatus::Denied);
        let permissions = MicrophoneOnly(real.clone());
        assert_eq!(
            permissions.check(Permission::Microphone),
            PermissionStatus::Denied
        );
        assert_eq!(
            permissions.check(Permission::Accessibility),
            PermissionStatus::Granted
        );
        permissions.request(Permission::Accessibility).unwrap();
        assert!(real.requests().is_empty());
        permissions.request(Permission::Microphone).unwrap();
        assert_eq!(real.requests(), [Permission::Microphone]);
    }
}
