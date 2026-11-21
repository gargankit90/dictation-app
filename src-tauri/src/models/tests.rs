//! The model commands' logic, against a local HTTP server.

use std::path::{Path, PathBuf};
use std::time::Duration;

use dictation_transcribe::local::SpeechModel;
use dictation_transcribe::models::ModelStore;
use tokio::sync::oneshot;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::engine::LocalModels;

const DEFAULT: &str = local_models::DEFAULT_MODEL;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("l1-cmds-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Silent;

impl SpeechModel for Silent {
    fn run(&mut self, _: &[f32], _: Option<&str>) -> Result<String, String> {
        Ok(String::new())
    }
}

fn live(dir: &Path) -> LiveEngine {
    let store = ModelStore::new(dir.to_owned(), Arc::new(|_| Ok(u64::MAX))).unwrap();
    let local = LocalModels::trusting(
        store,
        Arc::new(|_: &Path| Ok(Box::new(Silent) as Box<dyn SpeechModel>)),
    );
    LiveEngine::fixed().with_local(local)
}

fn info<'a>(list: &'a [LocalModelInfo], id: &str) -> &'a LocalModelInfo {
    list.iter().find(|m| m.id == id).unwrap()
}

#[test]
fn criterion_4_lists_the_catalogue_with_statuses() {
    let dir = TempDir::new("list");
    let live = live(&dir.0);
    let downloads = ModelDownloads::default();
    let list = downloads.list(&live);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, DEFAULT);
    assert!(list[0].default);
    assert_eq!(list[0].languages, "English only");
    // The language picker (U2) filters by these codes.
    assert_eq!(list[0].language_codes, ["en"]);
    for (info, model) in list.iter().zip(local_models::CATALOGUE) {
        assert_eq!(info.language_codes, model.languages);
    }
    assert!(list.iter().all(|m| m.state == ModelState::Missing));
    assert_eq!(list[0].size_bytes, 1_319_830_656.0);
    // Cleanup models are listed on their own (Q4), never as speech.
    let cleanup = downloads.list_cleanup(&live);
    assert_eq!(cleanup.len(), 1);
    assert_eq!(cleanup[0].id, local_models::DEFAULT_CLEANUP_MODEL);
    assert!(cleanup[0].default);
    assert_eq!(cleanup[0].size_bytes, 3_106_738_272.0);

    // A part, then a full file of the right size.
    let model = local_models::find(DEFAULT).unwrap();
    std::fs::create_dir_all(&dir.0).unwrap();
    let part = dir.0.join(format!("{}.part", model.file_name));
    std::fs::write(&part, [0u8; 10]).unwrap();
    let list = downloads.list(&live);
    assert_eq!(info(&list, DEFAULT).state, ModelState::Partial);
    assert_eq!(info(&list, DEFAULT).downloaded_bytes, 10.0);
    std::fs::File::create(dir.0.join(model.file_name))
        .unwrap()
        .set_len(model.size_bytes)
        .unwrap();
    assert_eq!(
        info(&downloads.list(&live), DEFAULT).state,
        ModelState::Ready
    );

    // Deleting removes both and lets go of the loaded model.
    let local = live.local().unwrap();
    assert!(matches!(
        local.engine(DEFAULT, None, &|_| {}),
        dictation_transcribe::engine::Engine::Local(_)
    ));
    assert_eq!(local.loaded(), Some(DEFAULT));
    // The cleanup model is part of "On this Mac" (Q4): it goes with it.
    let gemma = local_models::find_any(local_models::DEFAULT_CLEANUP_MODEL).unwrap();
    std::fs::write(dir.0.join(gemma.file_name), [0u8; 10]).unwrap();
    downloads.delete(&live, DEFAULT).unwrap();
    assert!(!dir.0.join(gemma.file_name).exists());
    assert_eq!(local.loaded(), None);
    assert_eq!(
        info(&downloads.list(&live), DEFAULT).state,
        ModelState::Missing
    );
    assert!(!part.exists());
}

