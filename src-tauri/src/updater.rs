//! M5.4: "Check for updates…" in the tray, on top of `tauri-plugin-updater`.
//!
//! The plugin reads its endpoint list and the minisign **public** key from
//! `plugins.updater` in `tauri.conf.json` and refuses any package whose
//! signature doesn't verify against that key. The key is the owner's
//! (M5.4); an empty `pubkey` would mean a build without one, and the menu item
//! then says so instead of failing in a way a tester can't read.
//!
//! The app checks once at launch (quietly: it speaks up only to offer an
//! update) and whenever the tray item is chosen. Nothing is installed until
//! the user picks "Install and restart".
//!
//! The plugin's own commands (`check`, `download`, `install`) are not in any
//! capability, so no window can call them: only this module, from the tray,
//! starts an update.

use reqwest::Url;
use reqwest::redirect::Policy;
use serde_json::Value;
use tauri::{AppHandle, Builder, Runtime};
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

const SIGNATURE_MESSAGE: &str =
    "The update's signature didn't match this app's update key, so it was not installed.";

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
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => SIGNATURE_MESSAGE.to_owned(),
        Error::InsecureTransportProtocol => {
            "The update isn't offered over a secure connection, so it was refused.".to_owned()
        }
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => {
            "The newest release has no download for this Mac.".to_owned()
        }
        _ => {
            "The update check failed. The log has the details (tray menu > Reveal log).".to_owned()
        }
    }
}

/// Registers the updater and the native dialogs on `builder`. The shell and
/// the tests both call this, so a test builds the app with the plugins the
/// shipped app has (the tray action panics without their state).
pub fn plugins<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
}

