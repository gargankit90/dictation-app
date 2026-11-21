//! Connect OpenRouter (Q3): sign in at `OpenRouter` in the browser and keep
//! the key it makes in the Keychain, instead of making the user copy one.
//!
//! The flow itself is `dictation_transcribe::connect`. This file runs it on
//! behalf of the window: [`connect_openrouter`] returns when the key is
//! saved, the attempt fails, times out or is cancelled with
//! [`cancel_connect`]; a second attempt replaces the first. The window
//! never sees the key. [`saved_key_hint`] gives the masked form Settings
//! shows.

use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};

use dictation_core::settings::Provider;
use dictation_pipeline::config::Config;
use dictation_ports::{Opener, SecretStore};
use dictation_transcribe::connect::{self, ConnectError};
use tauri::State;
use tokio::sync::oneshot;

use crate::commands::{AppConfig, save_key};
use crate::diagnostics::AppOpener;
use crate::engine::LiveEngine;
use crate::usage::{self, Usage};

/// The attempt in progress, if any: the way to cancel it.
#[derive(Default)]
pub struct AppConnect(Mutex<Option<oneshot::Sender<()>>>);

/// Signs in at `OpenRouter` and saves the key. Returns when it is saved, or
/// with the reason it isn't. A cancelled attempt is an error too; the window
/// that cancelled it knows.
#[tauri::command]
#[specta::specta]
pub async fn connect_openrouter(
    state: State<'_, AppConfig>,
    engine: State<'_, LiveEngine>,
    opener: State<'_, AppOpener>,
    usage: State<'_, Usage>,
    attempt: State<'_, AppConnect>,
) -> Result<(), String> {
    let (cancel, cancelled) = oneshot::channel::<()>();
    // A newer attempt replaces the older one, which ends as cancelled.
    *attempt.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(cancel);
    let (connect_config, browser) = plan(engine.inner(), &opener.0);
    connect_and_save(
        &state.0,
        engine.inner(),
        &connect_config,
        browser.as_ref(),
        async {
            let _ = cancelled.await;
        },
    )
    .await?;
    usage.record(usage::engine_selected(Provider::OpenRouter));
    Ok(())
}

/// Ends the attempt in progress, if there is one.
#[tauri::command]
#[specta::specta]
pub fn cancel_connect(attempt: State<'_, AppConnect>) -> Result<(), String> {
    let cancel = attempt
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    if let Some(cancel) = cancel {
        let _ = cancel.send(());
    }
    Ok(())
}

/// Where to sign in and what plays the browser. Normally `OpenRouter` and
/// the system browser. A test build pointed at a fake provider
/// (`--debug-provider-url <base>`, loopback `http` only) signs in at
/// `<base>/auth` and exchanges at `<base>/auth/keys`, and "opens the
/// browser" by requesting the sign-in address itself, so the fake (which
/// reads `callback_url` and `state` from it) can call the callback back.
fn plan(live: &LiveEngine, opener: &Arc<dyn Opener>) -> (connect::Config, Arc<dyn Opener>) {
    let base = live.provider_base_url(Provider::OpenRouter);
    if base == dictation_transcribe::openrouter::BASE_URL {
        return (connect::Config::openrouter(), Arc::clone(opener));
    }
    let config = connect::Config {
        auth_url: format!("{base}/auth"),
        exchange_url: format!("{base}/auth/keys"),
        ..connect::Config::openrouter()
    };
    (config, Arc::new(FakeBrowser))
}

/// The debug-only browser: one plain `GET` of the sign-in address.
struct FakeBrowser;

impl Opener for FakeBrowser {
    fn open_url(&self, url: &str) -> Result<(), String> {
        use std::io::Write as _;
        let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
        let host = parsed.host_str().ok_or("no host")?.to_owned();
        let port = parsed.port_or_known_default().ok_or("no port")?;
        let target = match parsed.query() {
            Some(query) => format!("{}?{query}", parsed.path()),
            None => parsed.path().to_owned(),
        };
        std::thread::spawn(move || {
            if let Ok(mut stream) = std::net::TcpStream::connect((host.as_str(), port)) {
                let _ = write!(
                    stream,
                    "GET {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
                );
                let _ = std::io::Read::read_to_end(&mut stream, &mut Vec::new());
            }
        });
        Ok(())
    }