#[test]
fn unknown_ids_and_builds_without_models_are_errors() {
    let dir = TempDir::new("unknown");
    let downloads = ModelDownloads::default();
    let error = downloads
        .start(&live(&dir.0), "whisper-1", None, |_| {})
        .unwrap_err();
    assert!(error.contains("whisper-1"), "{error}");
    let fixed = LiveEngine::fixed();
    assert!(downloads.start(&fixed, DEFAULT, None, |_| {}).is_err());
    assert!(downloads.delete(&fixed, DEFAULT).is_err());
    assert!(
        downloads
            .list(&fixed)
            .iter()
            .all(|m| m.state == ModelState::Missing)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn criterion_3_a_failed_download_is_reported_in_plain_words() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![1u8; 1000]))
        .mount(&server)
        .await;
    let dir = TempDir::new("failed");
    let live = live(&dir.0);
    let downloads = ModelDownloads::default();
    let (tx, rx) = oneshot::channel();
    downloads
        .start(&live, DEFAULT, Some(server.uri()), move |result| {
            let _ = tx.send(result);
        })
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .unwrap()
        .unwrap();
    // 1000 bytes is not the model: it ended early.
    assert!(
        matches!(result, Err(DownloadError::Network(_))),
        "{result:?}"
    );
    let list = downloads.list(&live);
    let shown = info(&list, DEFAULT);
    assert_eq!(shown.state, ModelState::Partial);
    assert!(
        shown
            .error
            .as_deref()
            .unwrap()
            .starts_with("The download failed."),
        "{shown:?}"
    );
    assert!(!downloads.is_running(DEFAULT));
}

#[tokio::test(flavor = "multi_thread")]
async fn criterion_3_a_running_download_shows_progress_starts_once_and_cancels_quietly() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(vec![1u8; 1000])
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let dir = TempDir::new("cancel");
    let live = live(&dir.0);
    let downloads = ModelDownloads::default();
    let (tx, rx) = oneshot::channel();
    downloads
        .start(&live, DEFAULT, Some(server.uri()), move |result| {
            let _ = tx.send(result);
        })
        .unwrap();
    assert!(downloads.is_running(DEFAULT));
    // A second start while it runs does nothing (and its `done` never runs).
    downloads
        .start(&live, DEFAULT, Some(server.uri()), |_| {
            panic!("a second download started")
        })
        .unwrap();
    let list = downloads.list(&live);
    assert_eq!(info(&list, DEFAULT).state, ModelState::Downloading);
    assert_eq!(
        info(
            &downloads.list_cleanup(&live),
            local_models::DEFAULT_CLEANUP_MODEL
        )
        .state,
        ModelState::Missing
    );

    downloads.cancel(DEFAULT);
    let result = tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, Err(DownloadError::Cancelled));
    let list = downloads.list(&live);
    assert_eq!(info(&list, DEFAULT).error, None);
    assert_ne!(info(&list, DEFAULT).state, ModelState::Downloading);
    // Cancelling what isn't running is fine.
    downloads.cancel(DEFAULT);
}

