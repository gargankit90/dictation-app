//! [`connect`]: get an `OpenRouter` API key by signing in at `OpenRouter`
//! (OAuth with PKCE, Q3), instead of making the user copy one.
//!
//! The steps, per <https://openrouter.ai/docs/use-cases/oauth-pkce>:
//!
//! 1. a random `code_verifier` (kept in memory only) and its S256
//!    `code_challenge`, and a random `state`;
//! 2. a one-shot HTTP listener on `127.0.0.1` and, where the machine has it,
//!    `[::1]`, on one free port (`localhost`, the host `OpenRouter` documents
//!    for callbacks, may resolve to either);
//! 3. the caller opens the sign-in address in the browser;
//! 4. the browser comes back to `http://localhost:<port>/callback?code=…&state=…`;
//!    only a request with this attempt's `state` counts, anything else gets
//!    an error page and the wait goes on;
//! 5. the code and the verifier are exchanged for the key, and the browser
//!    gets a "you can close this tab" page.
//!
//! The listener serves one purpose and lives for one attempt: it closes when
//! the attempt ends, by success, failure, timeout or cancellation. No Vocet
//! user data is sent: the sign-in address carries the challenge, the
//! callback and a fixed key label, nothing else. The key never goes into an
//! error or a log.

use std::future::Future;
use std::time::Duration;

use dictation_core::encoding::{base64url, percent_encode};
use dictation_ports::Secret;
use reqwest::header::{CONTENT_TYPE, HeaderValue};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::http;

/// How long to wait for the browser to come back.
pub const TIMEOUT: Duration = Duration::from_secs(300);

/// How long one connection to the listener may take to send its request.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The most of a request the listener reads: the request line and headers
/// of a callback are far smaller.
const MAX_REQUEST: usize = 8 * 1024;

/// Where `OpenRouter` signs people in.
const AUTH_URL: &str = "https://openrouter.ai/auth";

/// Where a code becomes a key.
const EXCHANGE_URL: &str = "https://openrouter.ai/api/v1/auth/keys";

/// The label the new key gets on `OpenRouter`'s key list. Fixed: no host
/// name, no user name.
const KEY_LABEL: &str = "Vocet on this Mac";

/// The callback's path.
const CALLBACK_PATH: &str = "/callback";

/// Where the attempt talks to; tests point it at a local fake.
#[derive(Debug, Clone)]
pub struct Config {
    /// The sign-in page the browser opens.
    pub auth_url: String,
    /// The endpoint that exchanges the code for a key.
    pub exchange_url: String,
    /// How long to wait for the browser, in total.
    pub timeout: Duration,
}

impl Config {
    /// `OpenRouter`'s real addresses and a five minute wait.
    #[must_use]
    pub fn openrouter() -> Self {
        Self {
            auth_url: AUTH_URL.to_owned(),
            exchange_url: EXCHANGE_URL.to_owned(),
            timeout: TIMEOUT,
        }
    }
}

/// Why connecting failed. Every message is fixed text: none holds the key,
/// the code or the verifier.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConnectError {
    /// The browser didn't come back in time.
    #[error("Connecting took too long. Try again.")]
    TimedOut,
    /// The user cancelled.
    #[error("Connecting was cancelled.")]
    Cancelled,
    /// The browser couldn't be opened, or the local listener couldn't start.
    #[error("Couldn't start connecting: {0}")]
    Start(String),
    /// `OpenRouter` refused the code (expired, or not this attempt's).
    #[error("OpenRouter didn't accept the sign-in. Try again.")]
    Rejected,
    /// `OpenRouter` couldn't be reached.
    #[error("Couldn't reach OpenRouter. Check your internet connection.")]
    Network,
    /// `OpenRouter` failed, or its answer had no key.
    #[error("OpenRouter gave an answer Vocet can't use. Try again, or paste a key instead.")]
    Server,
}

/// A PKCE pair.
struct Pkce {
    verifier: String,
    challenge: String,
}

impl Pkce {
    fn new() -> Result<Self, ConnectError> {
        // 32 random bytes are 43 base64url characters, the shortest verifier
        // RFC 7636 allows.
        let verifier = base64url(&random::<32>()?);
        let challenge = challenge_of(&verifier);
        Ok(Self {
            verifier,
            challenge,
        })
    }
}

