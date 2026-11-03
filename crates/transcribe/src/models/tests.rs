//! The model store against a local HTTP server (wiremock) that honours, or
//! ignores, `Range` requests.

use std::sync::Mutex;
use std::sync::atomic::AtomicU64;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use super::*;

/// A fresh, empty folder; removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("l1-models-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Test bytes: 3.5 MB, so progress is reported more than once.
fn content() -> Vec<u8> {
    (0..3_500_000u32).map(|i| (i % 251) as u8).collect()
}

fn hex_sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A catalogue entry for `bytes`.
fn model_for(bytes: &[u8]) -> LocalModel {
    LocalModel {
        id: "test-model",
        name: "Test",
        languages: &["en"],
        languages_label: "English only",
        size_bytes: bytes.len() as u64,
        url: "https://example.invalid/model.gguf",
        sha256: Box::leak(hex_sha(bytes).into_boxed_str()),
        file_name: "test-model.gguf",
        licence: "MIT",
    }
}

fn plenty() -> FreeSpace {
    Arc::new(|_| Ok(u64::MAX))
}

fn store(dir: &TempDir, free: FreeSpace) -> ModelStore {
    ModelStore::new(dir.0.clone(), free).unwrap()
}

/// Serves `body`, honouring `Range: bytes=N-` with a 206 unless
/// `ignore_range`. Records the Range header of every request.
struct Ranged {
    body: Vec<u8>,
    ignore_range: bool,
    ranges: Arc<Mutex<Vec<Option<String>>>>,
}

impl Respond for Ranged {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let range = request
            .headers
            .get("range")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        self.ranges.lock().unwrap().push(range.clone());
        let start = range
            .as_deref()
            .and_then(|r| r.strip_prefix("bytes="))
            .and_then(|r| r.strip_suffix('-'))
            .and_then(|n| n.parse::<usize>().ok());
        match start {
            Some(start) if !self.ignore_range => {
                if start >= self.body.len() {
                    return ResponseTemplate::new(416);
                }
                ResponseTemplate::new(206).set_body_bytes(self.body[start..].to_vec())
            }
            _ => ResponseTemplate::new(200).set_body_bytes(self.body.clone()),
        }
    }
}

async fn serve(body: Vec<u8>, ignore_range: bool) -> (MockServer, Arc<Mutex<Vec<Option<String>>>>) {
    let server = MockServer::start().await;
    let ranges = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("GET"))
        .and(path("/model.gguf"))
        .respond_with(Ranged {
            body,
            ignore_range,
            ranges: Arc::clone(&ranges),
        })
        .mount(&server)
        .await;
    (server, ranges)
}

fn url(server: &MockServer) -> String {
    format!("{}/model.gguf", server.uri())
}

