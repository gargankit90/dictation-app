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

/// A client with `timeout` per request.
///
/// # Errors
///
/// [`TranscribeError::Engine`] if the TLS setup or the client can't be
/// built.
pub(crate) fn client(timeout: Duration) -> Result<Client, TranscribeError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| failed("TLS versions", &e))?
        .with_platform_verifier()
        .map_err(|e| failed("TLS trust store", &e))?
        .with_no_client_auth();
    Client::builder()
        .tls_backend_preconfigured(tls)
        .timeout(timeout)
        // Never follow a redirect: requests go only to the configured
        // provider, and a 3xx is a provider error (issue #76).
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| failed("HTTP client", &e))
}

fn failed(what: &str, error: &dyn std::fmt::Display) -> TranscribeError {
    TranscribeError::Engine(format!("could not set up the {what}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_client_with_rustls_and_ring() {
        assert!(client(Duration::from_secs(1)).is_ok());
    }

    #[test]
    fn failures_name_the_step() {
        assert_eq!(
            failed("HTTP client", &"boom").to_string(),
            "transcription engine error: could not set up the HTTP client: boom"
        );
    }
}
