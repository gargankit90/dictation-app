use std::path::Path;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

/// Text that must never leave the process.
const SECRETS: [&str; 6] = [
    "gsk_PLANTED_API_KEY",
    "my private dictated words",
    "/Users/gauri/Documents/secret-project",
    "gauri",
    "Gauris-MacBook",
    "hunter2",
];

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dictation-sn-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn reporter(server: Option<&MockServer>, dir: &Path, enabled: bool) -> Arc<SentryReporter> {
    let dsn = server.map(|s| format!("http://abc123@{}/7", s.address()));
    let version = Version::new("0.3.0").expect("version");
    let config = Config::new(dsn.as_deref(), &version, "production", 15, dir);
    SentryReporter::start(config, enabled, &tokio::runtime::Handle::current())
}

async fn requests(server: &MockServer, want: usize) -> Vec<wiremock::Request> {
    for _ in 0..60 {
        let got = server.received_requests().await.unwrap_or_default();
        if got.len() >= want {
            return got;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    server.received_requests().await.unwrap_or_default()
}

async fn ok_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/7/envelope/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

fn leaky_errors() -> Vec<DictationError> {
    vec![
        DictationError::TranscriptionFailed {
            reason: format!("{} {} at {}", SECRETS[0], SECRETS[1], SECRETS[2]),
        },
        DictationError::InsertFailed {
            reason: format!("{} on {} ({})", SECRETS[5], SECRETS[4], SECRETS[3]),
        },
    ]
}

fn assert_clean(bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    for secret in SECRETS {
        assert!(!text.contains(secret), "leaked {secret:?} in {text}");
    }
}

#[test]
fn dsns_are_https_with_a_public_key_and_a_project() {
    let ok = Dsn::parse("https://abc@o1.ingest.us.sentry.io/42").expect("dsn");
    assert_eq!(
        ok.envelope_url(),
        "https://o1.ingest.us.sentry.io/api/42/envelope/"
    );
    assert!(ok.auth().ends_with("sentry_key=abc"));
    assert!(Dsn::parse("http://abc@127.0.0.1:9/1").is_some());
    for bad in [
        "",
        "https://o1.ingest.sentry.io/42",
        "http://abc@sentry.io/42",
        "https://abc:secret@sentry.io/42",
        "https://abc@sentry.io/42/extra",
        "https://abc@sentry.io/x",
        "https://abc@sentry.io/42?x=1",
        "https://a b@sentry.io/42",
    ] {
        assert!(Dsn::parse(bad).is_none(), "{bad:?}");
    }
}

#[test]
fn paths_become_crate_relative() {
    for (path, want) in [
        (
            "/Users/gauri/work/dictation/crates/app/src/x.rs",
            "crates/app/src/x.rs",
        ),
        (
            "/Users/gauri/work/dictation/src-tauri/src/lib.rs",
            "src-tauri/src/lib.rs",
        ),
        ("crates/core/src/a.rs", "crates/core/src/a.rs"),
        (
            "/Users/gauri/.cargo/registry/src/index.crates.io-1949/tokio-1.5.0/src/rt.rs",
            "tokio-1.5.0/src/rt.rs",
        ),
        (
            "/rustc/abcdef0123/library/std/src/panic.rs",
            "library/std/src/panic.rs",
        ),
        ("/Users/gauri/Documents/secret-project/main.rs", "main.rs"),
        ("C:\\Users\\gauri\\secret-project\\main.rs", "main.rs"),
    ] {
        assert_eq!(crate_relative(path), want);
    }
}

#[tokio::test]
async fn an_error_report_holds_the_code_and_type_and_no_text() {
    let server = ok_server().await;
    let dir = temp("error");
    let reporter = reporter(Some(&server), &dir, true);
    for error in leaky_errors() {
        reporter.report(&error);
    }
    let got = requests(&server, 2).await;
    assert_eq!(got.len(), 2);
    for request in &got {
        assert_clean(&request.body);
        let text = String::from_utf8_lossy(&request.body).into_owned();
        assert!(text.contains(r#""release":"0.3.0""#), "{text}");
        assert!(text.contains(r#""environment":"production""#), "{text}");
        assert!(text.contains(r#""error_code":"#), "{text}");
        for absent in [
            "server_name",
            "breadcrumbs",
            "\"user\"",
            "\"request\"",
            "contexts",
        ] {
            assert!(!text.contains(absent), "{absent} in {text}");
        }
    }
    let auth = got[0].headers.get("x-sentry-auth").expect("auth");
    assert!(auth.to_str().unwrap_or("").contains("sentry_key=abc123"));
}

#[test]
fn a_panic_report_keeps_places_and_drops_everything_else() {
    let dir = temp("panic");
    let version = Version::new("0.3.0").expect("version");
    let config = Config::new(
        Some("https://k@sentry.io/1"),
        &version,
        "production",
        15,
        &dir,
    );
    let reporter = SentryReporter {
        config,
        client: None,
        runtime: None,
        state: Arc::new(Mutex::new(State {
            enabled: true,
            ..State::default()
        })),
        cancel: watch::channel(0).0,
    };
    // A stack as std prints it, with the user's folders in it.
    let trace = format!(
        "   0: dictation_app::boom\n             at {0}/crates/app/src/x.rs:10:5\n   \
         1: std::rt::lang_start\n             at /rustc/abc/library/std/src/rt.rs:1:1\n   \
         2: main\n             at {0}/main.rs:3:3\n",
        SECRETS[2]
    );
    let file = format!("{}/crates/app/src/x.rs", SECRETS[2]);
    reporter.record_panic(Some((&file, 10)), &trace);
    let body = std::fs::read(dir.join(PENDING_FILE)).expect("a pending report");
    assert_clean(&body);
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("panicked at crates/app/src/x.rs:10"),
        "{text}"
    );
    assert!(text.contains("dictation_app::boom"), "{text}");
    assert!(text.contains(r#""filename":"main.rs""#), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn off_stops_reports_and_on_resumes_them() {
    let server = ok_server().await;
    let dir = temp("switch");
    let reporter = reporter(Some(&server), &dir, true);
    reporter.set_enabled(false);
    assert!(!reporter.is_enabled());
    reporter.report(&DictationError::EmptyTranscript);
    reporter.record_panic(None, "");
    assert!(!dir.join(PENDING_FILE).exists());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "sent while off"
    );
    reporter.set_enabled(true);
    reporter.report(&DictationError::EmptyTranscript);
    assert_eq!(requests(&server, 1).await.len(), 1);
}

#[tokio::test]
async fn off_stops_a_request_under_way_and_deletes_a_waiting_panic_report() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;
    let dir = temp("cancel");
    let reporter = reporter(Some(&server), &dir, true);
    reporter.report(&DictationError::EmptyTranscript);
    assert_eq!(requests(&server, 1).await.len(), 1);
    assert_eq!(reporter.in_flight(), 1);
    reporter.record_panic(None, "");
    assert!(dir.join(PENDING_FILE).exists());
    reporter.set_enabled(false);
    assert!(
        !dir.join(PENDING_FILE).exists(),
        "the waiting report stayed"
    );
    for _ in 0..40 {
        if reporter.in_flight() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(reporter.in_flight(), 0, "the request was not dropped");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_waiting_panic_report_is_sent_at_the_next_launch_once() {
    let server = ok_server().await;
    let dir = temp("launch");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join(PENDING_FILE), b"{}\n{}\n{}\n").expect("write");
    let _reporter = reporter(Some(&server), &dir, true);
    assert_eq!(requests(&server, 1).await.len(), 1);
    assert!(!dir.join(PENDING_FILE).exists());
    // With sharing off at launch, the report is deleted unsent.
    let off_server = ok_server().await;
    std::fs::write(dir.join(PENDING_FILE), b"{}\n{}\n{}\n").expect("write");
    let _off = reporter(Some(&off_server), &dir, false);
    assert!(!dir.join(PENDING_FILE).exists());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        off_server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn without_a_dsn_nothing_is_sent_or_written() {
    let dir = temp("nodsn");
    let reporter = reporter(None, &dir, true);
    reporter.set_enabled(true);
    reporter.report(&DictationError::EmptyTranscript);
    reporter.record_panic(None, "");
    assert!(!reporter.is_enabled());
    assert!(!dir.exists());
    let disabled = SentryReporter::disabled();
    disabled.report(&DictationError::EmptyTranscript);
    assert!(!disabled.is_enabled());
}

#[tokio::test]
async fn reports_are_capped_per_run() {
    let server = ok_server().await;
    let dir = temp("cap");
    let reporter = reporter(Some(&server), &dir, true);
    for _ in 0..(MAX_PER_RUN + 20) {
        reporter.report(&DictationError::EmptyTranscript);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let sent = server.received_requests().await.unwrap_or_default().len();
    assert!(sent <= MAX_PER_RUN, "{sent}");
}
