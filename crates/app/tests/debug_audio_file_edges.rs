//! M2.2 Tester (run 1): edge cases around `--debug-audio-file` and the
//! audio worker (issue #40) that the Developer's tests don't cover: empty,
//! very short, missing and non-16 kHz files, back-to-back dictations, and a
//! blocked `start` followed by Cancel and a fresh dictation.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use std::time::Duration;

use dictation_core::coordinator::{MIN_HOLD, Phase};
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{DiscardingInserter, GrantedPermissions};
use dictation_pipeline::test_hooks::{SyntheticHotkey, debug_audio_file, hook_events};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{FakeClock, FakeHotkey, FileAudioSource};
use dictation_ports::{
    Audio, AudioError, AudioSource, HotkeyError, HotkeyEvent, TranscribeError, Transcriber,
};

const WAIT: Duration = Duration::from_secs(5);

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

struct TempWav(PathBuf);

impl TempWav {
    fn path(name: &str) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "dictation-tester-m22-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn new(name: &str, sample_rate: u32, samples: &[i16]) -> Self {
        let path = Self::path(name);
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for &sample in samples {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        Self(path)
    }
}

impl Drop for TempWav {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct Run<A> {
    pipeline: Pipeline,
    ui: UiReceiver,
    injector: dictation_pipeline::test_hooks::Injector,
    clock: FakeClock,
    transcriber: RecordingTranscriber,
    _audio: std::marker::PhantomData<A>,
}

async fn next(ui: &mut UiReceiver) -> UiEvent {
    tokio::time::timeout(WAIT, ui.recv())
        .await
        .unwrap()
        .unwrap()
}

fn spawn<A: AudioSource + 'static>(audio: A) -> Run<A> {
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
    let (pipeline, ui) = Pipeline::spawn(ports, Config::default()).unwrap();
    Run {
        pipeline,
        ui,
        injector,
        clock,
        transcriber,
        _audio: std::marker::PhantomData,
    }
}

impl<A> Run<A> {
    fn press(&self) {
        for event in hook_events(["dictation-app", "--press"]) {
            assert!(self.injector.inject(event));
        }
    }

    fn release(&self) {
        self.clock.advance(MIN_HOLD * 2);
        for event in hook_events(["dictation-app", "--release"]) {
            assert!(self.injector.inject(event));
        }
    }

    /// Events until the pipeline is back at Idle (Recording excluded).
    async fn until_idle(&mut self) -> Vec<UiEvent> {
        let mut seen = Vec::new();
        loop {
            let event = next(&mut self.ui).await;
            let idle = event == UiEvent::StateChanged(Phase::Idle);
            seen.push(event);
            if idle {
                return seen;
            }
        }
    }

    async fn dictate_once(&mut self) -> Vec<UiEvent> {
        self.press();
        assert_eq!(
            next(&mut self.ui).await,
            UiEvent::StateChanged(Phase::Recording)
        );
        self.release();
        self.until_idle().await
    }

    fn transcribed(&self) -> Vec<Audio> {
        self.transcriber.0.lock().unwrap().clone()
    }
}

