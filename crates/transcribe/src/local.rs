//! On-device speech-to-text (L1): a GGUF model run by transcribe.cpp.
//!
//! [`LocalTranscriber`] is the [`Transcriber`] the pipeline sees. It holds one
//! loaded [`SpeechModel`] and runs it on tokio's blocking pool, so the CPU or
//! GPU work never stalls the event loop; calls that overlap queue on the
//! model's lock. The model stays loaded (warm) for as long as the
//! transcriber lives; dropping the last handle frees it.
//!
//! [`SpeechModel`] is the seam for tests: [`CppModel`] is the real one
//! (transcribe.cpp through the `transcribe-cpp` crate, as Handy uses it),
//! and a test can hand in a fake that returns fixed text.
//!
//! Privacy: audio is passed in memory and never written anywhere; neither
//! the audio nor the text is logged.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use dictation_core::redact;
use dictation_ports::{Audio, TranscribeError, Transcriber};

/// The sample rate the models take.
pub const SAMPLE_RATE: u32 = 16_000;

/// A loaded speech-to-text model.
///
/// Threading: `run` takes `&mut self` and may take seconds; callers run it on
/// a blocking thread, one call at a time.
pub trait SpeechModel: Send {
    /// Transcribes `pcm` (16 kHz mono, `-1.0..=1.0`). `language` is an ISO
    /// code the model supports, or `None` for its default.
    ///
    /// # Errors
    ///
    /// A short description of what failed, for the log. It never holds the
    /// audio or the text.
    fn run(&mut self, pcm: &[f32], language: Option<&str>) -> Result<String, String>;
}

/// transcribe.cpp running a GGUF file.
pub struct CppModel {
    session: transcribe_cpp::Session,
}

impl CppModel {
    /// Loads the model at `path` (Metal on Macs, else the CPU). Slow: seconds,
    /// and the file's size in memory. Call it from a blocking thread.
    ///
    /// # Errors
    ///
    /// [`TranscribeError::Engine`] if the file can't be read or isn't a model
    /// transcribe.cpp knows.
    pub fn load(path: &Path) -> Result<Self, TranscribeError> {
        let session = transcribe_cpp::Model::load(path)
            .and_then(|model| model.session())
            .map_err(|error| {
                TranscribeError::Engine(format!("couldn't load the on-device model: {error}"))
            })?;
        Ok(Self { session })
    }
}

impl SpeechModel for CppModel {
    fn run(&mut self, pcm: &[f32], language: Option<&str>) -> Result<String, String> {
        let options = transcribe_cpp::RunOptions {
            language: language.map(str::to_owned),
            ..transcribe_cpp::RunOptions::default()
        };
        self.session
            .run(pcm, &options)
            .map(|transcript| transcript.text)
            .map_err(|error| error.to_string())
    }
}

impl std::fmt::Debug for CppModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CppModel")
    }
}

/// A [`Transcriber`] over a loaded [`SpeechModel`]. Clones share the model.
#[derive(Clone)]
pub struct LocalTranscriber {
    model: Arc<Mutex<Box<dyn SpeechModel>>>,
    language: Option<String>,
}

impl LocalTranscriber {
    /// Wraps a loaded model. `language` is passed to every run; give only a
    /// language the model supports (or `None`).
    #[must_use]
    pub fn new(model: Box<dyn SpeechModel>, language: Option<String>) -> Self {
        Self {
            model: Arc::new(Mutex::new(model)),
            language,
        }
    }

    /// The same loaded model with another language hint, so a settings save
    /// that changes only the language doesn't load the model again.
    #[must_use]
    pub fn with_language(&self, language: Option<String>) -> Self {
        Self {
            model: Arc::clone(&self.model),
            language,
        }
    }

    /// The language hint passed to the model.
    #[must_use]
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Whether `self` and `other` run the same loaded model.
    #[must_use]
    pub fn shares_model(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.model, &other.model)
    }
}

impl std::fmt::Debug for LocalTranscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalTranscriber")
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}

