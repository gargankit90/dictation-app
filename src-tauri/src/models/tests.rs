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
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, DEFAULT);
    assert!(list[0].default && !list[1].default);
    assert_eq!(list[0].languages, "English only");
    // The language picker (U2) filters by these codes.
    assert_eq!(list[0].language_codes, ["en"]);
    assert!(list[1].language_codes.iter().any(|c| c == "hi"));
    for (info, model) in list.iter().zip(local_models::CATALOGUE) {
        assert_eq!(info.language_codes, model.languages);
    }
    assert!(list.iter().all(|m| m.state == ModelState::Missing));
    assert_eq!(list[0].size_bytes, 477_274_496.0);

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
    downloads.delete(&live, DEFAULT).unwrap();
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
        info(&list, "qwen3-asr-0.6b-q4_k_m").state,
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
    const QWEN: &str = "qwen3-asr-0.6b-q4_k_m";
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
    // Enough for Qwen (the larger) plus headroom, not for both.
    let free = 589_560_480 + dictation_transcribe::models::SPACE_HEADROOM + 1;
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
    let refused = downloads.start(&live, QWEN, Some(server.uri()), |_| {
        panic!("a second download started")
    });
    assert_eq!(refused, Err(ONE_AT_A_TIME.to_owned()));
    assert!(!downloads.is_running(QWEN));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert!(!dir.0.join("Qwen3-ASR-0.6B-Q4_K_M.gguf.part").exists());

    // Once the first is cancelled, the other may start.
    downloads.cancel(DEFAULT);
    let _ = tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .unwrap();
    downloads
        .start(&live, QWEN, Some(server.uri()), |_| {})
        .unwrap();
    assert!(downloads.is_running(QWEN));
    downloads.cancel(QWEN);
}
