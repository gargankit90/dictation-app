//! The HTTP client the cloud engines share.

use std::time::Duration;

use dictation_ports::TranscribeError;
use reqwest::Client;

/// A client with `timeout` per request.
///
/// # Errors
///
/// [`TranscribeError::Engine`] if the client (its TLS setup) can't be built.
pub(crate) fn client(timeout: Duration) -> Result<Client, TranscribeError> {
    Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| TranscribeError::Engine(format!("could not start the HTTP client: {e}")))
}
