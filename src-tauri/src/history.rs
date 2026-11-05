//! Settings → History's commands (U7). The entries live in
//! [`dictation_pipeline::history::History`]; the text goes to the settings
//! window and, for Copy, to the clipboard, and nowhere else. Nothing here logs
//! it. An error says what failed, from the OS's reason, without any text.

use std::sync::Arc;

use dictation_core::history::Entry;
use dictation_pipeline::config::Config;
use dictation_pipeline::history::History;
use dictation_ports::SecretStore;
use serde::Serialize;
use tauri::State;

use crate::commands::{self, AppConfig};
use crate::usage::{self, Usage};

/// The recent dictations, as Tauri state.
pub struct AppHistory(pub Arc<History>);

/// One row of Settings → History. Deliberately no `Debug`: it holds the text.
#[derive(Serialize, specta::Type)]
pub struct HistoryRow {
    /// The entry's id, as text (a 64-bit number doesn't fit a JS number).
    pub id: String,
    /// The whole text; the window shows two lines of it.
    pub text: String,
    /// When it finished, in milliseconds since the Unix epoch.
    pub created_at: f64,
    /// Whether the paste worked.
    pub pasted: bool,
}

/// What Settings → History shows.
#[derive(Serialize, specta::Type)]
pub struct HistoryList {
    /// The entries, newest first.
    pub rows: Vec<HistoryRow>,
    /// A history file exists that can't be read, so nothing is being saved
    /// until it can be, or Clear or Off deletes it.
    pub unreadable: bool,
}

impl From<Entry> for HistoryRow {
    #[allow(
        clippy::cast_precision_loss,
        reason = "milliseconds since 1970 fit a double exactly for 285,000 years"
    )]
    fn from(entry: Entry) -> Self {
        Self {
            id: entry.id.to_string(),
            text: entry.text,
            created_at: entry.created_at as f64,
            pasted: entry.pasted,
        }
    }
}

/// An id from the window: one this app gave it, so anything else is a bug.
fn parse_id(id: &str) -> Result<u64, String> {
    id.parse().map_err(|_| "that isn't a dictation".to_owned())
}

/// Runs `call` on the blocking pool: the file and the clipboard can block.
async fn blocking<T: Send + 'static>(
    call: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(call)
        .await
        .map_err(|_| "the history task crashed".to_owned())?
}

/// The recent dictations, newest first. History is in memory, so no blocking.
#[tauri::command]
#[specta::specta]
pub async fn list_history(history: State<'_, AppHistory>) -> Result<HistoryList, String> {
    Ok(HistoryList {
        rows: history.0.entries().into_iter().map(Into::into).collect(),
        unreadable: history.0.is_unreadable(),
    })
}

/// Puts an entry's full text on the clipboard, with the same privacy markers
/// as the menu's "Copy last dictation", so clipboard managers skip it.
#[tauri::command]
#[specta::specta]
pub async fn copy_history_entry(history: State<'_, AppHistory>, id: String) -> Result<(), String> {
    let text = history
        .0
        .text_of(parse_id(&id)?)
        .ok_or_else(|| "That dictation is gone.".to_owned())?;
    blocking(move || {
        dictation_platform::insert::SystemClipboard::new()
            .copy(&text)
            .map_err(|error| {
                tracing::warn!(%error, "could not copy a dictation");
                error.to_string()
            })
    })
    .await
}

/// Deletes one entry.
#[tauri::command]
#[specta::specta]
pub async fn delete_history_entry(
    history: State<'_, AppHistory>,
    id: String,
) -> Result<(), String> {
    let id = parse_id(&id)?;
    let history = Arc::clone(&history.0);
    blocking(move || history.delete(id).map_err(|error| failed(&error))).await
}

/// Deletes every entry. The window has already asked the user.
#[tauri::command]
#[specta::specta]
pub async fn clear_history(history: State<'_, AppHistory>) -> Result<(), String> {
    let history = Arc::clone(&history.0);
    blocking(move || history.clear().map_err(|error| failed(&error))).await
}

/// Sets how many dictations are kept, and nothing else, then trims to it at
/// once; 0 (Off) deletes them all. The window has already asked the user if
/// that deletes anything.
#[tauri::command]
#[specta::specta]
pub async fn set_history_limit(
    state: State<'_, AppConfig>,
    history: State<'_, AppHistory>,
    usage: State<'_, Usage>,
    limit: u32,
) -> Result<(), String> {
    let (config, history, usage) = (
        Arc::clone(&state.0),
        Arc::clone(&history.0),
        usage.inner().clone(),
    );
    blocking(move || write_limit(&config, &history, &usage, limit)).await
}