    fn reveal_folder(&self, _dir: &std::path::Path) -> Result<(), String> {
        Err("no file manager in a test build".to_owned())
    }
}

/// The sign-in and the save, with the pieces a test replaces.
async fn connect_and_save<S: SecretStore + 'static>(
    config: &Arc<Config<S>>,
    live: &LiveEngine,
    connect_config: &connect::Config,
    opener: &dyn Opener,
    cancel: impl Future<Output = ()>,
) -> Result<(), String> {
    let key = connect::connect(connect_config, |url| opener.open_url(url), cancel)
        .await
        .map_err(|error: ConnectError| {
            tracing::warn!(%error, "connecting OpenRouter failed");
            error.to_string()
        })?;
    save_key(config, live, Provider::OpenRouter, key.expose().to_owned()).await
}

/// The saved key as Settings shows it: `sk-or-…` and its last four
/// characters. `None` if no key is saved. Never the whole key.
#[tauri::command]
#[specta::specta]
pub async fn saved_key_hint(
    state: State<'_, AppConfig>,
    engine: State<'_, LiveEngine>,
    provider: Provider,
) -> Result<Option<String>, String> {
    let (config, live) = (Arc::clone(&state.0), engine.inner().clone());
    tauri::async_runtime::spawn_blocking(move || live.read_key(&config, provider))
        .await
        .map_err(|_| "the settings task crashed".to_owned())?
        .map(|key| key.map(|key| mask(key.expose())))
        .map_err(|error| {
            tracing::warn!(%error, "reading the saved key failed");
            dictation_pipeline::config::ConfigError::Secret(error).to_string()
        })
}

