//! #124: the app's own logging (the real subscriber, rotating file and
//! stderr, installed once by [`super::init_with`]) stays free of keys and
//! transcripts when the records come from the spawned pipeline, its jobs, the
//! engine rebuild threads and a panicking worker, not from a thread-local
//! subscriber around a direct call.
//!
//! A global subscriber and panic hook can be set once per process, so the
//! test re-runs its own binary as a child that does the dictating; the
//! parent reads every log file the child left and everything it wrote to
//! stderr. The parent first checks that the records it expects are there
//! (from each worker, in the files and on stderr, and that the file rotated),
//! so an empty or broken log can't pass as a private one; only then does it
//! look for the sentinels.
//!
//! No sleep stands in for "it happened": the child waits on the UI events,
//! joins the rebuild threads, and every wait has an upper bound.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dictation_core::coordinator::Phase;
use dictation_core::settings::{Provider, Settings};
use dictation_pipeline::config::Config;
use dictation_pipeline::history::{HISTORY_FILE, History, Recording};
use dictation_pipeline::pipeline::{Config as PipelineConfig, Pipeline, Ports, UiEvent};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_pipeline::usage::UsageTracker;
use dictation_platform::fakes::{
    FakeClock, FakeErrorReporter, FakeHotkey, FakeInserter, FakePermissions, FakeTelemetry,
    FileAudioSource, MemSecretStore,
};
use dictation_platform::history_file::HistoryFile;
use dictation_ports::{
    Audio, EngineKind, HotkeyEvent, InsertError, PermissionStatus, TranscribeError, Transcriber,
    Transcript,
};
use dictation_transcribe::engine::Transcribers;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{LEVEL_VARIABLE, LOG_FILE_PREFIX, Limits, init_with};
use crate::diagnostics::{Diagnostics, Environment, report_text};
use crate::engine::{LiveEngine, changed, starting};

/// Fake keys that look like no provider's, so only the registry of keys
/// the app has handled (`redact::register_secret`) can hide them.
const OLD_KEY: &str = "t3-old-fake-credential-QX7Z1R";
const NEW_KEY: &str = "t3-new-fake-credential-PW4K8M";
/// What the fake provider "heard".
const TRANSCRIPT: &str = "zebra quantum marmalade sentinel";

/// None of these may appear anywhere: the sentinels and distinctive parts.
const FORBIDDEN: [&str; 7] = [
    OLD_KEY,
    NEW_KEY,
    TRANSCRIPT,
    "QX7Z1R",
    "PW4K8M",
    "fake-credential",
    "marmalade",
];

/// Small files, so a few dictations rotate them several times; enough of
/// them that none is pruned and every byte written can be checked.
const LIMITS: Limits = Limits {
    file_bytes: 1024,
    keep: 1000,
};

/// Set in the child: where it logs.
const CHILD_DIR: &str = "DICTATION_T3_PRIVACY_CHILD_DIR";
const TEST_NAME: &str =
    "logging::spawned_privacy::spawned_pipeline_logs_stay_private_across_rotation";

/// Upper bound on any one wait in the child.
const WAIT: Duration = Duration::from_secs(20);
/// Upper bound on the whole child.
const CHILD_DEADLINE: Duration = Duration::from_secs(120);

/// A line the panicking worker's panic hook output must contain (where the
/// panic happened), whatever the hook prints.
const PANIC_SITE: &str = "spawned_privacy.rs";