/// Whether `plugins.updater.dangerousInsecureTransportProtocol` is on. Only
/// the local test harness (`scripts/updater-test.sh`) turns it on.
fn insecure_allowed(updater_config: Option<&Value>) -> bool {
    updater_config
        .and_then(|config| config.get("dangerousInsecureTransportProtocol"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The configured manifest endpoints.
fn endpoints(updater_config: Option<&Value>) -> Vec<Url> {
    updater_config
        .and_then(|config| config.get("endpoints"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter_map(|endpoint| Url::parse(endpoint).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Why a package URL from the manifest must not be fetched, or `None`.
///
/// The manifest comes from our endpoint, but its package URL is just data:
/// the plugin only warns about plain http and follows any host. The package
/// must be https (unless the test harness allows http), on the endpoint's own
/// host, and under the endpoint's `/releases/` path (a project's release
/// assets), so a wrong manifest can't point the download elsewhere. The
/// signature still protects what gets installed (#134).
#[must_use]
pub fn package_url_problem(
    package: &Url,
    endpoints: &[Url],
    insecure_ok: bool,
) -> Option<&'static str> {
    if package.scheme() != "https" && !insecure_ok {
        return Some("The update isn't offered over a secure connection, so it was refused.");
    }
    let same_place = endpoints.iter().any(|endpoint| {
        // A GitHub releases endpoint allows packages only under its
        // repository's `releases/download/`; any other endpoint (the local
        // test harness) allows its own host.
        let prefix = endpoint.path().find("/releases/").map_or_else(
            || "/".to_owned(),
            |at| format!("{}download/", &endpoint.path()[..at + "/releases/".len()]),
        );
        let path = package.path();
        // Encoded slashes and backslashes could smuggle a different path
        // past the prefix check; dot segments are already resolved by `Url`.
        let lower = path.to_ascii_lowercase();
        if lower.contains("%2f") || lower.contains("%5c") || path.split('/').any(|s| s == "..") {
            return false;
        }
        package.scheme() == endpoint.scheme()
            && package.host_str() == endpoint.host_str()
            && package.port_or_known_default() == endpoint.port_or_known_default()
            && path.starts_with(prefix.as_str())
    });
    if same_place {
        None
    } else {
        Some("The update isn't hosted where this app expects, so it was refused.")
    }
}

/// Whether a download may follow a redirect to `next`: https, on GitHub or
/// its release-asset hosts (`*.githubusercontent.com`), a few hops at most.
/// Release assets redirect from github.com to a storage host.
#[must_use]
pub fn redirect_allowed(next: &Url, hops: usize) -> bool {
    hops < 5
        && next.scheme() == "https"
        && next
            .host_str()
            .is_some_and(|host| host == "github.com" || host.ends_with(".githubusercontent.com"))
}

/// The updater for this app. With https only (production), redirects are
/// limited to [`redirect_allowed`]; the test harness (insecure transport on)
/// keeps the default policy for its local server.
fn updater_for<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<tauri_plugin_updater::Updater, tauri_plugin_updater::Error> {
    let mut builder = app.updater_builder();
    if !insecure_allowed(app.config().plugins.0.get("updater")) {
        builder = builder.configure_client(|client| {
            client.redirect(Policy::custom(|attempt| {
                if redirect_allowed(attempt.url(), attempt.previous().len()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
        });
    }
    builder.build()
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
    let updater = match updater_for(app) {
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
    let config = app.config().plugins.0.get("updater");
    if let Some(problem) = package_url_problem(
        &update.download_url,
        &endpoints(config),
        insecure_allowed(config),
    ) {
        tracing::warn!(host = ?update.download_url.host_str(), "update package refused");
        return Err(problem.to_owned());
    }
    tracing::info!(version = %update.version, "downloading the update");
    update
        .download_and_install(|_chunk, _total| {}, || tracing::info!("download finished"))
        .await
        .map_err(|error| {
            tracing::warn!(%error, "update failed");
            failure_reason(&error)
        })?;
    tracing::info!(version = %update.version, "update installed; restarting");
    // A restart request (not `restart`) runs the normal exit, so the
    // pipeline stops and the single-instance lock is released first.
    app.request_restart();
    Ok(())
}

/// Who asked for a check: the tray item tells the user every outcome; the
/// check at launch speaks up only when there is an update to offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// "Check for updates…" in the tray.
    Tray,
    /// The quiet check when the app starts.
    Launch,
}

impl Trigger {
    /// Whether the user sees `outcome` (when no update is offered).
    #[must_use]
    pub fn shows(self, outcome: &Outcome) -> bool {
        match self {
            Self::Tray => true,
            Self::Launch => matches!(outcome, Outcome::Available { .. }),
        }
    }
}

/// Whether to check for updates at launch: always for a person's launch; a
/// scripted test launch (the mock engine or a WAV for audio) only when it
/// asks with `--debug-update-check`, so an update dialog can't take the focus
/// from the window a test pastes into.
#[must_use]
pub fn check_at_launch_wanted(scripted: bool, asked: bool) -> bool {
    !scripted || asked
}

/// When this run of the app last heard from the update server (U6:
/// Settings, General shows it next to "Check for updates").
static LAST_CHECKED: std::sync::Mutex<Option<std::time::SystemTime>> = std::sync::Mutex::new(None);

fn note_checked(when: std::time::SystemTime) {
    *LAST_CHECKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(when);
}

/// When this run of the app last heard from the update server, if it has.
#[must_use]
pub fn last_checked() -> Option<std::time::SystemTime> {
    *LAST_CHECKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The tray item: checks, tells the user, and installs if they say yes.
/// Runs on the async runtime; the dialogs are native and don't block it.
pub fn check_from_tray<R: Runtime>(app: &AppHandle<R>) {
    spawn_check(app, Trigger::Tray);
}

/// The check at launch: offers an update if there is one; otherwise only
/// logs (no dialog for "up to date" or an offline Mac).
pub fn check_at_launch<R: Runtime>(app: &AppHandle<R>) {
    tracing::info!("checking for updates at launch");
    spawn_check(app, Trigger::Launch);
}

fn spawn_check<R: Runtime>(app: &AppHandle<R>, trigger: Trigger) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (outcome, update) = check(&app).await;
        if !matches!(outcome, Outcome::Failed { .. }) {
            note_checked(std::time::SystemTime::now());
        }
        let (title, text) = outcome.message();
        let Some(update) = update else {
            if !trigger.shows(&outcome) {
                return;
            }
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
    fn the_launch_check_speaks_only_to_offer_an_update() {
        let up_to_date = Outcome::UpToDate {
            current: "0.1.0".into(),
        };
        let failed = Outcome::Failed {
            reason: "offline".into(),
        };
        let available = Outcome::Available {
            current: "0.1.0".into(),
            version: "0.1.1".into(),
            notes: None,
        };
        for outcome in [&Outcome::NotConfigured, &up_to_date, &failed, &available] {
            assert!(Trigger::Tray.shows(outcome), "{outcome:?}");
        }
        assert!(Trigger::Launch.shows(&available));
        assert!(!Trigger::Launch.shows(&up_to_date));
        assert!(!Trigger::Launch.shows(&failed));
        assert!(!Trigger::Launch.shows(&Outcome::NotConfigured));
    }

    #[test]
    fn a_scripted_launch_checks_only_when_asked() {
        assert!(check_at_launch_wanted(false, false));
        assert!(check_at_launch_wanted(false, true));
        assert!(!check_at_launch_wanted(true, false));
        assert!(check_at_launch_wanted(true, true));
    }

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
        // The same plugin set the shipped app registers (`plugins`), so a
        // missing registration fails here as it would in the app.
        plugins(mock_builder()).build(context).unwrap()
    }

    /// The manifest key the updater looks for on this machine.
    fn platform_key() -> String {
        let os = match std::env::consts::OS {
            "macos" => "darwin",
            other => other,
        };
        format!("{os}-{}", std::env::consts::ARCH)
    }

    /// Serves a manifest announcing `version`, whose package is `package`.
    async fn serve(version: &str, package: &'static [u8]) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/latest.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "version": version,
                "notes": "Fixes things.",
                "platforms": {platform_key(): {
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

    #[test]
    fn the_shipped_plugin_set_provides_what_the_tray_action_uses() {
        // The tray action calls `app.dialog()` and `updater_for`; both read
        // state that only their plugins install, and panic without it (R1).
        let mut context = mock_context(noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            json!({"endpoints": ["https://example.invalid/latest.json"], "pubkey": TEST_PUBKEY}),
        );
        let app = plugins(mock_builder()).build(context).unwrap();
        let _ = app.handle().dialog().message("registered");
        assert!(updater_for(app.handle()).is_ok());
    }

    #[tokio::test]
    async fn a_manifest_with_a_garbled_signature_gets_the_signature_message() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/latest.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "version": "2.0.0",
                "platforms": {platform_key(): {
                    "url": format!("{}/package", server.uri()),
                    "signature": "this is not a signature",
                }},
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/package"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(&b"test"[..]))
            .mount(&server)
            .await;
        let app = app_for(&server, TEST_PUBKEY);
        let (outcome, update) = check(app.handle()).await;
        // Whether the garbled signature is caught at the check or only when
        // the package is verified, the user is told what it is.
        let message = match (outcome, update) {
            (Outcome::Failed { reason }, _) => reason,
            (Outcome::Available { .. }, Some(update)) => {
                install(app.handle(), &update).await.unwrap_err()
            }
            (outcome, _) => panic!("unexpected {outcome:?}"),
        };
        assert_eq!(message, SIGNATURE_MESSAGE);
    }

    #[tokio::test]
    async fn installing_a_tampered_package_says_why_and_installs_nothing() {
        let server = serve("2.0.0", b"tampered").await;
        let app = app_for(&server, TEST_PUBKEY);
        let (_, update) = check(app.handle()).await;
        let message = install(app.handle(), &update.unwrap()).await.unwrap_err();
        assert_eq!(message, SIGNATURE_MESSAGE);
    }

    #[test]
    fn signature_failures_share_one_clear_message() {
        use tauri_plugin_updater::Error;
        assert_eq!(
            failure_reason(&Error::MissingSignedVersion),
            SIGNATURE_MESSAGE
        );
        assert_eq!(
            failure_reason(&Error::SignatureUtf8("x".into())),
            SIGNATURE_MESSAGE
        );
        assert!(failure_reason(&Error::InsecureTransportProtocol).contains("secure connection"));
    }

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    /// The owner's endpoint (R1): this repository's releases.
    const ENDPOINT: &str =
        "https://github.com/gargankit90/dictation-app/releases/latest/download/latest.json";

    /// The owner's minisign public key (M5.4, `docs/history/milestones/M5.4.md`).
    const OWNER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEY2QzgwRjE3M0E5NkQ3OEYKUldTUDE1WTZGdy9JOWpxTlNIUjA4ampTb2pSdkFHdENZVjA4TEtBSkkwVW5zNHFTZkNmSURRWHAK";

    #[test]
    fn the_shipped_config_loads_into_the_plugin_with_the_owners_key_and_endpoint() {
        // The real tauri.conf.json: a typo in `plugins.updater` would stop
        // the app at startup, which no other test would notice.
        // (generate_context! can't run twice in one crate.)
        let file: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let config = file.pointer("/plugins/updater").cloned();
        let mut context = mock_context(noop_assets());
        context
            .config_mut()
            .plugins
            .0
            .insert("updater".into(), config.clone().unwrap());
        assert!(pubkey_configured(config.as_ref()));
        assert_eq!(
            config
                .as_ref()
                .and_then(|c| c.get("pubkey"))
                .and_then(Value::as_str),
            Some(OWNER_PUBKEY),
            "the updater must trust the owner's key and no other"
        );
        assert!(!insecure_allowed(config.as_ref()));
        let endpoints = endpoints(config.as_ref());
        assert_eq!(endpoints, [url(ENDPOINT)]);
        plugins(mock_builder()).build(context).unwrap();
    }

    #[test]
    fn the_shell_registers_the_plugins_through_plugins() {
        // `run` can't be built in a test (it starts the real app), so check
        // the source: the production builder must start from `plugins`.
        let shell = include_str!("lib.rs");
        assert!(
            shell.contains("updater::plugins(tauri::Builder::default())"),
            "lib.rs must register the updater and dialog plugins via updater::plugins"
        );
    }

    #[test]
    fn a_package_must_be_https_on_this_repos_release_downloads() {
        let endpoints = [url(ENDPOINT)];
        let ok = |text: &str| package_url_problem(&url(text), &endpoints, false).is_none();
        assert!(ok(
            "https://github.com/gargankit90/dictation-app/releases/download/v0.2.0/dictation-app.app.tar.gz"
        ));
        // The archived releases-only repository was never used for a release.
        assert!(!ok(
            "https://github.com/gargankit90/dictation-app-releases/releases/download/v0.2.0/a.tar.gz"
        ));
        // Look-alike repositories and hosts.
        assert!(!ok(
            "https://github.com/gargankit90/dictation-app.evil/releases/download/v1/a.tar.gz"
        ));
        assert!(!ok(
            "https://github.com/gargankit90/dictation-appx/releases/download/v1/a.tar.gz"
        ));
        assert!(!ok(
            "https://github.com.evil.example/gargankit90/dictation-app/releases/download/v1/a"
        ));
        assert!(!ok(
            "https://evil.example/gargankit90/dictation-app/releases/download/v1/a"
        ));
        assert!(!ok(
            "https://github.com/someone/else/releases/download/v1/a.tar.gz"
        ));
        // Other paths in the right repository.
        assert!(!ok(
            "https://github.com/gargankit90/dictation-app/releases/latest/download/a.tar.gz"
        ));
        assert!(!ok(
            "https://github.com/gargankit90/dictation-app/archive/main.tar.gz"
        ));
        // Path tricks.
        assert!(!ok(
            "https://github.com/gargankit90/dictation-app/releases/download/../../../evil/x/a.tar.gz"
        ));
        assert!(!ok(
            "https://github.com/gargankit90/dictation-app/releases/download/%2e%2e/%2e%2e/evil/a"
        ));
        assert!(!ok(
            "https://github.com/gargankit90/dictation-app/releases/download/v1%2F..%2F..%2Fa"
        ));
        assert!(!ok(
            "https://github.com/gargankit90/dictation-app/releases/download/v1%5Ca"
        ));
        assert!(!ok(
            "https://github.com/gargankit90/dictation-app%2Freleases/download/v1/a"
        ));
        // Not https, or another port.
        let http = url("http://github.com/gargankit90/dictation-app/releases/download/v1/a.tar.gz");
        assert!(
            package_url_problem(&http, &endpoints, false)
                .unwrap()
                .contains("secure connection")
        );
        assert!(!ok(
            "https://github.com:8443/gargankit90/dictation-app/releases/download/v1/a"
        ));
        assert!(
            package_url_problem(&url("https://evil.example/a"), &endpoints, false)
                .unwrap()
                .contains("isn't hosted where")
        );
        assert!(package_url_problem(&url(ENDPOINT), &[], false).is_some());
    }

    #[test]
    fn the_shipped_endpoint_is_this_repos_latest_json() {
        assert_eq!(
            ENDPOINT,
            "https://github.com/gargankit90/dictation-app/releases/latest/download/latest.json"
        );
    }

    #[test]
    fn plain_http_is_allowed_only_for_the_local_test_harness() {
        let endpoints = [url("http://192.168.64.1:8765/latest.json")];
        let package = url("http://192.168.64.1:8765/dictation-app_0.1.1.app.tar.gz");
        assert_eq!(package_url_problem(&package, &endpoints, true), None);
        assert!(package_url_problem(&package, &endpoints, false).is_some());
        // Even then, another host or port is refused.
        let other = url("http://192.168.64.1:9999/a.tar.gz");
        assert!(package_url_problem(&other, &endpoints, true).is_some());
    }

    #[test]
    fn redirects_stay_on_https_github_hosts() {
        assert!(redirect_allowed(
            &url("https://github.com/x/y/releases/download/v1/a"),
            0
        ));
        assert!(redirect_allowed(
            &url("https://objects.githubusercontent.com/github-production-release-asset/a"),
            1
        ));
        assert!(redirect_allowed(
            &url("https://release-assets.githubusercontent.com/a"),
            4
        ));
        assert!(
            !redirect_allowed(&url("http://github.com/a"), 0),
            "downgrade to http"
        );
        assert!(!redirect_allowed(&url("https://evil.example/a"), 0));
        assert!(!redirect_allowed(
            &url("https://githubusercontent.com.evil.example/a"),
            0
        ));
        assert!(
            !redirect_allowed(&url("https://github.com/a"), 5),
            "too many hops"
        );
    }

    #[test]
    fn config_readers_cope_with_missing_or_odd_values() {
        assert!(!insecure_allowed(None));
        assert!(!insecure_allowed(Some(
            &json!({"dangerousInsecureTransportProtocol": "yes"})
        )));
        assert!(insecure_allowed(Some(
            &json!({"dangerousInsecureTransportProtocol": true})
        )));
        assert!(endpoints(None).is_empty());
        assert_eq!(
            endpoints(Some(
                &json!({"endpoints": ["https://a.example/x", 7, "not a url"]})
            )),
            [url("https://a.example/x")]
        );
    }
}