/// `N` random bytes from the operating system.
fn random<const N: usize>() -> Result<[u8; N], ConnectError> {
    let mut bytes = [0_u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|_| ConnectError::Start("no source of randomness".to_owned()))?;
    Ok(bytes)
}

/// The S256 `code_challenge` of `verifier`: the base64url of its SHA-256.
fn challenge_of(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

/// Signs in at `OpenRouter` and returns the key it makes.
///
/// `open` is called once with the sign-in address, after the listener is
/// ready. `cancel` ends the attempt when it completes.
///
/// # Errors
///
/// A [`ConnectError`]; see its variants.
pub async fn connect(
    config: &Config,
    open: impl FnOnce(&str) -> Result<(), String>,
    cancel: impl Future<Output = ()>,
) -> Result<Secret, ConnectError> {
    let pkce = Pkce::new()?;
    let state = base64url(&random::<16>()?);
    let listeners = bind_loopback().await?;
    let port = listeners
        .first()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
        .ok_or_else(|| ConnectError::Start("no listener".to_owned()))?;
    let callback = format!("http://localhost:{port}{CALLBACK_PATH}");
    let url = format!(
        "{}?callback_url={}&code_challenge={}&code_challenge_method=S256&state={}&key_label={}",
        config.auth_url,
        percent_encode(&callback),
        pkce.challenge,
        state,
        percent_encode(KEY_LABEL),
    );
    open(&url).map_err(ConnectError::Start)?;
    tokio::select! {
        outcome = tokio::time::timeout(config.timeout, wait(&listeners, &state, config, &pkce)) => {
            outcome.unwrap_or(Err(ConnectError::TimedOut))
        }
        () = cancel => Err(ConnectError::Cancelled),
    }
}

/// Binds `127.0.0.1` on a free port, then `[::1]` on the same port. Without
/// IPv6 (or with that port taken there) the IPv4 listener serves alone;
/// neither is ever reachable from another machine.
async fn bind_loopback() -> Result<Vec<TcpListener>, ConnectError> {
    let v4 = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| ConnectError::Start(e.to_string()))?;
    let port = v4
        .local_addr()
        .map_err(|e| ConnectError::Start(e.to_string()))?
        .port();
    let mut listeners = vec![v4];
    if let Ok(v6) = TcpListener::bind(("::1", port)).await {
        listeners.push(v6);
    }
    Ok(listeners)
}

/// The next connection on any of `listeners` (one or two).
async fn accept_any(listeners: &[TcpListener]) -> Option<TcpStream> {
    let accepted = match listeners {
        [one] => one.accept().await,
        [first, second] => {
            tokio::select! {
                result = first.accept() => result,
                result = second.accept() => result,
            }
        }
        _ => return None,
    };
    accepted.ok().map(|(stream, _)| stream)
}

/// Accepts connections until one is the callback of this attempt, then
/// exchanges its code.
async fn wait(
    listeners: &[TcpListener],
    state: &str,
    config: &Config,
    pkce: &Pkce,
) -> Result<Secret, ConnectError> {
    loop {
        let Some(mut stream) = accept_any(listeners).await else {
            // A failed accept (out of descriptors, say): don't spin.
            tokio::time::sleep(Duration::from_millis(50)).await;
            continue;
        };
        let Some(code) = tokio::time::timeout(READ_TIMEOUT, read_callback(&mut stream, state))
            .await
            .ok()
            .flatten()
        else {
            // Not ours (a stray request, a wrong state): say so, keep waiting.
            respond(&mut stream, "400 Bad Request", NOT_THIS_ATTEMPT).await;
            continue;
        };
        let result = exchange(config, &code, &pkce.verifier).await;
        match &result {
            Ok(_) => respond(&mut stream, "200 OK", CONNECTED_PAGE).await,
            Err(_) => respond(&mut stream, "502 Bad Gateway", FAILED_PAGE).await,
        }
        return result;
    }
}

