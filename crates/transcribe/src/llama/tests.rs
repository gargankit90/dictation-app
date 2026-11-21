//! The cleanup runner: the cleaner against a fake runner (criterion 2),
//! the HTTP side against wiremock, and the supervisor against a stand-in
//! helper script.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

/// A runner with a scripted answer per call, recording what it was asked.
#[derive(Default)]
struct Fake {
    calls: AtomicUsize,
    asked: Mutex<Vec<(String, String, u32)>>,
    /// The first call never finishes; `dropped` says whether its future
    /// was dropped (the request cancelled).
    hang_first: bool,
    dropped: Arc<AtomicBool>,
    fail: bool,
}

/// Sets its flag when dropped: a request future that was cancelled.
struct OnDrop(Arc<AtomicBool>);

impl Drop for OnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl ChatRunner for Fake {
    fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> impl Future<Output = Result<String, LlamaError>> + Send {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        self.asked
            .lock()
            .unwrap()
            .push((system.to_owned(), user.to_owned(), max_tokens));
        let hang = self.hang_first && call == 0;
        let guard = OnDrop(Arc::clone(&self.dropped));
        let fail = self.fail;
        async move {
            if hang {
                let _guard = guard;
                std::future::pending::<()>().await;
            }
            if fail {
                return Err(LlamaError::Status(500));
            }
            Ok("Can you send me the report?".to_owned())
        }
    }
}

#[tokio::test]
async fn criterion_2_the_models_reply_is_returned_for_prompt_v3() {
    let fake = Arc::new(Fake::default());
    let cleaner = LocalCleaner::new(Arc::clone(&fake));
    let raw = "um can you like send me the the report";
    assert_eq!(
        cleaner.clean(raw).await.unwrap(),
        "Can you send me the report?"
    );
    let asked = fake.asked.lock().unwrap();
    let (system, user, max_tokens) = &asked[0];
    assert_eq!(system, local_cleanup::SYSTEM_PROMPT);
    assert_eq!(user, &format!("<transcript>\n{raw}\n</transcript>"));
    assert_eq!(*max_tokens, local_cleanup::max_tokens(raw));
}

#[tokio::test]
async fn criterion_2_an_error_is_returned_so_the_caller_pastes_the_raw_text() {
    let fake = Fake {
        fail: true,
        ..Fake::default()
    };
    let cleaner = LocalCleaner::new(fake);
    assert_eq!(cleaner.clean("hello").await, Err(LlamaError::Status(500)));
}

#[tokio::test]
async fn whitespace_is_not_sent_to_the_model() {
    let fake = Arc::new(Fake::default());
    let cleaner = LocalCleaner::new(Arc::clone(&fake));
    assert_eq!(cleaner.clean("  \n").await.unwrap(), "  \n");
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn criterion_2_a_timeout_cancels_the_request_and_the_next_job_runs_at_once() {
    let fake = Arc::new(Fake {
        hang_first: true,
        ..Fake::default()
    });
    let cleaner = LocalCleaner::with_timeout(Arc::clone(&fake), Duration::from_secs(4));
    let started = tokio::time::Instant::now();
    assert_eq!(cleaner.clean("first").await, Err(LlamaError::Timeout));
    assert_eq!(started.elapsed(), Duration::from_secs(4));
    // The hung request's future was dropped: its connection closes, which
    // stops llama-server's generation.
    assert!(fake.dropped.load(Ordering::SeqCst));
    // The next dictation isn't held up.
    let next = tokio::time::Instant::now();
    assert_eq!(
        cleaner.clean("second").await.unwrap(),
        "Can you send me the report?"
    );
    assert_eq!(next.elapsed(), Duration::ZERO);
}

#[test]
fn the_request_body_is_the_evals() {
    let body = request_body("sys", "<transcript>\nhi\n</transcript>", 101);
    assert_eq!(
        body,
        json!({
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "user", "content": "<transcript>\nhi\n</transcript>"},
            ],
            "temperature": 0.0,
            "top_k": 1,
            "seed": 1,
            "max_tokens": 101,
            "chat_template_kwargs": {"enable_thinking": false},
            "cache_prompt": true,
            "stream": false,
        })
    );
}

#[test]
fn the_server_is_loopback_only_without_ui_log_or_key_in_its_arguments() {
    let args = server_args(Path::new("/m/gemma.gguf"), 4242);
    let joined = args.join(" ");
    assert_eq!(
        joined,
        "-m /m/gemma.gguf --host 127.0.0.1 --port 4242 -ngl 99 -c 4096 -np 1 \
         --no-webui --no-slots --reasoning off --temp 0 --seed 1 --log-disable"
    );
    assert!(!joined.contains("api-key"));
}

#[test]
fn keys_are_random_and_long() {
    let (a, b) = (new_key().unwrap(), new_key().unwrap());
    assert_eq!(a.len(), 64);
    assert_ne!(a, b);
}

#[tokio::test]
async fn a_completion_sends_the_key_and_reads_the_reply() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer k3y"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"role": "assistant", "content": "Hello."}}]
        })))
        .mount(&server)
        .await;
    let client = crate::http::client(&server.uri()).unwrap();
    let body = request_body("s", "u", 100);
    assert_eq!(
        complete_at(&client, &server.uri(), "k3y", &body)
            .await
            .unwrap(),
        "Hello."
    );
    // The wrong key is refused by the helper: an HTTP error.
    assert_eq!(
        complete_at(&client, &server.uri(), "other", &body).await,
        Err(LlamaError::Status(404))
    );
}

