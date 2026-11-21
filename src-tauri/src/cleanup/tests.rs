//! The keeper against a fake helper: warm while wanted, released on switch,
//! restarted when it dies; and the app's cleaner in a real pipeline.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use dictation_core::coordinator::Phase::Idle;
use dictation_core::local_cleanup;
use dictation_core::settings::Settings;
use dictation_pipeline::pipeline::{CleanupStep, Config, Extras, Pipeline, Ports, UiEvent};
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, FileAudioSource,
};
use dictation_ports::{
    Audio, CleanupOutcome, HotkeyEvent, NoCleanup, TranscribeError, Transcriber,
};

use super::*;

/// What the fake Gemma answers: [`RAW`] cleaned, which the guard accepts.
const CLEANED: &str = "So, send the report to Priya by 5.";

/// What every fake helper does, shared with the test.
#[derive(Default)]
struct Behaviour {
    /// The helper has died (its process exited).
    dead: AtomicBool,
    /// Requests never answer.
    hang: AtomicBool,
    /// Hung requests whose future was dropped: cancelled, which closes the
    /// helper's connection and stops its generation.
    dropped: AtomicUsize,
}

/// Counts a dropped request future.
struct OnDrop(Arc<Behaviour>);

impl Drop for OnDrop {
    fn drop(&mut self) {
        self.0.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

/// A started helper that counts how many are alive.
struct FakeServer {
    alive: Arc<AtomicUsize>,
    behaviour: Arc<Behaviour>,
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.alive.fetch_sub(1, Ordering::SeqCst);
    }
}

impl ChatRunner for FakeServer {
    async fn complete(&self, _: &str, _: &str, _: u32) -> Result<String, LlamaError> {
        if self.behaviour.hang.load(Ordering::SeqCst) {
            let _cancelled = OnDrop(Arc::clone(&self.behaviour));
            std::future::pending::<()>().await;
        }
        Ok(CLEANED.to_owned())
    }

