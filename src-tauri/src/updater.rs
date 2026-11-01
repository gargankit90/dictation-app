//! M5.4: "Check for updates…" in the tray, on top of `tauri-plugin-updater`.
//!
//! The plugin reads its endpoint list and the minisign **public** key from
//! `plugins.updater` in `tauri.conf.json` and refuses any package whose
//! signature doesn't verify against that key. An empty `pubkey` means the
//! owner hasn't approved a key yet (see `docs/releasing.md`): the menu item
//! then says so instead of failing in a way a tester can't read.
//!
//! The plugin's own commands (`check`, `download`, `install`) are not in any
//! capability, so no window can call them: only this module, from the tray,
//! starts an update.

use serde_json::Value;
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The tray item's id.
pub const CHECK_ID: &str = "check-updates";

/// The tray item's label.
pub const CHECK_LABEL: &str = "Check for updates…";

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// This build has no update key yet, so it cannot verify an update.
    NotConfigured,
    /// The newest release is the one running.
    UpToDate { current: String },
    /// A newer release is published.
    Available {
        current: String,
        version: String,
        notes: Option<String>,
    },
    /// The check failed; `reason` is for the user, not a raw error.
    Failed { reason: String },
}

impl Outcome {
    /// The dialog's title and text.
    #[must_use]
    pub fn message(&self) -> (&'static str, String) {
        match self {
            Self::NotConfigured => (
                "Updates aren't set up",
                "This build can't check for updates yet. Download the newest version from the \
                 link you were sent."
                    .to_owned(),
            ),
            Self::UpToDate { current } => (
                "You're up to date",
                format!("dictation-app {current} is the newest version."),
            ),
            Self::Available {
                current,
                version,
                notes,
            } => {
                let mut text = format!(
                    "dictation-app {version} is available; you have {current}.\n\nInstall it \
                     now? The app restarts, and your Microphone and Accessibility permissions \
                     stay."
                );
                if let Some(notes) = notes.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                    text.push_str("\n\nWhat's new:\n");
                    text.push_str(&shorten(notes, MAX_NOTES_CHARS));
                }
                ("Update available", text)
            }
            Self::Failed { reason } => ("Couldn't check for updates", reason.clone()),
        }
    }
}

/// Release notes longer than this are cut, so the dialog stays small.
const MAX_NOTES_CHARS: usize = 600;

fn shorten(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max_chars).collect();
    format!("{}…", cut.trim_end())
}

/// Whether `plugins.updater` has a public key. An empty one is the
/// placeholder before the owner approves the real key.
#[must_use]
pub fn pubkey_configured(updater_config: Option<&Value>) -> bool {
    updater_config
        .and_then(|config| config.get("pubkey"))
        .and_then(Value::as_str)
        .is_some_and(|key| !key.trim().is_empty())
}

/// A user-facing reason for a failed check. Raw errors name URLs and
/// libraries; only their kind helps a tester, and the log has the rest.
#[must_use]
pub fn failure_reason(error: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error;
    match error {
        Error::Reqwest(_) | Error::Network(_) | Error::ReleaseNotFound => {
            "Couldn't reach the update server. Check your internet connection and try again."
                .to_owned()
        }
        Error::EmptyEndpoints => "This build has no update server set.".to_owned(),
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => {
            "The newest release has no download for this Mac.".to_owned()
        }
        _ => {
            "The update check failed. The log has the details (tray menu > Reveal log).".to_owned()
        }
    }
}

