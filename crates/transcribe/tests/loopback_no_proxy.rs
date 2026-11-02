//! Review T4a-round1 R1: a test build's loopback provider
//! (`--debug-provider-url`) is reached directly, never through an
//! environment proxy, because the request carries the key and the audio.
//! Both dictation (`transcribe`) and Settings' Test key (`check_key`) are
//! covered. No real key: the key is fake.
//!
//! The proxy variables are set on a child process (this test binary,
//! re-run for the one child test), so no test changes its own process's
//! environment and the check holds under `cargo test` threads as well as
//! nextest processes.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::io::Read;
use std::net::TcpListener;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dictation_ports::{Audio, Secret, Transcriber};
use dictation_transcribe::check_key::check_key;
use dictation_transcribe::openai_compat::{Config, GROQ, OpenAiCompat};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "test-key-NOT-REAL-proxy-7a8b9c";
/// Set on the child: run the requests instead of skipping.
const CHILD: &str = "DICTATION_TEST_PROXY_CHILD";
const PROXY_VARS: [&str; 6] = [
    "HTTP_PROXY",
    "http_proxy",
    "ALL_PROXY",
    "all_proxy",
    "HTTPS_PROXY",
    "https_proxy",
];

#[test]
fn loopback_override_requests_bypass_an_environment_proxy() {
    // A capturing proxy: counts every connection and drops it.
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_url = format!("http://{}", proxy.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    std::thread::spawn(move || {
        for stream in proxy.incoming() {
            counter.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut stream) = stream {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                let _ = stream.read(&mut [0; 1024]);
            }
        }
    });

    let mut child = Command::new(std::env::current_exe().unwrap());
    child.args([
        "child_sends_through_the_loopback_override",
        "--exact",
        "--nocapture",
    ]);
    for var in ["NO_PROXY", "no_proxy"] {
        child.env_remove(var);
    }
    for var in PROXY_VARS {
        child.env(var, &proxy_url);
    }
    let output = child.env(CHILD, "1").output().unwrap();

    assert!(
        output.status.success(),
        "child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "the child test didn't run"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 0, "the proxy got a request");
}

/// Runs only as the child of the test above, with the proxy variables set:
/// dictation and Test key both reach the local provider.
#[tokio::test]
async fn child_sends_through_the_loopback_override() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": "local"})),
        )
        .mount(&provider)
        .await;
    let mut config = Config::from_preset(GROQ);
    // MockServer::uri() is http://127.0.0.1:<port>, a loopback override.
    config.base_url = format!("{}/v1", provider.uri());
    config.timeout = Duration::from_secs(5);

    let engine = OpenAiCompat::new(config.clone(), Secret::new(KEY)).unwrap();
    let audio = Audio {
        sample_rate: 16_000,
        samples: vec![0.0; 1_600],
    };
    let transcript = engine.transcribe(&audio).await;
    assert!(transcript.is_ok(), "dictation: {transcript:?}");
    assert_eq!(provider.received_requests().await.unwrap().len(), 1);

    let checked = check_key(config, Secret::new(KEY)).await;
    assert_eq!(checked, Ok(()), "Test key");
    assert_eq!(provider.received_requests().await.unwrap().len(), 2);
}