impl Transcriber for LocalTranscriber {
    async fn transcribe(&self, audio: &Audio) -> Result<String, TranscribeError> {
        if audio.sample_rate != SAMPLE_RATE {
            return Err(TranscribeError::Engine(format!(
                "the on-device model needs {SAMPLE_RATE} Hz audio, got {} Hz",
                audio.sample_rate
            )));
        }
        if audio.samples.is_empty() {
            return Ok(String::new());
        }
        let model = Arc::clone(&self.model);
        let language = self.language.clone();
        // The samples are copied once so the blocking task owns them; a
        // dictation is seconds of audio, so this is small next to the run.
        let samples = audio.samples.clone();
        let run = tokio::task::spawn_blocking(move || {
            let mut model = model.lock().unwrap_or_else(PoisonError::into_inner);
            model.run(&samples, language.as_deref())
        });
        // Dropping this future doesn't stop a run already started: the native
        // call can't be interrupted here. It finishes and its text is dropped.
        match run.await {
            Ok(Ok(text)) => Ok(text.trim().to_owned()),
            Ok(Err(error)) => Err(TranscribeError::Engine(format!(
                "the on-device model failed: {}",
                redact::summary(&error)
            ))),
            Err(_) => Err(TranscribeError::Engine(
                "the on-device model crashed".to_owned(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// Each run: how many samples, and the language hint.
    type Seen = Arc<Mutex<Vec<(usize, Option<String>)>>>;

    /// Returns `reply`, and records what it was given.
    struct Fake {
        reply: Result<String, String>,
        calls: Arc<AtomicUsize>,
        seen: Seen,
        thread: Arc<Mutex<Option<std::thread::ThreadId>>>,
    }

    impl SpeechModel for Fake {
        fn run(&mut self, pcm: &[f32], language: Option<&str>) -> Result<String, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen
                .lock()
                .unwrap()
                .push((pcm.len(), language.map(str::to_owned)));
            *self.thread.lock().unwrap() = Some(std::thread::current().id());
            self.reply.clone()
        }
    }

    struct Probe {
        calls: Arc<AtomicUsize>,
        seen: Seen,
        thread: Arc<Mutex<Option<std::thread::ThreadId>>>,
    }

    fn fake(reply: Result<&str, &str>) -> (Box<dyn SpeechModel>, Probe) {
        let probe = Probe {
            calls: Arc::default(),
            seen: Arc::default(),
            thread: Arc::default(),
        };
        let model = Fake {
            reply: reply.map(str::to_owned).map_err(str::to_owned),
            calls: Arc::clone(&probe.calls),
            seen: Arc::clone(&probe.seen),
            thread: Arc::clone(&probe.thread),
        };
        (Box::new(model), probe)
    }

    fn audio(rate: u32, len: usize) -> Audio {
        Audio {
            sample_rate: rate,
            samples: vec![0.1; len],
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn criterion_1_runs_the_model_on_16_khz_audio_and_returns_trimmed_text() {
        let (model, probe) = fake(Ok("  Hello from this Mac. \n"));
        let engine = LocalTranscriber::new(model, Some("en".into()));
        let text = engine.transcribe(&audio(16_000, 32_000)).await.unwrap();
        assert_eq!(text, "Hello from this Mac.");
        assert_eq!(*probe.seen.lock().unwrap(), [(32_000, Some("en".into()))]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn criterion_6_the_run_is_off_the_calling_thread() {
        let (model, probe) = fake(Ok("x"));
        let engine = LocalTranscriber::new(model, None);
        engine.transcribe(&audio(16_000, 10)).await.unwrap();
        let ran_on = probe.thread.lock().unwrap().unwrap();
        assert_ne!(ran_on, std::thread::current().id());
    }

    #[tokio::test]
    async fn other_sample_rates_and_empty_audio_never_reach_the_model() {
        let (model, probe) = fake(Ok("x"));
        let engine = LocalTranscriber::new(model, None);
        let error = engine.transcribe(&audio(44_100, 10)).await.unwrap_err();
        assert!(error.to_string().contains("16000 Hz"), "{error}");
        assert_eq!(engine.transcribe(&audio(16_000, 0)).await.unwrap(), "");
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_model_error_is_an_engine_error_cut_to_one_line() {
        let long = format!("decode failed\n{}", "x".repeat(2_000));
        let (model, _probe) = fake(Err(&long));
        let engine = LocalTranscriber::new(model, None);
        let error = engine.transcribe(&audio(16_000, 10)).await.unwrap_err();
        let TranscribeError::Engine(text) = &error else {
            panic!("{error:?}");
        };
        assert!(text.starts_with("the on-device model failed: decode failed"));
        assert!(!error.to_string().contains('\n') && error.to_string().len() < 300);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn criterion_6_a_language_change_keeps_the_loaded_model() {
        let (model, probe) = fake(Ok("x"));
        let english = LocalTranscriber::new(model, Some("en".into()));
        let detect = english.with_language(None);
        assert!(detect.shares_model(&english));
        assert_eq!(detect.language(), None);
        detect.transcribe(&audio(16_000, 5)).await.unwrap();
        english.transcribe(&audio(16_000, 6)).await.unwrap();
        assert_eq!(
            *probe.seen.lock().unwrap(),
            [(5, None), (6, Some("en".into()))]
        );
        let (other, _) = fake(Ok("y"));
        assert!(!LocalTranscriber::new(other, None).shares_model(&english));
        assert!(format!("{english:?}").contains("LocalTranscriber"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn overlapping_calls_queue_on_the_one_model() {
        let (model, probe) = fake(Ok("x"));
        let engine = LocalTranscriber::new(model, None);
        let clip = audio(16_000, 5);
        let (a, b) = tokio::join!(engine.transcribe(&clip), engine.transcribe(&clip));
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(probe.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_file_that_is_not_a_model_fails_to_load_with_an_engine_error() {
        let dir = std::env::temp_dir().join(format!("l1-not-a-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("broken.gguf");
        std::fs::write(&path, b"not a gguf file").unwrap();
        let error = CppModel::load(&path).unwrap_err();
        assert!(matches!(error, TranscribeError::Engine(_)), "{error:?}");
        assert!(
            error
                .to_string()
                .contains("couldn't load the on-device model")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