#[test]
fn spawned_pipeline_logs_stay_private_across_rotation() {
    if let Some(dir) = std::env::var_os(CHILD_DIR) {
        child(Path::new(&dir));
        return;
    }
    let dir = TempDir::new();
    let (stdout, stderr) = run_child(&dir.0);

    // The child's own checks passed (it is a test too).
    assert!(
        stdout.contains("1 passed"),
        "child failed:\n{}\n{}",
        tail(&stdout),
        tail(&stderr)
    );

    let files = log_files(&dir.0.join("logs"));
    // Positive controls first: rotation happened, nothing was pruned (so the
    // files hold everything written), and each worker's records are there.
    assert!(
        files.len() >= 3,
        "the log didn't rotate: {} file(s)",
        files.len()
    );
    assert!(files.len() < LIMITS.keep, "files were pruned");
    let file_text: String = files
        .iter()
        .map(|file| std::fs::read_to_string(file).unwrap())
        .collect();
    for (what, text) in [("the files", &file_text), ("stderr", &stderr)] {
        for expected in [
            // The engine-startup thread and the Settings-save thread.
            "engine settings",
            "engine rebuilt",
            "has_key=true",
            // The pipeline task: state steps and the failure it showed.
            "step",
            "StateChanged(Inserting)",
            "dictation failed",
            "rejected your API key",
            // The transcription jobs and the engine's request.
            "transcribing",
            "transcribed",
            // The echo of both keys, which the registry hid.
            "provider echo: rejected [key hidden], accepted [key hidden]",
            // The panicking job, as the pipeline saw it.
            "a background job panicked",
            "child done",
        ] {
            assert!(
                text.contains(expected),
                "{what}: no {expected:?}:\n{}",
                tail(text)
            );
        }
        // Each successful dictation's transcript length, at debug.
        assert!(
            text.contains(&format!("len: {}", TRANSCRIPT.len())),
            "{what}: no transcript length:\n{}",
            tail(text)
        );
    }
    // The panic hook's own output, wherever the hook writes it.
    for expected in [PANIC_SITE, "panicked"] {
        assert!(
            stderr.contains(expected),
            "no panic report:\n{}",
            tail(&stderr)
        );
    }

    // Only now: nothing private anywhere.
    for (what, text) in [("the files", &file_text), ("stderr", &stderr)] {
        for secret in FORBIDDEN {
            assert!(
                !text.contains(secret),
                "{what} hold {secret:?}:\n{}",
                lines_with(text, secret)
            );
        }
    }
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for secret in FORBIDDEN {
            assert!(
                !text.contains(secret),
                "{} holds {secret:?}",
                file.display()
            );
        }
    }
}

/// Runs this test again as a child that logs into `dir`, and returns its
/// stdout and stderr. Fails the test if it doesn't finish in time.
fn run_child(dir: &Path) -> (String, String) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([TEST_NAME, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_DIR, dir)
        .env(LEVEL_VARIABLE, "debug")
        .env("RUST_BACKTRACE", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Each pipe is read to its end on a thread of its own; both end when the
    // child exits, which is the signal to collect it. The deadline only turns
    // a hang into a failure.
    let (done, ended) = std::sync::mpsc::channel();
    let drain = |mut pipe: Box<dyn std::io::Read + Send>| {
        let done = done.clone();
        std::thread::spawn(move || {
            let mut text = String::new();
            pipe.read_to_string(&mut text).unwrap();
            let _ = done.send(());
            text
        })
    };
    let stdout = drain(Box::new(child.stdout.take().unwrap()));
    let stderr = drain(Box::new(child.stderr.take().unwrap()));
    let deadline = Instant::now() + CHILD_DEADLINE;
    for _ in 0..2 {
        let left = deadline.saturating_duration_since(Instant::now());
        if ended.recv_timeout(left).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the child didn't finish in {CHILD_DEADLINE:?}");
        }
    }
    let status = child.wait().unwrap();
    let (stdout, stderr) = (stdout.join().unwrap(), stderr.join().unwrap());
    assert!(
        status.success(),
        "child: {status}\n{}\n{}",
        tail(&stdout),
        tail(&stderr)
    );
    (stdout, stderr)
}

/// The end of `text`, so a failure shows what happened last without
/// pages of log.
fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(40)..].join("\n")
}

/// The lines of `text` that hold `needle` (fake values only), at most ten.
fn lines_with(text: &str, needle: &str) -> String {
    text.lines()
        .filter(|line| line.contains(needle))
        .take(10)
        .collect::<Vec<_>>()
        .join("\n")
}

fn log_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(LOG_FILE_PREFIX))
        })
        .collect();
    files.sort();
    files
}

/// A directory under the temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("dictation-t3-privacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---- the child ----

/// The engine, plus a switch that makes the next transcription panic with
/// both keys and the transcript in its message: the panic hook must not
/// print them.
struct Panicky {
    inner: Transcribers,
    armed: Arc<AtomicBool>,
}

impl Panicky {
    fn maybe_panic(&self) {
        if self.armed.swap(false, Ordering::SeqCst) {
            panic!("worker blew up holding {OLD_KEY} / {NEW_KEY}: {TRANSCRIPT}");
        }
    }
}

