//! U1 criterion 1: the name people see is Vocet. This fails if
//! "dictation-app" appears in a user-visible string in `src/`,
//! `src-tauri/src/`, the other crates' code, the HTML pages, the microphone
//! purpose string or the window titles and product name in
//! `tauri.conf.json`.
//!
//! "dictation-app" stays where it is an internal identifier, so settings,
//! the saved key and permissions survive the rename: the log file prefix,
//! GitHub URLs, the overlay's hidden window title, the signing certificate
//! name, telemetry client names and crate names. Those lines are listed in
//! [`INTERNAL`]; add to it only for a string no person reads.

use std::fs;
use std::path::{Path, PathBuf};

const OLD_NAME: &str = "dictation-app";

/// Lines that may name the old crate/identifier: nobody reads them.
const INTERNAL: [&str; 6] = [
    "github.com/gargankit90/dictation-app", // URLs
    "dictation-app overlay",                // the overlay's hidden window title
    "dictation-app.log",                    // the log file prefix
    "-p dictation-app",                     // cargo commands
    "a thread panicked before logging",     // stderr before logging starts
    "signingIdentity",                      // the certificate keeps its name
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn files_under(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if name == "tests" || name == "node_modules" || name == "target" {
                continue;
            }
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}

fn is_comment(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("//") || line.starts_with('*') || line.starts_with("/*")
}

/// The lines of `path` that a person can read, with the test code cut off.
fn visible_lines(path: &Path) -> Vec<(usize, String)> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    if !matches!(extension, "rs" | "ts" | "tsx" | "html" | "plist" | "json") {
        return Vec::new();
    }
    if name == "tests.rs" || name.contains(".test.") || name == "bindings.ts" {
        return Vec::new();
    }
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for (index, line) in text.lines().enumerate() {
        // Unit tests sit at the end of a Rust file.
        if extension == "rs" && line.trim() == "#[cfg(test)]" {
            break;
        }
        if !is_comment(line) {
            lines.push((index + 1, line.to_owned()));
        }
    }
    lines
}

#[test]
fn no_user_visible_string_says_dictation_app() {
    let root = root();
    let mut files = Vec::new();
    for dir in ["src", "src-tauri/src", "crates/core/src", "crates/app/src"] {
        files_under(&root.join(dir), &mut files);
    }
    for file in [
        "index.html",
        "overlay.html",
        "src-tauri/Info.plist",
        "src-tauri/tauri.conf.json",
    ] {
        files.push(root.join(file));
    }
    let mut offenders = Vec::new();
    for file in files {
        for (number, line) in visible_lines(&file) {
            if line.contains(OLD_NAME) && !INTERNAL.iter().any(|ok| line.contains(ok)) {
                let shown = file.strip_prefix(&root).unwrap_or(&file);
                offenders.push(format!("{}:{number}: {}", shown.display(), line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "user-visible text still says {OLD_NAME}; say Vocet:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_app_is_called_vocet_in_its_config() {
    let config = fs::read_to_string(root().join("src-tauri/tauri.conf.json")).unwrap();
    assert!(config.contains("\"productName\": \"Vocet\""));
    assert!(config.contains("\"title\": \"Vocet Settings\""));
    // The identifier is what keeps permissions, settings and the saved key.
    assert!(config.contains("\"identifier\": \"com.dictationapp.desktop\""));
    let plist = fs::read_to_string(root().join("src-tauri/Info.plist")).unwrap();
    assert!(plist.contains("Vocet records your voice"));
}

/// U1 criterion 2: an update to Vocet keeps what testers saved. The app-data
/// folder comes from the bundle identifier (pinned above), the Keychain
/// service is `dictation_platform::secrets::SERVICE`, and the settings file
/// is read by the same code as before. A settings file and a key written by
/// the last dictation-app build load unchanged.
#[test]
#[allow(clippy::unwrap_used, reason = "a test fails on purpose")]
fn saved_settings_and_key_still_load_after_the_rename() {
    use dictation_core::settings::Provider;
    use dictation_pipeline::config::{Config, SETTINGS_FILE};
    use dictation_platform::fakes::MemSecretStore;
    use dictation_ports::SecretStore as _;

    assert_eq!(
        dictation_platform::secrets::SERVICE,
        "com.dictationapp.desktop"
    );

    let dir = std::env::temp_dir().join(format!("u1-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(SETTINGS_FILE),
        r#"{"version":2,"provider":"openai","model":"whisper-1","language":"en","hotkey":"RightCmd","launch_at_login":true,"transcription":"cloud","local_model":"parakeet-unified-en-0.6b-q4_k_m","share_usage_events":false}"#,
    )
    .unwrap();
    let secrets = MemSecretStore::default();
    secrets
        .set(
            "openai-api-key",
            &dictation_ports::Secret::new("sk-dummy-NOT-A-REAL-KEY-0123456789"),
        )
        .unwrap();

    let config = Config::new(&dir, secrets);
    let settings = config.settings().unwrap();
    assert_eq!(settings.provider, Provider::OpenAi);
    assert_eq!(settings.hotkey, "RightCmd");
    assert!(settings.launch_at_login);
    assert!(config.has_api_key(Provider::OpenAi).unwrap());
    let _ = fs::remove_dir_all(&dir);
}