    fn alive(&self) -> bool {
        !self.behaviour.dead.load(Ordering::SeqCst)
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("q4-cleanup-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn download_gemma(&self) {
        let model = &local_models::GEMMA_4_E2B;
        std::fs::File::create(self.0.join(model.file_name))
            .unwrap()
            .set_len(model.size_bytes)
            .unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Rig {
    keeper: LocalCleanup<FakeServer>,
    starts: Arc<AtomicUsize>,
    alive: Arc<AtomicUsize>,
    behaviour: Arc<Behaviour>,
}

fn rig(dir: &TempDir, delay: Duration, fail: bool) -> Rig {
    let starts = Arc::new(AtomicUsize::new(0));
    let alive = Arc::new(AtomicUsize::new(0));
    let behaviour = Arc::new(Behaviour::default());
    let (s, a, b) = (
        Arc::clone(&starts),
        Arc::clone(&alive),
        Arc::clone(&behaviour),
    );
    let starter: Starter<FakeServer> = Arc::new(move |path: PathBuf, _cancel| {
        let (s, a, b) = (Arc::clone(&s), Arc::clone(&a), Arc::clone(&b));
        Box::pin(async move {
            assert!(path.ends_with(local_models::GEMMA_4_E2B.file_name));
            s.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(delay).await;
            if fail {
                return Err(LlamaError::StartTimeout);
            }
            a.fetch_add(1, Ordering::SeqCst);
            // A new process: alive again.
            b.dead.store(false, Ordering::SeqCst);
            Ok(FakeServer {
                alive: a,
                behaviour: b,
            })
        })
    });
    let spawn: Spawner = Arc::new(|task| {
        tokio::spawn(task);
    });
    let store = ModelStore::new(dir.0.clone(), Arc::new(|_| Ok(u64::MAX))).unwrap();
    Rig {
        keeper: LocalCleanup::trusting(store, starter, spawn),
        starts,
        alive,
        behaviour,
    }
}

fn settings(transcription: Transcription) -> Settings {
    Settings {
        transcription,
        ..Settings::default()
    }
}

async fn until(what: impl Fn() -> bool) {
    for _ in 0..200 {
        if what() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out");
}

#[tokio::test(flavor = "multi_thread")]
async fn warm_while_local_and_downloaded_and_released_on_switch() {
    let dir = TempDir::new("warm");
    dir.download_gemma();
    let rig = rig(&dir, Duration::ZERO, false);
    assert_eq!(rig.keeper.state(), CleanupState::Off);
    assert!(rig.keeper.cleaner().is_none());

    rig.keeper.follow(&settings(Transcription::Local));
    until(|| rig.keeper.state() == CleanupState::Ready).await;
    let cleaner = rig.keeper.cleaner().unwrap();
    assert_eq!(cleaner.clean("um hi").await.unwrap(), CLEANED);
    drop(cleaner);

    // Saving an unrelated setting keeps it warm: no second start.
    rig.keeper.follow(&Settings {
        hotkey: "RightCmd".into(),
        ..settings(Transcription::Local)
    });
    assert_eq!(rig.starts.load(Ordering::SeqCst), 1);
    assert_eq!(rig.alive.load(Ordering::SeqCst), 1);

    // Switching to the cloud stops it.
    rig.keeper.follow(&settings(Transcription::Cloud));
    assert_eq!(rig.keeper.state(), CleanupState::Off);
    assert!(rig.keeper.cleaner().is_none());
    assert_eq!(rig.alive.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn not_started_without_the_model_or_with_cloud_speech() {
    let dir = TempDir::new("missing");
    let rig = rig(&dir, Duration::ZERO, false);
    rig.keeper.follow(&settings(Transcription::Local));
    rig.keeper.follow(&settings(Transcription::Cloud));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(rig.starts.load(Ordering::SeqCst), 0);
    assert_eq!(rig.keeper.state(), CleanupState::Off);

    // Once downloaded, the next rebuild starts it.
    dir.download_gemma();
    rig.keeper.follow(&settings(Transcription::Local));
    until(|| rig.keeper.state() == CleanupState::Ready).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_that_finishes_after_a_switch_is_thrown_away() {
    let dir = TempDir::new("stale");
    dir.download_gemma();
    let rig = rig(&dir, Duration::from_millis(200), false);
    rig.keeper.follow(&settings(Transcription::Local));
    assert_eq!(rig.keeper.state(), CleanupState::Starting);
    // The helper is loading when the user switches to the cloud.
    until(|| rig.starts.load(Ordering::SeqCst) == 1).await;
    rig.keeper.follow(&settings(Transcription::Cloud));
    tokio::time::sleep(Duration::from_millis(400)).await;
    // It started, saw the settings had moved on and stopped again.
    assert_eq!(rig.alive.load(Ordering::SeqCst), 0);
    assert_eq!(rig.keeper.state(), CleanupState::Off);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_start_leaves_no_cleaner_and_release_stops_it() {
    let dir = TempDir::new("fails");
    dir.download_gemma();
    let failing = rig(&dir, Duration::ZERO, true);
    failing.keeper.follow(&settings(Transcription::Local));
    until(|| failing.starts.load(Ordering::SeqCst) == 1).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(failing.keeper.cleaner().is_none());
    assert_eq!(failing.keeper.state(), CleanupState::Off);
    // The next rebuild tries again.
    failing.keeper.follow(&settings(Transcription::Local));
    until(|| failing.starts.load(Ordering::SeqCst) == 2).await;

    let rig = rig(&dir, Duration::ZERO, false);
    rig.keeper.follow(&settings(Transcription::Local));
    until(|| rig.keeper.state() == CleanupState::Ready).await;
    rig.keeper.release();
    assert_eq!(rig.alive.load(Ordering::SeqCst), 0);
    assert_eq!(rig.keeper.state(), CleanupState::Off);
}

#[test]
fn it_runs_gemma() {
    let dir = TempDir::new("model");
    let rig = rig(&dir, Duration::ZERO, false);
    assert_eq!(rig.keeper.model().id, local_models::DEFAULT_CLEANUP_MODEL);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_helper_that_dies_after_starting_is_started_again() {
    let dir = TempDir::new("dies");
    dir.download_gemma();
    let rig = rig(&dir, Duration::ZERO, false);
    rig.keeper.follow(&settings(Transcription::Local));
    until(|| rig.keeper.state() == CleanupState::Ready).await;

    // The helper crashes (or is killed): the next dictation finds it dead,
    // gets no cleaner (the raw text is pasted) and it is started again.
    rig.behaviour.dead.store(true, Ordering::SeqCst);
    assert!(rig.keeper.cleaner().is_none());
    assert_eq!(
        rig.alive.load(Ordering::SeqCst),
        0,
        "the dead one is dropped"
    );
    until(|| rig.keeper.state() == CleanupState::Ready).await;
    assert_eq!(rig.starts.load(Ordering::SeqCst), 2);
    let cleaner = rig.keeper.cleaner().unwrap();
    assert_eq!(cleaner.clean("um hi").await.unwrap(), CLEANED);
}

/// Always transcribes to [`RAW`], as OrukQwen would: no case, no punctuation.
struct Says;

const RAW: &str = "um so send the report to priya by 5";

impl Transcriber for Says {
    async fn transcribe(&self, _audio: &Audio) -> Result<String, TranscribeError> {
        Ok(RAW.to_owned())
    }
}

/// One dictation through a real pipeline whose cleanup is the app's
/// cleaner over `keeper`; the outcome and what was pasted.
async fn dictate_with(keeper: &LocalCleanup<FakeServer>) -> (Vec<CleanupOutcome>, Vec<String>) {
    let wav = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/e2e/fixtures/tone-16k.wav");
    let (hotkey, clock, inserter) = (
        FakeHotkey::new(),
        FakeClock::at(Duration::from_secs(100)),
        FakeInserter::new(),
    );
    let ports = Ports {
        hotkey: hotkey.clone(),
        audio: FileAudioSource::new(&wav),
        transcriber: Says,
        inserter: inserter.clone(),
        permissions: FakePermissions::all_granted(),
        clock: clock.clone(),
    };
    let extras = Extras {
        cursor: None,
        cleanup: Some(CleanupStep::new(AppCleaner::new(
            NoCleanup,
            Some(keeper.clone()),
        ))),
    };
    let (pipeline, mut ui) = Pipeline::spawn_with(ports, extras, Config::default()).unwrap();
    assert!(hotkey.emit(HotkeyEvent::Pressed));
    clock.advance(Duration::from_millis(500));
    assert!(hotkey.emit(HotkeyEvent::Released));
    let mut outcomes = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(20), ui.recv())
            .await
            .expect("no UI event in time")
            .expect("pipeline stopped");
        match event {
            UiEvent::Cleanup(outcome) => outcomes.push(outcome),
            UiEvent::StateChanged(Idle) => break,
            _ => {}
        }
    }
    pipeline.shutdown().await;
    (outcomes, inserter.inserted())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_local_dictation_pastes_the_cleaned_text_and_the_raw_text_on_a_helper_timeout() {
    let dir = TempDir::new("pipeline");
    dir.download_gemma();
    let rig = rig(&dir, Duration::ZERO, false);
    rig.keeper.follow(&settings(Transcription::Local));
    until(|| rig.keeper.state() == CleanupState::Ready).await;

    // Gemma answers: its reply is judged against prompt v3 and pasted.
    let (outcomes, pasted) = dictate_with(&rig.keeper).await;
    assert_eq!(outcomes, [CleanupOutcome::Applied]);
    assert_eq!(pasted, [CLEANED]);

    // Gemma hangs: at the on-device deadline the raw transcript is pasted.
    rig.behaviour.hang.store(true, Ordering::SeqCst);
    let started = std::time::Instant::now();
    let (outcomes, pasted) = dictate_with(&rig.keeper).await;
    assert_eq!(outcomes, [CleanupOutcome::FallbackTimeout]);
    assert_eq!(pasted, [RAW]);
    let waited = started.elapsed();
    assert!(
        waited >= local_cleanup::TIMEOUT && waited < local_cleanup::TIMEOUT * 3,
        "{waited:?}"
    );
    // Criterion 2: the timed-out inference was cancelled (its request
    // dropped), and the next dictation is cleaned at once, not queued
    // behind it.
    assert_eq!(rig.behaviour.dropped.load(Ordering::SeqCst), 1);
    rig.behaviour.hang.store(false, Ordering::SeqCst);
    let started = std::time::Instant::now();
    let (outcomes, pasted) = dictate_with(&rig.keeper).await;
    assert_eq!(outcomes, [CleanupOutcome::Applied]);
    assert_eq!(pasted, [CLEANED]);
    assert!(
        started.elapsed() < local_cleanup::TIMEOUT,
        "{:?}",
        started.elapsed()
    );

    // Cloud speech selected: the helper is stopped and cleanup is the
    // cloud's (off here), so the raw text is pasted with nothing counted.
    rig.keeper.follow(&settings(Transcription::Cloud));
    let (outcomes, pasted) = dictate_with(&rig.keeper).await;
    assert!(outcomes.is_empty());
    assert_eq!(pasted, [RAW]);
}

#[tokio::test(flavor = "multi_thread")]
async fn review_1_a_cleanup_model_that_lands_later_starts_the_helper_without_a_settings_change() {
    let dir = TempDir::new("lands");
    let first = rig(&dir, Duration::ZERO, false);
    // On-device chosen; the speech model's rebuild runs while Gemma is
    // still downloading.
    first.keeper.follow(&settings(Transcription::Local));
    assert_eq!(first.keeper.state(), CleanupState::Off);
    // The chained download finishes: no settings save, no restart.
    dir.download_gemma();
    first.keeper.refresh();
    until(|| first.keeper.state() == CleanupState::Ready).await;
    assert!(first.keeper.cleaner().is_some());

    // With cloud speech, a download that lands starts nothing.
    let dir = TempDir::new("lands-cloud");
    let cloud = rig(&dir, Duration::ZERO, false);
    cloud.keeper.follow(&settings(Transcription::Cloud));
    dir.download_gemma();
    cloud.keeper.refresh();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(cloud.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn review_2_a_superseded_start_is_cancelled_and_exits_before_the_next_loads() {
    let dir = TempDir::new("one-helper");
    dir.download_gemma();
    // Each start loads until cancelled (a model that is never ready), then
    // takes a moment to exit, as the real helper does.
    let loading = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let cancelled = Arc::new(AtomicUsize::new(0));
    let (l, m, c) = (
        Arc::clone(&loading),
        Arc::clone(&most),
        Arc::clone(&cancelled),
    );
    let starter: Starter<FakeServer> = Arc::new(move |_path: PathBuf, cancel| {
        let (l, m, c) = (Arc::clone(&l), Arc::clone(&m), Arc::clone(&c));
        Box::pin(async move {
            let now = l.fetch_add(1, Ordering::SeqCst) + 1;
            m.fetch_max(now, Ordering::SeqCst);
            cancel.await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            l.fetch_sub(1, Ordering::SeqCst);
            c.fetch_add(1, Ordering::SeqCst);
            Err(LlamaError::Cancelled)
        })
    });
    let spawn: Spawner = Arc::new(|task| {
        tokio::spawn(task);
    });
    let store = ModelStore::new(dir.0.clone(), Arc::new(|_| Ok(u64::MAX))).unwrap();
    let keeper = LocalCleanup::trusting(store, starter, spawn);

    keeper.follow(&settings(Transcription::Local));
    until(|| loading.load(Ordering::SeqCst) == 1).await;
    keeper.follow(&settings(Transcription::Cloud));
    keeper.follow(&settings(Transcription::Local));
    // The first start is cancelled and has exited; only then does the
    // second load.
    until(|| cancelled.load(Ordering::SeqCst) == 1 && loading.load(Ordering::SeqCst) == 1).await;
    assert_eq!(most.load(Ordering::SeqCst), 1, "two helpers loaded at once");
    keeper.release();
    until(|| cancelled.load(Ordering::SeqCst) == 2).await;
    assert_eq!(loading.load(Ordering::SeqCst), 0);
    assert_eq!(most.load(Ordering::SeqCst), 1);
}
