//! The cleanup runner: the cleaner against a fake runner (criterion 2),
//! the HTTP side against wiremock, and the supervisor against a stand-in
//! helper script.

use std::time::Instant;

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

/// A runner with a scripted answer, recording what it was asked.
#[derive(Default)]
struct Fake {
    asked: Mutex<Vec<(String, String, u32)>>,
    fail: Option<LlamaError>,
}

impl ChatRunner for Fake {
    async fn complete(
        &self,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<String, LlamaError> {
        self.asked
            .lock()
            .unwrap()
            .push((system.to_owned(), user.to_owned(), max_tokens));
        match &self.fail {
            Some(error) => Err(error.clone()),
            None => Ok("Can you send me the report?".to_owned()),
        }
    }
}

#[tokio::test]
async fn criterion_2_the_port_cleaner_sends_prompt_v3_and_returns_the_reply() {
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
    assert_eq!(user, &cleanup::user_message(raw));
    assert_eq!(*max_tokens, local_cleanup::max_tokens(raw));
    // The pipeline's deadline and guard use these for this cleaner.
    assert_eq!(cleaner.deadline(), Some(local_cleanup::TIMEOUT));
    assert_eq!(cleaner.prompt(), Some(local_cleanup::SYSTEM_PROMPT));
}

#[tokio::test]
async fn criterion_2_errors_map_to_the_ports_so_the_pipeline_pastes_the_raw_text() {
    for (error, expected) in [
        (LlamaError::Status(500), CleanupError::Provider(500)),
        (LlamaError::Reply, CleanupError::BadReply),
        (LlamaError::Request("closed".into()), CleanupError::Network),
    ] {
        let cleaner = LocalCleaner::new(Fake {
            fail: Some(error),
            ..Fake::default()
        });
        assert_eq!(cleaner.clean("hello").await, Err(expected));
    }
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
            "choices": [{"message": {"role": "assistant", "content": " <transcript>Hello.</transcript>"}, "finish_reason": "stop"}]
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

/// Criterion 2: cancelling a cleanup (dropping its future, as the
/// pipeline's deadline does) closes the helper's connection, which is how
/// llama-server learns to stop that generation and free its one slot.
#[tokio::test]
async fn a_dropped_request_closes_its_connection_so_the_helper_stops_generating() {
    use tokio::io::AsyncReadExt as _;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    // A helper that reads the request, never answers (a long generation),
    // and reports when the client goes away.
    let closed = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0u8; 4096];
        loop {
            match socket.read(&mut buffer).await {
                Ok(0) | Err(_) => return true,
                Ok(_) => {}
            }
        }
    });
    let client = crate::http::client(&base_url).unwrap();
    let body = request_body("s", "u", 4_000);
    let request = complete_at(&client, &base_url, "k", &body);
    assert!(
        tokio::time::timeout(Duration::from_millis(200), request)
            .await
            .is_err()
    );
    // The client (and its pool) stays alive: the drop alone closes it.
    assert!(
        tokio::time::timeout(Duration::from_secs(2), closed)
            .await
            .expect("the connection stayed open after the request was dropped")
            .unwrap()
    );
    drop(client);
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

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_start_returns_only_once_its_helper_has_exited() {
    let dir = TempDir::new("cancel-start");
    let pid_file = dir.0.join("pid");
    // Never gets ready, like a model still loading; says its pid.
    let helper = dir.helper(&format!(
        "echo $$ > '{0}.tmp'\nmv '{0}.tmp' '{0}'\nexec sleep 300",
        pid_file.display()
    ));
    let model = dir.0.join("model.gguf");
    let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();
    let start = tokio::spawn({
        let helper = helper.clone();
        async move {
            LlamaServer::start_until(&helper, &model, async {
                let _ = cancelled.await;
            })
            .await
        }
    });
    let mut pid = None;
    for _ in 0..250 {
        if let Ok(text) = std::fs::read_to_string(&pid_file) {
            pid = text.trim().parse::<u32>().ok();
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let pid = pid.expect("the helper started");
    assert!(alive(pid));
    cancel.send(()).unwrap();
    assert!(matches!(start.await.unwrap(), Err(LlamaError::Cancelled)));
    // Already gone when the start returns: the next start can't overlap it.
    assert!(!alive(pid), "the cancelled helper is still running");
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
