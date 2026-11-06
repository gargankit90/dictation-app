//! The HTTP client the cloud engines share.
//!
//! TLS is rustls with the `ring` crypto provider and the OS trust store
//! (`rustls-platform-verifier`), configured here explicitly instead of
//! through reqwest's default (aws-lc, whose licence `deny.toml` doesn't
//! allow) or a process-wide default provider.

use std::sync::Arc;
use std::time::Duration;

use dictation_ports::TranscribeError;
use reqwest::Client;
use rustls_platform_verifier::BuilderVerifierExt;

/// How long a connection may take to open. A dead network fails after this,
/// however long the clip is: the per-request limit (which grows with the
/// clip) only starts to matter once connected. 10 s covers a slow mobile
/// link's DNS, TCP and TLS set-up with room to spare, and is short enough
/// that three attempts at a long clip don't sit for minutes with nothing
/// reachable.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// A client for the API at `base_url`, with [`CONNECT_TIMEOUT`] to connect.
/// It has no limit on a whole request: the caller sets one per request
/// (`RequestBuilder::timeout`), because it depends on the length of the
/// audio.
///
/// A plain `http` loopback `base_url` (a test build's local provider) is
/// reached directly, never through `HTTP_PROXY`/`ALL_PROXY`: the request
/// carries the key and the audio, and a proxy could be anywhere. Every
/// other URL keeps the system proxy settings.
///
/// # Errors
///
/// [`TranscribeError::Engine`] if the TLS setup or the client can't be
/// built.
pub(crate) fn client(base_url: &str) -> Result<Client, TranscribeError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| failed("TLS versions", &e))?
        .with_platform_verifier()
        .map_err(|e| failed("TLS trust store", &e))?
        .with_no_client_auth();
    let mut builder = Client::builder();
    if is_loopback_http(base_url) {
        builder = builder.no_proxy();
    }
    builder
        .tls_backend_preconfigured(tls)
        .connect_timeout(CONNECT_TIMEOUT)
        // Never follow a redirect: requests go only to the configured
        // provider, and a 3xx is a provider error (issue #76).
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| failed("HTTP client", &e))
}

/// The client model downloads use (L1): the same TLS setup, `stall` to
/// connect and between reads (no limit on the whole download), and redirects
/// followed only to `https` URLs, at most `max_redirects` times (Hugging
/// Face answers with a redirect to its CDN). No API key is ever sent with
/// it.
///
/// # Errors
///
/// [`TranscribeError::Engine`] if the TLS setup or the client can't be
/// built.
pub(crate) fn download_client(
    stall: Duration,
    max_redirects: usize,
) -> Result<Client, TranscribeError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| failed("TLS versions", &e))?
        .with_platform_verifier()
        .map_err(|e| failed("TLS trust store", &e))?
        .with_no_client_auth();
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= max_redirects || attempt.url().scheme() != "https" {
            attempt.error("refused redirect")
        } else {
            attempt.follow()
        }
    });
    Client::builder()
        .tls_backend_preconfigured(tls)
        .connect_timeout(stall)
        .read_timeout(stall)
        .redirect(policy)
        .build()
        .map_err(|e| failed("HTTP client", &e))
}

/// Whether `url` is plain `http` to this machine (`localhost`, `127.0.0.0/8`
/// or `[::1]`).
fn is_loopback_http(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };
    // An IPv6 host comes back in brackets ("[::1]").
    url.scheme() == "http"
        && url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        })
}

fn failed(what: &str, error: &dyn std::fmt::Display) -> TranscribeError {
    TranscribeError::Engine(format!("could not set up the {what}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_client_with_rustls_and_ring() {
        assert!(client("https://api.groq.com/openai/v1").is_ok());
        assert!(client("http://127.0.0.1:9/v1").is_ok());
    }

    #[test]
    fn only_plain_http_to_loopback_bypasses_proxies() {
        for url in [
            "http://127.0.0.1:18731/v1",
            "http://127.1.2.3/v1",
            "http://localhost:8080/v1",
            "http://LOCALHOST/v1",
            "http://[::1]:8080/v1",
        ] {
            assert!(is_loopback_http(url), "{url}");
        }
        for url in [
            "https://api.groq.com/openai/v1",
            "https://127.0.0.1/v1",
            "http://example.com/v1",
            "http://10.0.0.1/v1",
            "http://localhost.example.com/v1",
            "not a url",
        ] {
            assert!(!is_loopback_http(url), "{url}");
        }
    }

    #[test]
    fn failures_name_the_step() {
        assert_eq!(
            failed("HTTP client", &"boom").to_string(),
            "transcription engine error: could not set up the HTTP client: boom"
        );
    }
}