/// `sk-or-…1234`: the family prefix, if it has one, and the last four.
fn mask(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    // A short key shows nothing of itself.
    if chars.len() < 16 {
        return "…".to_owned();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    let prefix = if key.starts_with("sk-or-") {
        "sk-or-"
    } else {
        ""
    };
    format!("{prefix}…{tail}")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests fail on purpose")]
mod tests {
    use std::time::Duration;

    use dictation_platform::fakes::{FakeOpener, MemSecretStore};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// Obviously fake.
    const KEY: &str = "sk-or-v1-NOT-REAL-q3-0123456789abcdef";

    #[test]
    fn the_hint_shows_the_prefix_and_four_characters_only() {
        let hint = mask(KEY);
        assert_eq!(hint, "sk-or-…cdef");
        assert!(!hint.contains("NOT-REAL"));
        assert_eq!(mask("short"), "…");
        assert_eq!(mask("0123456789abcdefghij"), "…ghij");
    }

    fn temp() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("q3-connect-{}", std::process::id()))
    }

    /// An opener that plays the browser: on `open_url` it calls the
    /// callback the address names, from a thread of its own.
    struct Browser {
        inner: FakeOpener,
        state_ok: bool,
    }

    impl Opener for Browser {
        fn open_url(&self, url: &str) -> Result<(), String> {
            self.inner.open_url(url)?;
            let parsed = reqwest::Url::parse(url).unwrap();
            let get = |name: &str| {
                parsed
                    .query_pairs()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.into_owned())
                    .unwrap()
            };
            let callback = get("callback_url");
            let state = if self.state_ok {
                get("state")
            } else {
                "wrong".into()
            };
            let port = callback
                .strip_prefix("http://localhost:")
                .and_then(|rest| rest.strip_suffix("/callback"))
                .unwrap()
                .to_owned();
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut stream = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
                write!(
                    stream,
                    "GET /callback?code=abc&state={state} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                let mut sink = String::new();
                let _ = stream.read_to_string(&mut sink);
            });
            Ok(())
        }

        fn reveal_folder(&self, dir: &std::path::Path) -> Result<(), String> {
            self.inner.reveal_folder(dir)
        }
    }

    async fn exchange(response: ResponseTemplate) -> (MockServer, connect::Config) {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/keys"))
            .respond_with(response)
            .mount(&server)
            .await;
        let config = connect::Config {
            auth_url: "https://openrouter.test/auth".into(),
            exchange_url: format!("{}/keys", server.uri()),
            timeout: Duration::from_secs(20),
        };
        (server, config)
    }

    /// Criterion 2: a good sign-in stores the key in the Keychain.
    #[tokio::test]
    async fn a_good_sign_in_saves_the_key() {
        let (_server, connect_config) =
            exchange(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "key": KEY })))
                .await;
        let store = MemSecretStore::new();
        let config = Arc::new(Config::new(temp(), store.clone()));
        let browser = Browser {
            inner: FakeOpener::new(),
            state_ok: true,
        };
        connect_and_save(
            &config,
            &LiveEngine::fixed(),
            &connect_config,
            &browser,
            std::future::pending(),
        )
        .await
        .unwrap();
        let saved = store.get(Provider::OpenRouter.key_name()).unwrap().unwrap();
        assert_eq!(saved.expose(), KEY);
    }

    /// Criterion 2: failures are clear, and nothing is saved.
    #[tokio::test]
    async fn a_refused_code_saves_nothing_and_says_so() {
        let (_server, connect_config) = exchange(ResponseTemplate::new(403)).await;
        let store = MemSecretStore::new();
        let config = Arc::new(Config::new(temp(), store.clone()));
        let browser = Browser {
            inner: FakeOpener::new(),
            state_ok: true,
        };
        let error = connect_and_save(
            &config,
            &LiveEngine::fixed(),
            &connect_config,
            &browser,
            std::future::pending(),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "OpenRouter didn't accept the sign-in. Try again.");
        assert!(
            store
                .get(Provider::OpenRouter.key_name())
                .unwrap()
                .is_none()
        );
    }

    /// A test build's fake provider: `<base>/auth` gets the sign-in request
    /// (from the debug browser) and calls the callback back; `<base>/auth/keys`
    /// answers the exchange.
    struct FakeAuth;

    impl wiremock::Respond for FakeAuth {
        fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
            let get = |name: &str| {
                request
                    .url
                    .query_pairs()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.into_owned())
                    .unwrap()
            };
            let callback = reqwest::Url::parse(&get("callback_url")).unwrap();
            let target = format!("127.0.0.1:{}", callback.port().unwrap());
            let state = get("state");
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut stream = std::net::TcpStream::connect(target).unwrap();
                write!(
                    stream,
                    "GET /callback?code=fake-code&state={state} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                let _ = stream.read_to_string(&mut String::new());
            });
            ResponseTemplate::new(200)
        }
    }

    async fn fake_provider() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/auth"))
            .respond_with(FakeAuth)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/auth/keys"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "key": KEY })),
            )
            .mount(&server)
            .await;
        server
    }

    #[test]
    fn a_real_provider_signs_in_at_openrouter_with_the_system_browser() {
        let opener: Arc<dyn Opener> = Arc::new(FakeOpener::new());
        let (config, _) = plan(&LiveEngine::fixed(), &opener);
        assert_eq!(config.auth_url, "https://openrouter.ai/auth");
        assert_eq!(
            config.exchange_url,
            "https://openrouter.ai/api/v1/auth/keys"
        );
    }

    /// Q3 e2e hook: with `--debug-provider-url`, Connect goes through the
    /// fake provider alone and saves its key, with the real opener never used.
    #[tokio::test]
    async fn a_test_build_connects_through_the_fake_provider() {
        let server = fake_provider().await;
        let opener = FakeOpener::new();
        let shared: Arc<dyn Opener> = Arc::new(opener.clone());
        let live = LiveEngine::fixed().at(&server.uri());
        let (connect_config, browser) = plan(&live, &shared);
        assert_eq!(connect_config.auth_url, format!("{}/auth", server.uri()));
        let store = MemSecretStore::new();
        let config = Arc::new(Config::new(temp(), store.clone()));
        connect_and_save(
            &config,
            &live,
            &connect_config,
            browser.as_ref(),
            std::future::pending(),
        )
        .await
        .unwrap();
        let saved = store.get(Provider::OpenRouter.key_name()).unwrap().unwrap();
        assert_eq!(saved.expose(), KEY);
        assert!(opener.urls().is_empty(), "the system browser was used");
    }

    /// Q3 privacy: the key, the code and the verifier reach no log line, on
    /// success or failure.
    #[tokio::test]
    async fn the_connect_path_never_logs_the_key_the_code_or_the_verifier() {
        use std::sync::Mutex;
        use tracing_subscriber::fmt::MakeWriter;

        #[derive(Clone, Default)]
        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Sink {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> MakeWriter<'a> for Sink {
            type Writer = Sink;
            fn make_writer(&'a self) -> Sink {
                self.clone()
            }
        }

        let sink = Sink::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(sink.clone())
            .with_max_level(tracing::Level::TRACE)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);
        // Under `cargo test` threads, other tests ran these log calls with no
        // subscriber and cached "off"; the gate's nextest runs one test per
        // process, where this is exact.
        tracing::callsite::rebuild_interest_cache();

        // Success.
        let server = fake_provider().await;
        let live = LiveEngine::fixed().at(&server.uri());
        let shared: Arc<dyn Opener> = Arc::new(FakeOpener::new());
        let (connect_config, browser) = plan(&live, &shared);
        let config = Arc::new(Config::new(temp(), MemSecretStore::new()));
        connect_and_save(
            &config,
            &live,
            &connect_config,
            browser.as_ref(),
            std::future::pending(),
        )
        .await
        .unwrap();
        // A refused exchange, which does log a warning.
        let (_refusing, refused_config) = exchange(ResponseTemplate::new(403)).await;
        let browser = Browser {
            inner: FakeOpener::new(),
            state_ok: true,
        };
        connect_and_save(
            &config,
            &LiveEngine::fixed(),
            &refused_config,
            &browser,
            std::future::pending(),
        )
        .await
        .unwrap_err();

        let logged = String::from_utf8(sink.0.lock().unwrap().clone()).unwrap();
        assert!(logged.contains("connecting OpenRouter failed"), "{logged}");
        for secret in [KEY, "NOT-REAL-q3", "fake-code", "code_verifier"] {
            assert!(!logged.contains(secret), "{secret} in the log: {logged}");
        }
        // The one-time challenge and state never appear either.
        assert!(!logged.contains("code_challenge"), "{logged}");
    }

    #[tokio::test]
    async fn cancelling_saves_nothing() {
        let (_server, connect_config) = exchange(ResponseTemplate::new(500)).await;
        let store = MemSecretStore::new();
        let config = Arc::new(Config::new(temp(), store.clone()));
        let error = connect_and_save(
            &config,
            &LiveEngine::fixed(),
            &connect_config,
            &FakeOpener::new(),
            std::future::ready(()),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "Connecting was cancelled.");
        assert!(
            store
                .get(Provider::OpenRouter.key_name())
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_wrong_state_is_not_exchanged() {
        let (server, mut connect_config) = exchange(ResponseTemplate::new(500)).await;
        connect_config.timeout = Duration::from_millis(500);
        let config = Arc::new(Config::new(temp(), MemSecretStore::new()));
        let browser = Browser {
            inner: FakeOpener::new(),
            state_ok: false,
        };
        let error = connect_and_save(
            &config,
            &LiveEngine::fixed(),
            &connect_config,
            &browser,
            std::future::pending(),
        )
        .await
        .unwrap_err();
        assert_eq!(error, "Connecting took too long. Try again.");
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