impl Transcriber for Panicky {
    async fn transcribe(&self, audio: &Audio) -> Result<String, TranscribeError> {
        self.maybe_panic();
        self.inner.transcribe(audio).await
    }

    async fn transcribe_detailed(&self, audio: &Audio) -> Result<Transcript, TranscribeError> {
        self.maybe_panic();
        self.inner.transcribe_detailed(audio).await
    }
}

/// Installs the app's logging and dictates through every path, on a
/// multithreaded runtime.
fn child(dir: &Path) {
    init_with(&dir.join("logs"), LIMITS).unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(dictate_everything(dir));
    tracing::info!("child done");
}

async fn dictate_everything(dir: &Path) {
    let server = MockServer::start().await;
    // The new key works; any other (the old one) is rejected, and the
    // provider echoes it back, as some do.
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .and(header(
            "authorization",
            format!("Bearer {NEW_KEY}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": TRANSCRIPT,
            "segments": [{ "start": 0.0, "end": 0.5, "text": TRANSCRIPT, "no_speech_prob": 0.01 }],
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": { "message": format!("Invalid API Key {OLD_KEY}") },
        })))
        .with_priority(10)
        .mount(&server)
        .await;

    // Settings with the old key, saved before startup.
    let secrets = MemSecretStore::new();
    let config = Arc::new(Config::new(dir.join("config"), secrets));
    config.set_api_key(Provider::Groq, OLD_KEY).unwrap();
    let transcribers = Transcribers::new(starting());
    let live = LiveEngine::follow(transcribers.clone())
        .at(&server.uri())
        .with_key_timeout(WAIT);
    // Startup reads the key on a thread of its own.
    live.rebuild_in_background(Arc::clone(&config))
        .unwrap()
        .join()
        .unwrap();

    let wav = dir.join("speech.wav");
    let wav_bytes = dictation_core::wav::encode_f32(&vec![0.25; 8_000], 16_000).unwrap();
    std::fs::write(&wav, wav_bytes).unwrap();
    let armed = Arc::new(AtomicBool::new(false));
    let history = Arc::new(History::open(
        HistoryFile::new(dir.join(HISTORY_FILE)),
        Some(25),
    ));
    let mut run = Run::spawn(
        &wav,
        Arc::clone(&history),
        Panicky {
            inner: transcribers.clone(),
            armed: Arc::clone(&armed),
        },
    );

    // The provider rejects the old key.
    let events = run.dictate().await;
    assert!(
        matches!(events.get(2), Some(UiEvent::Error(_))),
        "{events:?}"
    );
    assert!(run.inserter.inserted().is_empty());
    run.seen.extend(events);

    // Settings saves the new key; the rebuild runs on a blocking thread,
    // as the command does.
    let (save_config, save_live) = (Arc::clone(&config), live.clone());
    tokio::task::spawn_blocking(move || {
        changed(&save_config, &save_live, |c| {
            c.set_api_key(Provider::Groq, NEW_KEY)
        })
        .unwrap();
    })
    .await
    .unwrap();
    // A library echoing both keys in free text, from a worker thread: no
    // type protects it, and neither key looks like one, so only the keys
    // the app registered as it read and saved them can hide them.
    tokio::spawn(async {
        tracing::info!("provider echo: rejected {OLD_KEY}, accepted {NEW_KEY}");
    })
    .await
    .unwrap();

    // Successes, enough to rotate the file a few times.
    for n in 1..=3 {
        let events = run.dictate().await;
        assert!(
            events.contains(&UiEvent::StateChanged(Phase::Inserting)),
            "{events:?}"
        );
        assert_eq!(run.inserter.inserted().len(), n);
        run.seen.extend(events);
    }
    // U7: a paste that fails, whose text is kept as not pasted, and one
    // Secure Input refuses, which isn't kept.
    run.inserter
        .fail_next(InsertError::Failed("no focus".to_owned()));
    let events = run.dictate().await;
    run.seen.extend(events);
    let kept = history.entries().len();
    assert!(history.entries().first().is_some_and(|e| !e.pasted));
    run.inserter.fail_next(InsertError::SecureInput);
    let events = run.dictate().await;
    run.seen.extend(events);
    assert_eq!(history.entries().len(), kept);
    history_stays_out_of_events_and_reports(dir, &history, &run.seen);
    assert!(
        run.inserter
            .inserted()
            .iter()
            .all(|text| text == TRANSCRIPT)
    );

    // A worker panics mid-dictation; the pipeline recovers.
    armed.store(true, Ordering::SeqCst);
    let events = run.dictate().await;
    assert!(
        matches!(events.get(2), Some(UiEvent::Error(_))),
        "{events:?}"
    );
    let events = run.dictate().await;
    assert!(
        events.contains(&UiEvent::StateChanged(Phase::Inserting)),
        "{events:?}"
    );

    tokio::time::timeout(WAIT, run.pipeline.shutdown())
        .await
        .unwrap();
}

