//! The keeper against a fake helper: warm while wanted, released on switch.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dictation_core::settings::Settings;

use super::*;

/// A started helper that counts how many are alive.
struct FakeServer {
    alive: Arc<AtomicUsize>,
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.alive.fetch_sub(1, Ordering::SeqCst);
    }
}

impl ChatRunner for FakeServer {
    async fn complete(&self, _: &str, _: &str, _: u32) -> Result<String, LlamaError> {
        Ok("Cleaned.".to_owned())
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
}

fn rig(dir: &TempDir, delay: Duration, fail: bool) -> Rig {
    let starts = Arc::new(AtomicUsize::new(0));
    let alive = Arc::new(AtomicUsize::new(0));
    let (s, a) = (Arc::clone(&starts), Arc::clone(&alive));
    let starter: Starter<FakeServer> = Arc::new(move |path: PathBuf| {
        let (s, a) = (Arc::clone(&s), Arc::clone(&a));
        Box::pin(async move {
            assert!(path.ends_with(local_models::GEMMA_4_E2B.file_name));
            s.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(delay).await;
            if fail {
                return Err(LlamaError::StartTimeout);
            }
            a.fetch_add(1, Ordering::SeqCst);
            Ok(FakeServer { alive: a })
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
    assert_eq!(cleaner.clean("um hi").await.unwrap(), "Cleaned.");
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