#[tokio::test]
async fn criterion_3_downloads_verifies_and_reports_progress() {
    let bytes = content();
    let model = model_for(&bytes);
    let (server, ranges) = serve(bytes.clone(), false).await;
    let dir = TempDir::new();
    let store = store(&dir, plenty());
    assert_eq!(store.status(&model), ModelStatus::Missing);

    let mut seen = Vec::new();
    let path = store
        .download_from(&model, &url(&server), &DownloadCancel::new(), |p| {
            seen.push(p)
        })
        .await
        .unwrap();

    assert_eq!(path, dir.0.join("test-model.gguf"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(store.status(&model), ModelStatus::Ready);
    assert_eq!(store.ready_path(&model), Some(path));
    assert!(!dir.0.join("test-model.gguf.part").exists());
    assert_eq!(*ranges.lock().unwrap(), [None]);
    // From zero to the total, never backwards, more than one step.
    assert_eq!(seen.first().unwrap().downloaded, 0);
    assert_eq!(seen.last().unwrap().downloaded, bytes.len() as u64);
    assert!(seen.windows(2).all(|w| w[0].downloaded <= w[1].downloaded));
    assert!(seen.len() >= 3, "{seen:?}");
    assert!(seen.iter().all(|p| p.total == bytes.len() as u64));
}

#[tokio::test]
async fn criterion_3_resumes_a_partial_download_with_a_range_request() {
    let bytes = content();
    let model = model_for(&bytes);
    let (server, ranges) = serve(bytes.clone(), false).await;
    let dir = TempDir::new();
    std::fs::create_dir_all(&dir.0).unwrap();
    std::fs::write(dir.0.join("test-model.gguf.part"), &bytes[..1_000_000]).unwrap();
    let store = store(&dir, plenty());
    assert_eq!(
        store.status(&model),
        ModelStatus::Partial {
            downloaded: 1_000_000
        }
    );

    let mut first = None;
    let path = store
        .download_from(&model, &url(&server), &DownloadCancel::new(), |p| {
            first.get_or_insert(p);
        })
        .await
        .unwrap();

    assert_eq!(*ranges.lock().unwrap(), [Some("bytes=1000000-".to_owned())]);
    assert_eq!(first.unwrap().downloaded, 1_000_000);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[tokio::test]
async fn criterion_3_a_server_that_ignores_the_range_starts_the_part_again() {
    let bytes = content();
    let model = model_for(&bytes);
    let (server, _) = serve(bytes.clone(), true).await;
    let dir = TempDir::new();
    std::fs::create_dir_all(&dir.0).unwrap();
    std::fs::write(dir.0.join("test-model.gguf.part"), &bytes[..500]).unwrap();
    let path = store(&dir, plenty())
        .download_from(&model, &url(&server), &DownloadCancel::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[tokio::test]
async fn criterion_3_a_file_with_the_wrong_hash_is_deleted_and_never_used() {
    let bytes = content();
    let mut model = model_for(&bytes);
    model.sha256 = "0000000000000000000000000000000000000000000000000000000000000000";
    let (server, _) = serve(bytes, false).await;
    let dir = TempDir::new();
    let store = store(&dir, plenty());
    let error = store
        .download_from(&model, &url(&server), &DownloadCancel::new(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(error, DownloadError::Checksum);
    assert_eq!(store.status(&model), ModelStatus::Missing);
    assert_eq!(store.ready_path(&model), None);
}

#[tokio::test]
async fn criterion_3_a_longer_body_than_the_catalogue_says_is_a_checksum_error() {
    let bytes = content();
    let model = model_for(&bytes[..1000]);
    let (server, _) = serve(bytes, false).await;
    let dir = TempDir::new();
    let store = store(&dir, plenty());
    let error = store
        .download_from(&model, &url(&server), &DownloadCancel::new(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(error, DownloadError::Checksum);
    assert_eq!(store.status(&model), ModelStatus::Missing);
}

#[tokio::test]
async fn criterion_3_no_space_is_refused_before_anything_is_fetched() {
    let bytes = content();
    let model = model_for(&bytes);
    let (server, ranges) = serve(bytes.clone(), false).await;
    let dir = TempDir::new();
    let store = store(&dir, Arc::new(|_| Ok(1_000)));
    let error = store
        .download_from(&model, &url(&server), &DownloadCancel::new(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(
        error,
        DownloadError::NoSpace {
            needed: bytes.len() as u64 + SPACE_HEADROOM,
            available: 1_000
        }
    );
    assert!(ranges.lock().unwrap().is_empty());
    assert!(
        error
            .user_message()
            .starts_with("Not enough disk space. Free up about")
    );
    // Only what is left of a part counts.
    std::fs::write(dir.0.join("test-model.gguf.part"), &bytes[..3_000_000]).unwrap();
    let just_enough = 500_000 + SPACE_HEADROOM;
    let store = ModelStore::new(dir.0.clone(), Arc::new(move |_| Ok(just_enough))).unwrap();
    store
        .download_from(&model, &url(&server), &DownloadCancel::new(), |_| {})
        .await
        .unwrap();
}

#[tokio::test]
async fn a_free_space_error_is_a_disk_error() {
    let bytes = content();
    let model = model_for(&bytes);
    let dir = TempDir::new();
    let store = store(&dir, Arc::new(|_| Err(io::Error::other("statvfs failed"))));
    let error = store
        .download_from(
            &model,
            "http://127.0.0.1:9/x",
            &DownloadCancel::new(),
            |_| {},
        )
        .await
        .unwrap_err();
    assert!(
        matches!(error, DownloadError::Disk(ref t) if t.contains("statvfs")),
        "{error:?}"
    );
}

#[tokio::test]
async fn criterion_3_cancel_stops_a_running_download_and_keeps_the_part() {
    let bytes = content();
    let model = model_for(&bytes);
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(bytes)
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let dir = TempDir::new();
    let store = store(&dir, plenty());
    let cancel = DownloadCancel::new();
    let canceller = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        canceller.cancel();
    });
    let started = std::time::Instant::now();
    let error = store
        .download_from(&model, &url(&server), &cancel, |_| {})
        .await
        .unwrap_err();
    assert_eq!(error, DownloadError::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_ne!(store.status(&model), ModelStatus::Ready);
    // A token cancelled before the start never sends a request.
    let error = store
        .download_from(&model, &url(&server), &cancel, |_| {})
        .await
        .unwrap_err();
    assert_eq!(error, DownloadError::Cancelled);
    assert_eq!(error.user_message(), "Download cancelled.");
}

#[tokio::test]
async fn criterion_3_a_short_body_is_a_network_error_and_the_next_try_resumes() {
    let bytes = content();
    let model = model_for(&bytes);
    let server = MockServer::start().await;
    // First answer: only the first megabyte, as if the connection dropped.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes[..1_000_000].to_vec()))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let ranges = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("GET"))
        .respond_with(Ranged {
            body: bytes.clone(),
            ignore_range: false,
            ranges: Arc::clone(&ranges),
        })
        .mount(&server)
        .await;
    let dir = TempDir::new();
    let store = store(&dir, plenty());
    let error = store
        .download_from(&model, &url(&server), &DownloadCancel::new(), |_| {})
        .await
        .unwrap_err();
    assert!(matches!(error, DownloadError::Network(_)), "{error:?}");
    assert!(error.user_message().contains("continues where it stopped"));
    assert_eq!(
        store.status(&model),
        ModelStatus::Partial {
            downloaded: 1_000_000
        }
    );
    let path = store
        .download_from(&model, &url(&server), &DownloadCancel::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(*ranges.lock().unwrap(), [Some("bytes=1000000-".to_owned())]);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[tokio::test]
async fn criterion_3_http_errors_and_unreachable_servers_are_network_errors() {
    let bytes = content();
    let model = model_for(&bytes);
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let dir = TempDir::new();
    let store = store(&dir, plenty());
    let error = store
        .download_from(&model, &url(&server), &DownloadCancel::new(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(
        error,
        DownloadError::Network("the server answered HTTP 503".into())
    );
    let error = store
        .download_from(
            &model,
            "http://127.0.0.1:9/x",
            &DownloadCancel::new(),
            |_| {},
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DownloadError::Network(_)), "{error:?}");
}

#[tokio::test]
async fn a_part_the_server_cannot_resume_is_dropped() {
    let bytes = content();
    let model = model_for(&bytes);
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(416))
        .mount(&server)
        .await;
    let dir = TempDir::new();
    std::fs::create_dir_all(&dir.0).unwrap();
    std::fs::write(dir.0.join("test-model.gguf.part"), &bytes[..10]).unwrap();
    let store = store(&dir, plenty());
    let error = store
        .download_from(&model, &url(&server), &DownloadCancel::new(), |_| {})
        .await
        .unwrap_err();
    assert!(matches!(error, DownloadError::Network(_)));
    assert_eq!(store.status(&model), ModelStatus::Missing);
}

#[tokio::test]
async fn redirects_to_plain_http_are_refused() {
    let bytes = content();
    let model = model_for(&bytes);
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "http://127.0.0.1:9/elsewhere"),
        )
        .mount(&server)
        .await;
    let dir = TempDir::new();
    let error = store(&dir, plenty())
        .download_from(&model, &url(&server), &DownloadCancel::new(), |_| {})
        .await
        .unwrap_err();
    assert_eq!(
        error,
        DownloadError::Network("too many or unsafe redirects".into())
    );
}

#[tokio::test]
async fn criterion_3_delete_removes_the_model_and_any_part() {
    let bytes = content();
    let model = model_for(&bytes);
    let dir = TempDir::new();
    let store = store(&dir, plenty());
    store.delete(&model).unwrap();
    std::fs::create_dir_all(&dir.0).unwrap();
    std::fs::write(dir.0.join("test-model.gguf"), &bytes).unwrap();
    std::fs::write(dir.0.join("test-model.gguf.part"), b"x").unwrap();
    assert_eq!(store.status(&model), ModelStatus::Ready);
    store.delete(&model).unwrap();
    assert_eq!(store.status(&model), ModelStatus::Missing);
}

#[tokio::test]
async fn a_downloaded_model_is_not_fetched_again_and_a_wrong_size_is_not_ready() {
    let bytes = content();
    let model = model_for(&bytes);
    let dir = TempDir::new();
    std::fs::create_dir_all(&dir.0).unwrap();
    let path = dir.0.join("test-model.gguf");
    std::fs::write(&path, &bytes).unwrap();
    let store = store(&dir, plenty());
    // No server at all: it returns before fetching.
    let got = store
        .download_from(
            &model,
            "http://127.0.0.1:9/x",
            &DownloadCancel::new(),
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(got, path);
    std::fs::write(&path, b"truncated").unwrap();
    assert_eq!(store.ready_path(&model), None);
    assert_eq!(store.status(&model), ModelStatus::Missing);
    assert!(format!("{store:?}").contains("ModelStore"));
    assert_eq!(store.dir(), dir.0.as_path());
}

#[test]
fn messages_say_what_to_do_and_hide_details() {
    assert_eq!(megabytes(477_274_496), "478 MB");
    assert_eq!(
        DownloadError::Checksum.user_message(),
        "The downloaded file was damaged and has been removed. Try again."
    );
    let disk = DownloadError::Disk("EACCES /Users/someone/secret".into());
    assert!(!disk.user_message().contains("secret"));
    let long = DownloadError::Network(format!("x\n{}", "y".repeat(2000)));
    assert!(!long.to_string().contains('\n') && long.to_string().len() < 300);
}

#[test]
fn hashes_a_file() {
    let dir = TempDir::new();
    std::fs::create_dir_all(&dir.0).unwrap();
    let path = dir.0.join("abc");
    std::fs::write(&path, b"abc").unwrap();
    assert_eq!(
        sha256_file(&path).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