/// U7 criterion 4: with the entries really in the file, what those same
/// dictations produce for analytics, for Sentry and for "Report a problem"
/// doesn't hold their text. The log files are searched by the parent.
fn history_stays_out_of_events_and_reports(dir: &Path, history: &History, seen: &[UiEvent]) {
    let stored = std::fs::read_to_string(dir.join(HISTORY_FILE)).unwrap();
    assert!(
        stored.contains(TRANSCRIPT),
        "the entries aren't in the file"
    );
    assert!(history.entries().iter().all(|e| e.text == TRANSCRIPT));

    let telemetry = Arc::new(FakeTelemetry::new());
    let reporter = Arc::new(FakeErrorReporter::new());
    let mut tracker =
        UsageTracker::new(telemetry.clone(), EngineKind::Cloud).with_reporter(reporter.clone());
    let diagnostics = Diagnostics::default();
    for (n, event) in (0_u64..).zip(seen) {
        tracker.on_event(event, Duration::from_millis(n * 100));
        diagnostics.record_event(event);
    }
    let events = telemetry.events();
    assert!(!events.is_empty(), "no usage events to check");
    assert!(!reporter.codes().is_empty(), "no error reports to check");
    let wire = serde_json::to_string(&events).unwrap();
    let codes = format!("{:?}", reporter.codes());
    let report = report_text(
        &Environment::read("1.0.0"),
        &Settings::default(),
        (PermissionStatus::Granted, PermissionStatus::Granted),
        Some(true),
        &diagnostics.recent(),
    );
    assert!(report.contains("1.0.0"), "{report}");
    for (what, text) in [("events", &wire), ("reports", &codes), ("report", &report)] {
        assert!(!text.contains(TRANSCRIPT), "{what}: {text}");
    }
}

/// A running pipeline with a fake hotkey and inserter.
struct Run {
    pipeline: Pipeline,
    ui: UiReceiver,
    hotkey: FakeHotkey,
    clock: FakeClock,
    inserter: FakeInserter,
    /// The UI events of the dictations so far.
    seen: Vec<UiEvent>,
}

impl Run {
    fn spawn<T: Transcriber + 'static>(wav: &Path, history: Arc<History>, transcriber: T) -> Self {
        let hotkey = FakeHotkey::new();
        let clock = FakeClock::at(Duration::from_secs(100));
        let inserter = FakeInserter::new();
        let ports = Ports {
            hotkey: hotkey.clone(),
            audio: FileAudioSource::new(wav),
            transcriber,
            inserter: Recording::new(inserter.clone(), history, || "Groq".to_owned()),
            permissions: FakePermissions::all_granted(),
            clock: clock.clone(),
        };
        let (pipeline, ui) = Pipeline::spawn(ports, PipelineConfig::default()).unwrap();
        Self {
            pipeline,
            ui,
            hotkey,
            clock,
            inserter,
            seen: Vec::new(),
        }
    }

    /// One press and release, then every UI event up to Idle.
    async fn dictate(&mut self) -> Vec<UiEvent> {
        assert!(self.hotkey.emit(HotkeyEvent::Pressed));
        self.clock.advance(Duration::from_millis(500));
        assert!(self.hotkey.emit(HotkeyEvent::Released));
        let mut events = Vec::new();
        loop {
            let event = tokio::time::timeout(WAIT, self.ui.recv())
                .await
                .expect("no UI event in time")
                .expect("the pipeline stopped");
            let idle = event == UiEvent::StateChanged(Phase::Idle);
            events.push(event);
            if idle {
                return events;
            }
        }
    }
}