/// Review round 1, major 2: with room for either model but not both, the
/// second download is refused before it sends a request or writes a byte.
#[tokio::test(flavor = "multi_thread")]
async fn only_one_model_downloads_at_a_time() {
    const GEMMA: &str = local_models::DEFAULT_CLEANUP_MODEL;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(vec![1u8; 1000])
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let dir = TempDir::new("one-at-a-time");
    // Enough for Gemma (the larger) plus headroom, not for both.
    let free =
        local_models::GEMMA_4_E2B.size_bytes + dictation_transcribe::models::SPACE_HEADROOM + 1;
    let store = ModelStore::new(dir.0.clone(), Arc::new(move |_| Ok(free))).unwrap();
    let live = LiveEngine::fixed().with_local(LocalModels::trusting(
        store,
        Arc::new(|_: &Path| Ok(Box::new(Silent) as Box<dyn SpeechModel>)),
    ));
    let downloads = ModelDownloads::default();
    let (tx, rx) = oneshot::channel();
    downloads
        .start(&live, DEFAULT, Some(server.uri()), move |result| {
            let _ = tx.send(result);
        })
        .unwrap();
    // Wait for the first request, so the second would really overlap.
    for _ in 0..100 {
        if !server.received_requests().await.unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let refused = downloads.start(&live, GEMMA, Some(server.uri()), |_| {
        panic!("a second download started")
    });
    assert_eq!(refused, Err(ONE_AT_A_TIME.to_owned()));
    assert!(!downloads.is_running(GEMMA));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert!(!dir.0.join("gemma-4-E2B-it-Q4_K_M.gguf.part").exists());

    // Once the first is cancelled, the other may start.
    downloads.cancel(DEFAULT);
    let _ = tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .unwrap();
    downloads
        .start(&live, GEMMA, Some(server.uri()), |_| {})
        .unwrap();
    assert!(downloads.is_running(GEMMA));
    downloads.cancel(GEMMA);
}

/// Q4: after the speech model, the cleanup model downloads too, unless it
/// is already on this Mac.
#[tokio::test(flavor = "multi_thread")]
async fn the_cleanup_model_follows_the_speech_model() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(vec![1u8; 1000])
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let dir = TempDir::new("chain");
    let live = live(&dir.0);
    let downloads = ModelDownloads::default().from_stand_in(&server.uri());
    let gemma = local_models::DEFAULT_CLEANUP_MODEL;
    downloads.then_cleanup(&live);
    assert!(downloads.is_running(gemma));
    for _ in 0..100 {
        if !server.received_requests().await.unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].url.path(),
        format!("/{}", local_models::GEMMA_4_E2B.file_name)
    );
    downloads.cancel(gemma);
    for _ in 0..250 {
        if !downloads.is_running(gemma) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Already downloaded: nothing starts.
    let model = &local_models::GEMMA_4_E2B;
    std::fs::File::create(dir.0.join(model.file_name))
        .unwrap()
        .set_len(model.size_bytes)
        .unwrap();
    downloads.then_cleanup(&live);
    assert!(!downloads.is_running(gemma));
}

/// Review finding 1: when the chained cleanup download finishes, the
/// helper starts with no settings save or restart.
#[tokio::test(flavor = "multi_thread")]
async fn a_finished_cleanup_download_starts_the_helper() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use dictation_core::settings::{Settings, Transcription};
    use dictation_transcribe::llama::{LlamaError, LlamaServer};

    use crate::cleanup::{LocalCleanup, Spawner, Starter};

    let dir = TempDir::new("chain-done");
    std::fs::create_dir_all(&dir.0).unwrap();
    let starts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&starts);
    let starter: Starter<LlamaServer> = Arc::new(move |_, _| {
        counted.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(LlamaError::HelperMissing) })
    });
    let spawn: Spawner = Arc::new(|task| {
        tokio::spawn(task);
    });
    let store = ModelStore::new(dir.0.clone(), Arc::new(|_| Ok(u64::MAX))).unwrap();
    let live = live(&dir.0).with_cleanup(LocalCleanup::trusting(store, starter, spawn));
    let keeper = live.cleanup().unwrap();
    // The speech model's rebuild: on-device chosen, Gemma not here yet.
    keeper.follow(&Settings {
        transcription: Transcription::Local,
        ..Settings::default()
    });
    // A failed download changes nothing.
    cleanup_downloaded(&live, &Err(DownloadError::Cancelled));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(starts.load(Ordering::SeqCst), 0);
    // Gemma lands: the helper is started.
    let gemma = &local_models::GEMMA_4_E2B;
    std::fs::File::create(dir.0.join(gemma.file_name))
        .unwrap()
        .set_len(gemma.size_bytes)
        .unwrap();
    cleanup_downloaded(&live, &Ok(()));
    for _ in 0..100 {
        if starts.load(Ordering::SeqCst) == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the helper wasn't started");
}