/// Reads one request and returns its code if it is this attempt's callback:
/// `GET /callback` with the right `state` and a `code`.
async fn read_callback(stream: &mut TcpStream, state: &str) -> Option<String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !buffer.windows(4).any(|w| w == b"\r\n\r\n") && buffer.len() < MAX_REQUEST {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(chunk.get(..read)?);
    }
    let head = String::from_utf8_lossy(&buffer);
    let line = head.lines().next()?;
    let mut parts = line.split(' ');
    if parts.next()? != "GET" {
        return None;
    }
    let target = parts.next()?;
    let (path, query) = target.split_once('?')?;
    if path != CALLBACK_PATH {
        return None;
    }
    let mut code = None;
    let mut got_state = None;
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("code", value)) => code = Some(value),
            Some(("state", value)) => got_state = Some(value),
            _ => {}
        }
    }
    // Base64url state and codes need no percent-decoding; anything with a
    // `%` is not this attempt's.
    if got_state? != state {
        return None;
    }
    let code = code.filter(|c| !c.is_empty() && !c.contains('%'))?;
    Some(code.to_owned())
}

/// Trades `code` and `verifier` for the key.
async fn exchange(config: &Config, code: &str, verifier: &str) -> Result<Secret, ConnectError> {
    let client = http::client(&config.exchange_url).map_err(|_| ConnectError::Network)?;
    let body = serde_json::json!({
        "code": code,
        "code_verifier": verifier,
        "code_challenge_method": "S256",
    });
    let response = client
        .post(&config.exchange_url)
        .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
        .body(body.to_string())
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| ConnectError::Network)?;
    let status = response.status();
    if status.is_client_error() {
        return Err(ConnectError::Rejected);
    }
    if !status.is_success() {
        return Err(ConnectError::Server);
    }
    let text = response.text().await.map_err(|_| ConnectError::Network)?;
    let reply: serde_json::Value = serde_json::from_str(&text).map_err(|_| ConnectError::Server)?;
    reply
        .get("key")
        .and_then(serde_json::Value::as_str)
        .filter(|key| !key.trim().is_empty())
        .map(|key| Secret::new(key.trim()))
        .ok_or(ConnectError::Server)
}

/// Writes a small HTML reply and closes. A failed write changes nothing: the
/// outcome is already decided.
async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let reply = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(reply.as_bytes()).await;
    let _ = stream.shutdown().await;
}

const NOT_THIS_ATTEMPT: &str = "<!doctype html><meta charset=utf-8><title>Vocet</title>\
    <body style=\"font:16px -apple-system,sans-serif;text-align:center;margin-top:20vh\">\
    This isn't the sign-in Vocet is waiting for.</body>";
const CONNECTED_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Vocet</title>\
    <body style=\"font:16px -apple-system,sans-serif;text-align:center;margin-top:20vh\">\
    Connected. You can close this tab.</body>";
const FAILED_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Vocet</title>\
    <body style=\"font:16px -apple-system,sans-serif;text-align:center;margin-top:20vh\">\
    Couldn't connect. Go back to Vocet and try again.</body>";

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests fail on purpose")]
mod tests {
    use tokio::sync::oneshot;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    /// Obviously fake.
    const KEY: &str = "sk-or-v1-NOT-REAL-connect-4d5e6f";