/// Asks the update server whether a newer release exists.
///
/// The `Update` comes back with an [`Outcome::Available`], for the install
/// step. Nothing is logged at info with the server's text: only the
/// versions.
pub async fn check<R: Runtime>(app: &AppHandle<R>) -> (Outcome, Option<Update>) {
    let configured = pubkey_configured(app.config().plugins.0.get("updater"));
    if !configured {
        tracing::info!("update check skipped: no update key in this build");
        return (Outcome::NotConfigured, None);
    }
    let current = app.package_info().version.to_string();
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(error) => {
            tracing::warn!(%error, "could not set up the updater");
            return (
                Outcome::Failed {
                    reason: failure_reason(&error),
                },
                None,
            );
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            tracing::info!(current, version = %update.version, "update available");
            let outcome = Outcome::Available {
                current,
                version: update.version.clone(),
                notes: update.body.clone(),
            };
            (outcome, Some(update))
        }
        Ok(None) => {
            tracing::info!(current, "up to date");
            (Outcome::UpToDate { current }, None)
        }
        Err(error) => {
            tracing::warn!(%error, "update check failed");
            (
                Outcome::Failed {
                    reason: failure_reason(&error),
                },
                None,
            )
        }
    }
}

/// Downloads `update`, verifies its signature, installs it and restarts the
/// app. The plugin verifies before it installs, so a package that doesn't
/// match the key is never written.
///
/// # Errors
///
/// A user-facing reason when the download, the signature check or the
/// install fails.
pub async fn install<R: Runtime>(app: &AppHandle<R>, update: &Update) -> Result<(), String> {
    tracing::info!(version = %update.version, "downloading the update");
    update
        .download_and_install(|_chunk, _total| {}, || tracing::info!("download finished"))
        .await
        .map_err(|error| {
            tracing::warn!(%error, "update failed");
            match error {
                tauri_plugin_updater::Error::Minisign(_)
                | tauri_plugin_updater::Error::Base64(_)
                | tauri_plugin_updater::Error::SignatureUtf8(_)
                | tauri_plugin_updater::Error::SignedVersionMismatch { .. }
                | tauri_plugin_updater::Error::MissingSignedVersion => {
                    "The download didn't match its signature, so it was not installed.".to_owned()
                }
                other => failure_reason(&other),
            }
        })?;
    tracing::info!(version = %update.version, "update installed; restarting");
    // A restart request (not `restart`) runs the normal exit, so the
    // pipeline stops and the single-instance lock is released first.
    app.request_restart();
    Ok(())
}