/// The setting first: if it can't be saved (or `limit` isn't allowed) nothing
/// is deleted. Records that the setting changed, never its value.
fn write_limit<S: SecretStore>(
    config: &Config<S>,
    history: &History,
    usage: &Usage,
    limit: u32,
) -> Result<(), String> {
    let _writing = commands::settings_write();
    let before = config.settings().map_err(|error| error.to_string())?;
    let mut after = before.clone();
    after.history_limit = limit;
    if after != before {
        config
            .save_settings(&after)
            .map_err(|error| error.to_string())?;
        usage::settings_saved(usage, usage::settings_change(&before, &after));
    }
    history.set_limit(limit).map_err(|error| failed(&error))
}

fn failed(error: &dictation_ports::HistoryStoreError) -> String {
    tracing::warn!(%error, "could not change the history");
    error.to_string()
}

#[cfg(test)]
mod tests {
    use dictation_platform::fakes::{MemHistoryStore, MemSecretStore};

    use super::*;

    struct Fixture {
        dir: std::path::PathBuf,
        config: Config<MemSecretStore>,
        store: MemHistoryStore,
        history: History,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "dictation-history-commands-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let store = MemHistoryStore::new();
            let history = History::open(store.clone(), Some(25));
            for n in 0..12 {
                history.record(&format!("dictation {n}"), "Groq", true);
            }
            Self {
                config: Config::new(&dir, MemSecretStore::new()),
                dir,
                store,
                history,
            }
        }

        fn write(&self, limit: u32) -> Result<(), String> {
            write_limit(&self.config, &self.history, &Usage::disabled(), limit)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn criterion_3_the_setting_is_saved_and_the_list_trimmed() {
        let fixture = Fixture::new("trim");
        fixture.write(10).unwrap();
        assert_eq!(fixture.config.settings().unwrap().history_limit, 10);
        assert_eq!(fixture.history.entries().len(), 10);
        fixture.write(0).unwrap();
        assert_eq!(fixture.config.settings().unwrap().history_limit, 0);
        assert!(fixture.history.entries().is_empty());
        assert_eq!(fixture.store.contents(), None);
    }

    #[test]
    fn criterion_1_a_value_not_allowed_is_refused_and_deletes_nothing() {
        let fixture = Fixture::new("invalid");
        let error = fixture.write(7).unwrap_err();
        assert!(error.contains("history_limit"), "{error}");
        assert_eq!(fixture.history.entries().len(), 12);
        assert_eq!(fixture.config.settings().unwrap().history_limit, 25);
    }

    #[test]
    fn a_setting_that_cannot_be_saved_deletes_nothing() {
        let fixture = Fixture::new("unsaved");
        std::fs::remove_dir_all(&fixture.dir).unwrap();
        std::fs::write(&fixture.dir, "not a folder").unwrap();
        assert!(fixture.write(10).is_err());
        assert_eq!(fixture.history.entries().len(), 12);
        std::fs::remove_file(&fixture.dir).unwrap();
    }

    #[test]
    fn only_the_limit_changes() {
        let fixture = Fixture::new("only");
        let mut settings = fixture.config.settings().unwrap();
        settings.hotkey = "F6".to_owned();
        settings.share_usage_events = false;
        fixture.config.save_settings(&settings).unwrap();
        fixture.write(50).unwrap();
        let after = fixture.config.settings().unwrap();
        assert_eq!(after.history_limit, 50);
        assert_eq!(after.hotkey, "F6");
        assert!(!after.share_usage_events);
    }

    /// Row text for the window, and an id a JS number can't hold exactly.
    #[test]
    fn criterion_5_a_row_has_the_full_text_and_its_id_as_text() {
        let row = HistoryRow::from(Entry {
            id: u64::MAX,
            text: "all of it".to_owned(),
            created_at: 1_000,
            engine: "Groq".to_owned(),
            pasted: false,
        });
        assert_eq!(row.id, u64::MAX.to_string());
        assert_eq!(parse_id(&row.id), Ok(u64::MAX));
        assert_eq!(row.text, "all of it");
        assert!(!row.pasted);
        assert!(parse_id("not a number").is_err());
    }
}