fn source_for(wav: &TempWav) -> FileAudioSource {
    let argv = [
        "dictation-app".to_owned(),
        "--debug-audio-file".to_owned(),
        wav.0.display().to_string(),
    ];
    FileAudioSource::new(debug_audio_file(&argv).unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_file_does_not_crash_and_returns_to_idle() {
    let wav = TempWav::new("empty.wav", 16_000, &[]);
    let mut run = spawn(source_for(&wav));
    let events = run.dictate_once().await;
    assert_eq!(events.last(), Some(&UiEvent::StateChanged(Phase::Idle)));
    // Empty audio never reaches the transcriber (#69).
    assert!(run.transcribed().is_empty());
    // The pipeline is still alive: a second dictation also completes.
    let events = run.dictate_once().await;
    assert_eq!(events.last(), Some(&UiEvent::StateChanged(Phase::Idle)));
    run.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_very_short_file_is_not_transcribed() {
    let samples = [1_000i16, -1_000, 2_000, -2_000, 0];
    let wav = TempWav::new("short.wav", 16_000, &samples);
    let mut run = spawn(source_for(&wav));
    let events = run.dictate_once().await;
    assert_eq!(events.last(), Some(&UiEvent::StateChanged(Phase::Idle)));
    let calls = run.transcribed();
    // Under 200 ms: dropped before the transcriber (#69).
    assert!(calls.is_empty());
    run.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_file_is_an_error_not_a_transcription() {
    let path = TempWav::path("missing.wav");
    let argv = [
        "dictation-app".to_owned(),
        format!("--debug-audio-file={}", path.display()),
    ];
    let mut run = spawn(FileAudioSource::new(debug_audio_file(&argv).unwrap()));
    let events = run.dictate_once().await;
    assert!(
        events
            .iter()
            .any(|event| matches!(event, UiEvent::Error(_))),
        "no error shown: {events:?}"
    );
    assert_eq!(events.last(), Some(&UiEvent::StateChanged(Phase::Idle)));
    assert!(run.transcribed().is_empty());
    run.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_48_khz_file_reaches_the_pipeline_at_its_own_rate() {
    // FileAudioSource doesn't resample: the pipeline gets the file's rate
    // and every sample, so a transcriber can't mistake its duration.
    let samples: Vec<i16> = (0..14_400).map(|n| (n % 1_000) as i16).collect();
    let wav = TempWav::new("48k.wav", 48_000, &samples);
    let mut run = spawn(source_for(&wav));
    run.dictate_once().await;
    let calls = run.transcribed();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].sample_rate, 48_000);
    assert_eq!(calls[0].samples.len(), samples.len());
    assert_eq!(calls[0].duration(), Duration::from_millis(300));
    run.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn back_to_back_dictations_each_get_the_whole_file() {
    let samples: Vec<i16> = (0..3_200).map(|n| (n % 3_000) as i16 - 1_500).collect();
    let wav = TempWav::new("b2b.wav", 16_000, &samples);
    let mut run = spawn(source_for(&wav));
    for _ in 0..3 {
        let events = run.dictate_once().await;
        assert_eq!(
            events,
            [
                UiEvent::StateChanged(Phase::Transcribing),
                UiEvent::StateChanged(Phase::Inserting),
                UiEvent::StateChanged(Phase::Idle),
            ]
        );
    }
    let calls = run.transcribed();
    assert_eq!(calls.len(), 3);
    assert!(
        calls
            .iter()
            .all(|audio| audio.samples.len() == samples.len())
    );
    run.pipeline.shutdown().await;
}

/// A source whose first `start` blocks until released (a Bluetooth mic
/// opening), then behaves like a 1 s 16 kHz recording.
#[derive(Clone)]
struct SlowFirstStart {
    gate: Arc<Mutex<Option<std_mpsc::Receiver<()>>>>,
    recording: Arc<Mutex<bool>>,
    starts: Arc<AtomicUsize>,
}

impl AudioSource for SlowFirstStart {
    fn start(&mut self) -> Result<(), AudioError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        let gate = self.gate.lock().unwrap().take();
        if let Some(gate) = gate {
            let _ = gate.recv();
        }
        *self.recording.lock().unwrap() = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        let mut recording = self.recording.lock().unwrap();
        if !*recording {
            return Err(AudioError::NotRecording);
        }
        *recording = false;
        Ok(Audio {
            sample_rate: 16_000,
            samples: vec![0.1; 16_000],
        })
    }

    fn discard(&mut self) {
        *self.recording.lock().unwrap() = false;
    }

    fn is_recording(&self) -> bool {
        *self.recording.lock().unwrap()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn issue_40_cancel_during_a_blocked_start_then_a_fresh_dictation_works() {
    let (release, gate) = std_mpsc::channel();
    let audio = SlowFirstStart {
        gate: Arc::new(Mutex::new(Some(gate))),
        recording: Arc::default(),
        starts: Arc::default(),
    };
    let starts = Arc::clone(&audio.starts);
    let mut run = spawn(audio);

    run.press();
    assert_eq!(
        next(&mut run.ui).await,
        UiEvent::StateChanged(Phase::Recording)
    );
    // Wait until the device open has begun: since M2.3 (#81 B1) a cancel
    // that reaches the audio worker first drops the Start instead.
    let deadline = std::time::Instant::now() + WAIT;
    while starts.load(Ordering::SeqCst) == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the start never began"
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    // Cancel while start is still blocked: handled at once.
    assert!(run.injector.inject(HotkeyEvent::CancelPressed));
    assert_eq!(next(&mut run.ui).await, UiEvent::StateChanged(Phase::Idle));
    assert!(run.transcribed().is_empty());

    // The device finishes opening; the cancelled recording is discarded and
    // a new dictation records and transcribes normally.
    release.send(()).unwrap();
    let events = run.dictate_once().await;
    assert_eq!(
        events,
        [
            UiEvent::StateChanged(Phase::Transcribing),
            UiEvent::StateChanged(Phase::Inserting),
            UiEvent::StateChanged(Phase::Idle),
        ]
    );
    let calls = run.transcribed();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].samples.len(), 16_000);
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    run.pipeline.shutdown().await;
}

#[test]
fn the_debug_audio_file_flag_without_a_value_is_ignored() {
    assert_eq!(
        debug_audio_file(["dictation-app", "--debug-audio-file"]),
        None
    );
    assert_eq!(
        debug_audio_file(["dictation-app", "--debug-audio-file="]),
        None
    );
    assert_eq!(debug_audio_file(["dictation-app", "--press"]), None);
}