/// The tray item: checks, tells the user, and installs if they say yes.
/// Runs on the async runtime; the dialogs are native and don't block it.
pub fn check_from_tray<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (outcome, update) = check(&app).await;
        let (title, text) = outcome.message();
        let Some(update) = update else {
            let kind = match outcome {
                Outcome::Failed { .. } => MessageDialogKind::Error,
                _ => MessageDialogKind::Info,
            };
            app.dialog()
                .message(text)
                .title(title)
                .kind(kind)
                .show(|_| {});
            return;
        };
        let dialog = app
            .dialog()
            .message(text)
            .title(title)
            .kind(MessageDialogKind::Info)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Install and restart".to_owned(),
                "Later".to_owned(),
            ));
        let accepted = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show())
            .await
            .unwrap_or(false);
        if !accepted {
            tracing::info!("update postponed");
            return;
        }
        if let Err(reason) = install(&app, &update).await {
            app.dialog()
                .message(reason)
                .title("Couldn't install the update")
                .kind(MessageDialogKind::Error)
                .show(|_| {});
        }
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_empty_or_missing_key_is_not_configured() {
        assert!(!pubkey_configured(None));
        assert!(!pubkey_configured(Some(&json!({}))));
        assert!(!pubkey_configured(Some(&json!({"pubkey": ""}))));
        assert!(!pubkey_configured(Some(&json!({"pubkey": "  \n"}))));
        assert!(!pubkey_configured(Some(&json!({"pubkey": 7}))));
        assert!(pubkey_configured(Some(&json!({"pubkey": "dW50cnVzdGVk"}))));
    }

    #[test]
    fn every_outcome_has_a_clear_message() {
        let (title, text) = Outcome::NotConfigured.message();
        assert_eq!(title, "Updates aren't set up");
        assert!(text.contains("can't check for updates yet"), "{text}");

        let (title, text) = Outcome::UpToDate {
            current: "0.1.0".into(),
        }
        .message();
        assert_eq!(title, "You're up to date");
        assert!(text.contains("0.1.0 is the newest"), "{text}");

        let (title, text) = Outcome::Failed {
            reason: "No network.".into(),
        }
        .message();
        assert_eq!(title, "Couldn't check for updates");
        assert_eq!(text, "No network.");
    }

    #[test]
    fn an_available_update_names_both_versions_and_the_notes() {
        let (title, text) = Outcome::Available {
            current: "0.1.0".into(),
            version: "0.2.0".into(),
            notes: Some("  Fixes the overlay.  ".into()),
        }
        .message();
        assert_eq!(title, "Update available");
        assert!(
            text.contains("0.2.0 is available; you have 0.1.0"),
            "{text}"
        );
        assert!(text.contains("permissions"), "{text}");
        assert!(text.ends_with("What's new:\nFixes the overlay."), "{text}");
    }

    #[test]
    fn empty_or_long_notes_are_dropped_or_cut() {
        let without = |notes: Option<&str>| {
            Outcome::Available {
                current: "1".into(),
                version: "2".into(),
                notes: notes.map(String::from),
            }
            .message()
            .1
        };
        assert!(!without(None).contains("What's new"));
        assert!(!without(Some("  \n ")).contains("What's new"));
        let long = "x".repeat(MAX_NOTES_CHARS + 50);
        let text = without(Some(&long));
        assert!(text.ends_with('…'), "{text}");
        assert_eq!(
            text.matches('x').count(),
            MAX_NOTES_CHARS,
            "cut to the limit"
        );
    }

    #[test]
    fn failures_are_explained_without_raw_errors() {
        use tauri_plugin_updater::Error;
        assert!(failure_reason(&Error::ReleaseNotFound).contains("internet connection"));
        assert!(failure_reason(&Error::EmptyEndpoints).contains("no update server"));
        assert!(failure_reason(&Error::TargetNotFound("darwin".into())).contains("this Mac"));
        let other = failure_reason(&Error::UnsupportedOs);
        assert!(other.contains("Reveal log"), "{other}");
        assert!(!other.contains("UnsupportedOs"));
    }

    // The updater's own published test vectors (tauri-plugin-updater's unit
    // tests): a throwaway key pair, the signature of the payload `test`
    // made with `tauri signer sign --app-version 2.0.0`, both base64 of the
    // minisign text. Only the public key and signature are here; no private
    // key is needed to check a download against them.
    const TEST_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEE0OUVGNDc0NkZBNkU2RDcKUldUWDVxWnZkUFNlcERHV2cyYno4bjZBclFuQk13Z1FLMWdlVm9zb3JjYkx6Y3VuRCtLSitjWWU=";
    const TEST_SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUWDVxWnZkUFNlcFBGMnpzanZQL0wvVFF0QkxSbldSdmFxKzRvbUsxVUV1Z3RRcC9VU3JHaUkxdjc2MnNEeTNpZDNhb3I3YlJDOUVUeWVvdDZsTUhIcldvWjVoalVNMVFjPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkxNjQ5MDUxCWZpbGU6cGF5bG9hZAl2ZXJzaW9uOjIuMC4wCkREcTJDTE9FbDVjUW1DMmVKUkhsblpQVlZpT1pzdTFDanBQS1FvZWk0QS9YQmZrRHFHTzBCK1FNcmNySGdFUTVxVjNtbmU3NXRkRldoSnBxVVhoOUFBPT0=";

    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// An app on the mock runtime with the updater plugin configured the way
    /// `tauri.conf.json` does it, but pointing at `server`.
    fn app_for(server: &MockServer, pubkey: &str) -> tauri::App<MockRuntime> {
        let mut context = mock_context(noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            json!({
                "endpoints": [format!("{}/latest.json", server.uri())],
                "pubkey": pubkey,
                "requireSignedVersion": true,
                // The mock server speaks http; the real endpoint is https.
                "dangerousInsecureTransportProtocol": true,
            }),
        );
        mock_builder()
            .plugin(
                tauri_plugin_updater::Builder::new()
                    .target("test-target")
                    .build(),
            )
            .build(context)
            .unwrap()
    }

    /// Serves a manifest announcing `version`, whose package is `package`.
    async fn serve(version: &str, package: &'static [u8]) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/latest.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "version": version,
                "notes": "Fixes things.",
                "platforms": {"test-target": {
                    "url": format!("{}/package", server.uri()),
                    "signature": TEST_SIGNATURE,
                }},
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/package"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(package))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn a_build_without_a_key_does_not_ask_the_server() {
        let server = serve("2.0.0", b"test").await;
        let app = app_for(&server, "");
        let (outcome, update) = check(app.handle()).await;
        assert_eq!(outcome, Outcome::NotConfigured);
        assert!(update.is_none());
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_newer_release_is_offered_with_both_versions() {
        let server = serve("2.0.0", b"test").await;
        let app = app_for(&server, TEST_PUBKEY);
        let current = app.package_info().version.to_string();
        let (outcome, update) = check(app.handle()).await;
        assert_eq!(
            outcome,
            Outcome::Available {
                current,
                version: "2.0.0".into(),
                notes: Some("Fixes things.".into())
            }
        );
        assert!(update.is_some());
    }

    #[tokio::test]
    async fn the_same_or_an_older_release_is_up_to_date() {
        let server = serve("0.0.1", b"test").await;
        let app = app_for(&server, TEST_PUBKEY);
        let (outcome, update) = check(app.handle()).await;
        assert!(matches!(outcome, Outcome::UpToDate { .. }), "{outcome:?}");
        assert!(update.is_none());
        let (_, text) = outcome.message();
        assert!(text.contains("newest version"), "{text}");
    }

    #[tokio::test]
    async fn an_unreachable_server_is_a_plain_failure() {
        let server = MockServer::start().await;
        let app = app_for(&server, TEST_PUBKEY);
        drop(server);
        let (outcome, _) = check(app.handle()).await;
        let Outcome::Failed { reason } = outcome else {
            panic!("expected a failure");
        };
        assert!(reason.contains("internet connection"), "{reason}");
    }

    #[tokio::test]
    async fn a_package_that_matches_its_signature_downloads() {
        let server = serve("2.0.0", b"test").await;
        let app = app_for(&server, TEST_PUBKEY);
        let (_, update) = check(app.handle()).await;
        let bytes = update.unwrap().download(|_, _| {}, || {}).await.unwrap();
        assert_eq!(bytes, b"test");
    }

    #[tokio::test]
    async fn a_tampered_package_is_refused_before_anything_is_installed() {
        let server = serve("2.0.0", b"tampered").await;
        let app = app_for(&server, TEST_PUBKEY);
        let (_, update) = check(app.handle()).await;
        let result = update.unwrap().download(|_, _| {}, || {}).await;
        assert!(result.is_err(), "a changed package must not verify");
    }

    #[tokio::test]
    async fn a_signature_from_another_key_is_refused() {
        // Right payload and signature, but this build trusts a different key.
        let other = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXkgRTc2MjBGMTg0MkI0RTgxRgpSV1FmNkxSQ0dBOWk1M21sWWVjTzRJelQ1MVRHUHB2V3VjTlNDaDFDQk0wUVRhTG43M1k3R0ZPMw==";
        let server = serve("2.0.0", b"test").await;
        let app = app_for(&server, other);
        let (_, update) = check(app.handle()).await;
        let result = update.unwrap().download(|_, _| {}, || {}).await;
        assert!(result.is_err(), "an untrusted signer must not verify");
    }
}