    // The RFC 7636 appendix B vector.
    #[test]
    fn the_challenge_is_the_s256_of_the_verifier() {
        assert_eq!(
            challenge_of("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_verifier_is_43_url_safe_characters_and_never_repeats() {
        let a = Pkce::new().unwrap();
        let b = Pkce::new().unwrap();
        assert_eq!(a.verifier.len(), 43);
        assert!(
            a.verifier
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        );
        assert_ne!(a.verifier, b.verifier);
        assert_eq!(a.challenge, challenge_of(&a.verifier));
        assert_eq!(a.challenge.len(), 43);
    }

    fn config(exchange_url: String, timeout: Duration) -> Config {
        Config {
            auth_url: "https://openrouter.test/auth".to_owned(),
            exchange_url,
            timeout,
        }
    }

    /// Reads `name` from the sign-in address's query.
    fn query_value(url: &str, name: &str) -> String {
        let parsed = reqwest::Url::parse(url).unwrap();
        parsed
            .query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
            .unwrap()
    }

    /// Runs an attempt; hands the sign-in address to `browser` (which plays
    /// the user's browser) and returns the attempt's result.
    async fn run<F, Fut>(
        config: Config,
        browser: F,
    ) -> (Result<Secret, ConnectError>, Option<String>)
    where
        F: FnOnce(String) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel::<String>();
        let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen_in = std::sync::Arc::clone(&seen);
        let task = tokio::spawn(async move {
            let url = rx.await.unwrap();
            *seen_in.lock().unwrap() = Some(url.clone());
            browser(url).await;
        });
        let result = connect(
            &config,
            |url| tx.send(url.to_owned()).map_err(|_| "gone".to_owned()),
            async {
                let _ = cancel_rx.await;
            },
        )
        .await;
        drop(cancel_tx);
        let _ = task.await;
        let url = seen.lock().unwrap().clone();
        (result, url)
    }

    /// A plain GET to `127.0.0.1:<port><path>?<query>`: status and body.
    async fn get(base: &str, query: &str) -> (u16, String) {
        let (host, path) = base
            .strip_prefix("http://")
            .unwrap()
            .split_once('/')
            .unwrap();
        let mut stream = TcpStream::connect(host).await.unwrap();
        let request =
            format!("GET /{path}?{query} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).await.unwrap();
        let status = reply.split(' ').nth(1).unwrap().parse().unwrap();
        (status, reply)
    }

    async fn exchange_server(response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/keys"))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn a_valid_callback_exchanges_the_code_and_returns_the_key() {
        let server = MockServer::start().await;
        // The verifier is random, so the expectation is checked from the
        // request afterwards.
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/keys"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "key": KEY })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let config = config(
            format!("{}/api/v1/auth/keys", server.uri()),
            Duration::from_secs(20),
        );
        let (result, url) = run(config, |url| async move {
            let callback = query_value(&url, "callback_url");
            let state = query_value(&url, "state");
            let port = callback
                .strip_prefix("http://localhost:")
                .and_then(|rest| rest.strip_suffix("/callback"))
                .unwrap()
                .to_owned();
            let (status, body) = get(
                &format!("http://127.0.0.1:{port}/callback"),
                &format!("code=abc123&state={state}"),
            )
            .await;
            assert_eq!(status, 200);
            assert!(body.contains("You can close this tab"));
        })
        .await;
        assert_eq!(result.unwrap().expose(), KEY);
        let url = url.unwrap();
        assert!(
            url.starts_with("https://openrouter.test/auth?callback_url=http%3A%2F%2Flocalhost%3A")
        );
        assert_eq!(query_value(&url, "code_challenge_method"), "S256");
        assert_eq!(query_value(&url, "key_label"), "Vocet on this Mac");
        // The exchange sent this attempt's verifier, whose challenge the
        // sign-in address carried.
        let requests = server.received_requests().await.unwrap();
        let sent: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(sent["code"], "abc123");
        assert_eq!(sent["code_challenge_method"], "S256");
        assert_eq!(
            challenge_of(sent["code_verifier"].as_str().unwrap()),
            query_value(&url, "code_challenge")
        );
    }

    #[tokio::test]
    async fn only_this_attempts_callback_counts() {
        let server = exchange_server(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "key": KEY })),
        )
        .await;
        let config = config(
            format!("{}/api/v1/auth/keys", server.uri()),
            Duration::from_secs(20),
        );
        let (result, _) = run(config, |url| async move {
            let callback = query_value(&url, "callback_url");
            let state = query_value(&url, "state");
            let port = callback
                .strip_prefix("http://localhost:")
                .and_then(|rest| rest.strip_suffix("/callback"))
                .unwrap()
                .to_owned();
            let base = format!("http://127.0.0.1:{port}");
            // A wrong state, no state, no code, another path: all refused,
            // and the listener still waits.
            for (path, query) in [
                ("/callback", "code=evil&state=wrong".to_owned()),
                ("/callback", "code=evil".to_owned()),
                ("/callback", format!("state={state}")),
                ("/other", format!("code=evil&state={state}")),
            ] {
                let (status, _) = get(&format!("{base}{path}"), &query).await;
                assert_eq!(status, 400, "{path}?{query}");
            }
            let (status, _) = get(
                &format!("{base}/callback"),
                &format!("code=good&state={state}"),
            )
            .await;
            assert_eq!(status, 200);
        })
        .await;
        assert_eq!(result.unwrap().expose(), KEY);
        // Only the good code was ever exchanged.
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let sent: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(sent["code"], "good");
    }

    #[tokio::test]
    async fn the_callback_address_is_localhost_on_a_free_port() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen_in = std::sync::Arc::clone(&seen);
        let _ = connect(
            &config("http://127.0.0.1:1/x".into(), Duration::from_millis(50)),
            |url| {
                *seen_in.lock().unwrap() = Some(query_value(url, "callback_url"));
                Ok(())
            },
            std::future::pending::<()>(),
        )
        .await;
        // The callback names `localhost`, which `OpenRouter` accepts; the
        // socket itself is on 127.0.0.1 only (the test above reaches it there).
        let callback = seen.lock().unwrap().clone().unwrap();
        assert!(callback.starts_with("http://localhost:"));
    }

    /// `localhost` may resolve to `::1`: where the machine has IPv6, the
    /// callback works there too, on the same port.
    #[tokio::test]
    async fn the_callback_also_works_on_ipv6_loopback_where_there_is_one() {
        let listeners = bind_loopback().await.unwrap();
        let port = listeners[0].local_addr().unwrap().port();
        assert!(listeners[0].local_addr().unwrap().ip().is_loopback());
        if listeners.len() == 2 {
            assert_eq!(listeners[1].local_addr().unwrap().port(), port);
            assert!(listeners[1].local_addr().unwrap().is_ipv6());
            assert!(listeners[1].local_addr().unwrap().ip().is_loopback());
            let client = tokio::spawn(async move {
                let mut stream = TcpStream::connect(("::1", port)).await.unwrap();
                stream.write_all(b"GET /x HTTP/1.1\r\n\r\n").await.unwrap();
            });
            assert!(accept_any(&listeners).await.is_some());
            client.await.unwrap();
        }
    }

    #[tokio::test]
    async fn it_times_out_when_the_browser_never_comes_back() {
        let result = connect(
            &config("http://127.0.0.1:1/x".into(), Duration::from_millis(50)),
            |_| Ok(()),
            std::future::pending::<()>(),
        )
        .await;
        assert_eq!(result.unwrap_err(), ConnectError::TimedOut);
    }

    #[tokio::test]
    async fn cancelling_ends_the_wait() {
        let result = connect(
            &config("http://127.0.0.1:1/x".into(), TIMEOUT),
            |_| Ok(()),
            std::future::ready(()),
        )
        .await;
        assert_eq!(result.unwrap_err(), ConnectError::Cancelled);
    }

    #[tokio::test]
    async fn a_browser_that_wont_open_is_an_error() {
        let result = connect(
            &config("http://127.0.0.1:1/x".into(), TIMEOUT),
            |_| Err("no browser".into()),
            std::future::pending::<()>(),
        )
        .await;
        assert!(matches!(result.unwrap_err(), ConnectError::Start(m) if m == "no browser"));
    }

    async fn exchange_with(status: u16, body: &str) -> Result<Secret, ConnectError> {
        let server = exchange_server(ResponseTemplate::new(status).set_body_string(body)).await;
        exchange(
            &config(format!("{}/api/v1/auth/keys", server.uri()), TIMEOUT),
            "code",
            "verifier",
        )
        .await
    }

    #[tokio::test]
    async fn exchange_failures_map_to_clear_errors() {
        for (status, body, want) in [
            (400, "bad", ConnectError::Rejected),
            (403, "expired", ConnectError::Rejected),
            (500, "oops", ConnectError::Server),
            (200, "not json", ConnectError::Server),
            (200, "{\"key\":\"\"}", ConnectError::Server),
            (200, "{}", ConnectError::Server),
        ] {
            assert_eq!(
                exchange_with(status, body).await.unwrap_err(),
                want,
                "{status} {body}"
            );
        }
    }

    #[tokio::test]
    async fn an_unreachable_server_is_a_network_error() {
        let result = exchange(
            &config("http://127.0.0.1:1/x".into(), TIMEOUT),
            "code",
            "verifier",
        )
        .await;
        assert_eq!(result.unwrap_err(), ConnectError::Network);
    }

    #[tokio::test]
    async fn exchange_posts_the_documented_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/auth/keys"))
            .and(body_json(serde_json::json!({
                "code": "c0de",
                "code_verifier": "v3rifier",
                "code_challenge_method": "S256",
            })))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "key": KEY })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let key = exchange(
            &config(format!("{}/api/v1/auth/keys", server.uri()), TIMEOUT),
            "c0de",
            "v3rifier",
        )
        .await
        .unwrap();
        assert_eq!(key.expose(), KEY);
    }

    #[test]
    fn no_error_message_can_hold_a_key() {
        for error in [
            ConnectError::TimedOut,
            ConnectError::Cancelled,
            ConnectError::Rejected,
            ConnectError::Network,
            ConnectError::Server,
        ] {
            assert!(!error.to_string().contains("sk-"));
        }
    }
}
