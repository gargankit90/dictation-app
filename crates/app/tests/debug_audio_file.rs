//! M2.2 criterion 1 without a GUI: a launch with `--debug-audio-file <wav>`
//! records that file instead of the microphone, wired the way the shell
//! does it, and the pipeline hands the file's samples to the transcriber.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_core::coordinator::{MIN_HOLD, Phase};
use dictation_pipeline::debug_audio::SavingAudio;
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{DiscardingInserter, GrantedPermissions};
use dictation_pipeline::test_hooks::{SyntheticHotkey, debug_audio_file, hook_events};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{FakeClock, FakeHotkey, FileAudioSource};
use dictation_ports::{Audio, AudioSource, HotkeyError, TranscribeError, Transcriber};

const WAIT: Duration = Duration::from_secs(5);

/// Records every clip it's given and answers with fixed text.
#[derive(Clone, Default)]
struct RecordingTranscriber(Arc<Mutex<Vec<Audio>>>);

impl Transcriber for RecordingTranscriber {
    fn transcribe(
        &self,
        audio: &Audio,
    ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
        self.0.lock().unwrap().push(audio.clone());
        async { Ok("text".to_owned()) }
    }
}

/// A file in the temp dir, deleted on drop.
struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Self {
        Self(std::env::temp_dir().join(format!(
            "dictation-debug-audio-file-{}-{name}",
            std::process::id()
        )))
    }

    /// One second of a 16 kHz ramp, so every sample is distinct.
    fn wav(name: &str) -> (Self, Vec<i16>) {
        let temp = Self::new(name);
        let samples: Vec<i16> = (0..16_000).map(|n| (n % 20_000) as i16 - 8_000).collect();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&temp.0, spec).unwrap();
        for &sample in &samples {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        (temp, samples)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn next(ui: &mut UiReceiver) -> UiEvent {
    tokio::time::timeout(WAIT, ui.recv())
        .await
        .unwrap()
        .unwrap()
}

/// Runs one `--press`, `--release` dictation through the pipeline with
/// `audio` and returns what the transcriber was given.
async fn dictate(audio: impl AudioSource + 'static) -> Vec<Audio> {
    // As in the VM: no Accessibility, so only the test flags drive it.
    let real = FakeHotkey::new();
    real.fail_next_start(HotkeyError::PermissionDenied);
    let (hotkey, injector) = SyntheticHotkey::new(real);
    let clock = FakeClock::new();
    let transcriber = RecordingTranscriber::default();
    let ports = Ports {
        hotkey,
        audio,
        transcriber: transcriber.clone(),
        inserter: DiscardingInserter,
        permissions: GrantedPermissions,
        clock: clock.clone(),
    };
    let (pipeline, mut ui) = Pipeline::spawn(ports, Config::default()).unwrap();
    for event in hook_events(["dictation-app", "--press"]) {
        assert!(injector.inject(event));
    }
    assert_eq!(next(&mut ui).await, UiEvent::StateChanged(Phase::Recording));
    clock.advance(MIN_HOLD * 2);
    for event in hook_events(["dictation-app", "--release"]) {
        assert!(injector.inject(event));
    }
    for phase in [Phase::Transcribing, Phase::Inserting, Phase::Idle] {
        assert_eq!(next(&mut ui).await, UiEvent::StateChanged(phase));
    }
    pipeline.shutdown().await;
    transcriber.0.lock().unwrap().clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn criterion_1_the_pipeline_receives_the_files_samples() {
    let (wav, samples) = Temp::wav("one.wav");
    let argv = [
        "dictation-app".to_owned(),
        "--debug-audio-file".to_owned(),
        wav.0.display().to_string(),
    ];
    // The shell's choice: the file named on the command line.
    let path = debug_audio_file(&argv).unwrap();
    let calls = dictate(FileAudioSource::new(path)).await;

    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].sample_rate, 16_000);
    let expected: Vec<f32> = samples.iter().map(|&s| f32::from(s) / 32_768.0).collect();
    assert_eq!(calls[0].samples, expected);
}

#[tokio::test(flavor = "multi_thread")]
async fn criterion_3_debug_mode_saves_what_the_pipeline_received() {
    let (wav, samples) = Temp::wav("two.wav");
    let dir = Temp::new("recordings");
    let audio = SavingAudio::new(FileAudioSource::new(&wav.0), &dir.0);
    let calls = dictate(audio).await;
    assert_eq!(calls.len(), 1);

    let saved: Vec<_> = std::fs::read_dir(&dir.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(saved.len(), 1);
    let reader = hound::WavReader::open(&saved[0]).unwrap();
    assert_eq!(reader.spec().sample_rate, 16_000);
    let back: Vec<i16> = reader.into_samples().map(Result::unwrap).collect();
    // f32 -> i16 scales by 32767, the file was read with 32768: within one.
    assert_eq!(back.len(), samples.len());
    assert!(
        back.iter()
            .zip(&samples)
            .all(|(&a, &b)| (i32::from(a) - i32::from(b)).abs() <= 1)
    );
}