#[tokio::test]
async fn http_errors_and_odd_answers_are_typed() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"choices": []})))
        .mount(&server)
        .await;
    let client = crate::http::client(&server.uri()).unwrap();
    let body = request_body("s", "u", 100);
    assert_eq!(
        complete_at(&client, &server.uri(), "k", &body).await,
        Err(LlamaError::Status(503))
    );
    assert_eq!(
        complete_at(&client, &server.uri(), "k", &body).await,
        Err(LlamaError::Reply)
    );
    // Nothing listening: a request error.
    let closed = format!("http://127.0.0.1:{}", free_port().unwrap());
    assert!(matches!(
        complete_at(&client, &closed, "k", &body).await,
        Err(LlamaError::Request(_))
    ));
}

#[tokio::test]
async fn health_is_ok_only_when_the_model_is_loaded() {
    let server = MockServer::start().await;
    let client = crate::http::client(&server.uri()).unwrap();
    assert!(!health_ok(&client, &server.uri()).await);
    Mock::given(path("/health"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"status": "loading"})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    assert!(!health_ok(&client, &server.uri()).await);
    Mock::given(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "ok"})))
        .mount(&server)
        .await;
    assert!(health_ok(&client, &server.uri()).await);
}

/// A folder for stand-in helpers, removed afterwards.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("q4-llama-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// An executable shell script named `llama-server`.
    fn helper(&self, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let path = self.0.join(HELPER_NAME);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Whether process `pid` exists.
fn alive(pid: u32) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

async fn gone_within(pid: u32, limit: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if !alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn closing_the_pipe_kills_the_server_and_the_key_is_only_in_its_environment() {
    let dir = TempDir::new("pipe");
    let seen = dir.0.join("seen");
    // Written aside, then renamed: the file never shows up half written.
    let helper = dir.helper(&format!(
        "printf '%s|%s' \"$LLAMA_API_KEY\" \"$*\" > '{0}.tmp'\nmv '{0}.tmp' '{0}'\nexec sleep 300",
        seen.display()
    ));
    let supervised = spawn_supervised(&helper, &["--port".into(), "1".into()], "s3cret")
        .await
        .unwrap();
    let pid = supervised.server_pid;
    assert!(alive(pid));
    for _ in 0..100 {
        if seen.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(std::fs::read_to_string(&seen).unwrap(), "s3cret|--port 1");
    // The app goes away (stop, drop or a crash): the pipe closes.
    drop(supervised);
    assert!(gone_within(pid, Duration::from_secs(5)).await, "orphaned");
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_kills_the_server_and_the_supervisor_exits_with_it() {
    let dir = TempDir::new("stop");
    let helper = dir.helper("exec sleep 300");
    let supervised = spawn_supervised(&helper, &[], "k").await.unwrap();
    let pid = supervised.server_pid;
    let server = LlamaServer {
        base_url: "http://127.0.0.1:9".to_owned(),
        key: "k".to_owned(),
        client: crate::http::client("http://127.0.0.1:9").unwrap(),
        supervisor: Mutex::new(Some(supervised)),
    };
    assert!(server.is_running());
    assert_eq!(server.server_pid(), Some(pid));
    server.stop();
    assert!(!server.is_running());
    assert!(gone_within(pid, Duration::from_secs(5)).await);
    server.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_helper_that_exits_while_starting_is_an_error_not_a_hang() {
    let dir = TempDir::new("exits");
    let helper = dir.helper("exit 3");
    let model = dir.0.join("model.gguf");
    let started = Instant::now();
    let error = LlamaServer::start(&helper, &model).await.unwrap_err();
    assert!(matches!(error, LlamaError::Start(_)), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_helper_that_never_gets_ready_is_stopped_after_the_limit() {
    let dir = TempDir::new("never-ready");
    let helper = dir.helper("exec sleep 300");
    let supervised = spawn_supervised(&helper, &[], "k").await.unwrap();
    let pid = supervised.server_pid;
    let closed = format!("http://127.0.0.1:{}", free_port().unwrap());
    let server = LlamaServer {
        client: crate::http::client(&closed).unwrap(),
        base_url: closed,
        key: "k".to_owned(),
        supervisor: Mutex::new(Some(supervised)),
    };
    assert_eq!(
        server.wait_ready(Duration::from_millis(500)).await,
        Err(LlamaError::StartTimeout)
    );
    assert!(gone_within(pid, Duration::from_secs(5)).await);
}

#[tokio::test]
async fn a_missing_helper_is_named() {
    let dir = TempDir::new("missing");
    assert_eq!(
        LlamaServer::start(&dir.0.join(HELPER_NAME), &dir.0.join("m.gguf"))
            .await
            .unwrap_err(),
        LlamaError::HelperMissing
    );
    assert_eq!(helper_next_to(&dir.0.join("Vocet")), None);
    dir.helper("exit 0");
    assert_eq!(
        helper_next_to(&dir.0.join("Vocet")),
        Some(dir.0.join(HELPER_NAME))
    );
}

#[test]
fn debug_never_shows_the_key() {
    let server = LlamaServer {
        base_url: "http://127.0.0.1:9".to_owned(),
        key: "s3cret-key".to_owned(),
        client: crate::http::client("http://127.0.0.1:9").unwrap(),
        supervisor: Mutex::new(None),
    };
    assert!(!format!("{server:?}").contains("s3cret"));
}
