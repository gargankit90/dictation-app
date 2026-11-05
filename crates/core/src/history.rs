//! Recent dictations (U7, `docs/privacy.md`): one entry per dictation, what
//! the list keeps, and the file's JSON. Pure: `dictation-pipeline` holds the
//! list and a port writes the file.
//!
//! An entry holds the text, the time, the engine's name and whether it was
//! pasted, and has no field for the app or window it was pasted into.
//! `Debug` never prints the text or its length, and [`from_json`] gives no
//! parser message, because serde's can quote what it read.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The file format's version.
const FILE_VERSION: u32 = 1;

/// One dictation.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Unique among the entries, and never reused while the app runs.
    pub id: u64,
    /// The final text, without the smart leading space.
    pub text: String,
    /// When it finished, in milliseconds since the Unix epoch.
    pub created_at: u64,
    /// The engine's display name ("Groq", "On this Mac (Parakeet English)").
    pub engine: String,
    /// Whether the paste worked.
    pub pasted: bool,
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never the text, nor its length.
        f.debug_struct("Entry")
            .field("id", &self.id)
            .field("pasted", &self.pasted)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize)]
struct FileOut<'a> {
    version: u32,
    entries: &'a [Entry],
}

#[derive(Deserialize)]
struct FileIn {
    version: u32,
    entries: Vec<Entry>,
}

/// The file's JSON for `entries`, or `None` if it can't be made. There is
/// no error to log, on purpose.
#[must_use]
pub fn to_json(entries: &[Entry]) -> Option<String> {
    serde_json::to_string_pretty(&FileOut {
        version: FILE_VERSION,
        entries,
    })
    .ok()
}

/// The entries in the file's JSON, newest first, or `None` if this build
/// can't read it (damaged, or from a newer version).
#[must_use]
pub fn from_json(json: &str) -> Option<Vec<Entry>> {
    let file: FileIn = serde_json::from_str(json).ok()?;
    if file.version > FILE_VERSION {
        return None;
    }
    let mut entries = file.entries;
    entries.retain(|entry| !entry.text.is_empty());
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.id));
    Some(entries)
}

/// The id for an entry made at `now`: its time, moved on if the clock would
/// repeat or go back past the newest entry or `after`.
#[must_use]
pub fn next_id(entries: &[Entry], after: u64, now: u64) -> u64 {
    let newest = entries.first().map_or(0, |entry| entry.id).max(after);
    now.max(newest.saturating_add(1))
}

/// Puts `entry` first and drops the oldest entries beyond `limit`.
pub fn push(entries: &mut Vec<Entry>, entry: Entry, limit: usize) {
    entries.insert(0, entry);
    entries.truncate(limit);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u64, text: &str) -> Entry {
        Entry {
            id,
            text: text.to_owned(),
            created_at: id,
            engine: "Groq".to_owned(),
            pasted: true,
        }
    }

    #[test]
    fn criterion_2_the_oldest_go_first_and_the_list_never_exceeds_the_limit() {
        let mut entries = Vec::new();
        for id in 1..=4 {
            push(&mut entries, entry(id, "words"), 3);
        }
        let ids: Vec<u64> = entries.iter().map(|entry| entry.id).collect();
        assert_eq!(ids, [4, 3, 2]);
    }

    #[test]
    fn criterion_2_the_file_round_trips_newest_first() {
        let entries = vec![entry(2, "two"), entry(1, "one")];
        let json = to_json(&entries).unwrap();
        assert!(json.contains(r#""version": 1"#), "{json}");
        assert_eq!(from_json(&json).unwrap(), entries);
    }

    /// Criterion 4: the stored fields are exactly the spec's, so a name of
    /// an app or window has nowhere to go.
    #[test]
    fn criterion_4_an_entry_has_only_the_specs_fields() {
        let value = serde_json::to_value(entry(1, "words")).unwrap();
        let mut fields: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(fields, ["created_at", "engine", "id", "pasted", "text"]);
    }

    /// Criterion 4: a field put in by hand isn't kept when the file is
    /// next written.
    #[test]
    fn criterion_4_an_unknown_field_does_not_survive_a_rewrite() {
        let json = r#"{"version":1,"entries":[{"id":1,"text":"words","created_at":1,
            "engine":"Groq","pasted":true,"app":"com.example.Mail"}]}"#;
        let rewritten = to_json(&from_json(json).unwrap()).unwrap();
        assert!(!rewritten.contains("com.example.Mail"), "{rewritten}");
    }

    #[test]
    fn a_damaged_or_newer_file_is_not_read() {
        for json in [
            "",
            "not json",
            "[]",
            r#"{"version":1}"#,
            r#"{"version":2,"entries":[]}"#,
            r#"{"version":1,"entries":[{"id":"x"}]}"#,
        ] {
            assert_eq!(from_json(json), None, "{json}");
        }
    }

    #[test]
    fn a_file_out_of_order_or_with_an_empty_text_is_put_right() {
        let json = to_json(&[entry(1, "one"), entry(3, ""), entry(2, "two")]).unwrap();
        let ids: Vec<u64> = from_json(&json)
            .unwrap()
            .iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(ids, [2, 1]);
    }

    #[test]
    fn ids_never_repeat_even_if_the_clock_goes_back_or_the_list_was_cleared() {
        assert_eq!(next_id(&[], 0, 100), 100);
        assert_eq!(next_id(&[entry(100, "a")], 0, 100), 101);
        assert_eq!(next_id(&[entry(100, "a")], 0, 50), 101);
        assert_eq!(next_id(&[], 100, 50), 101);
    }

    #[test]
    fn criterion_4_debug_never_prints_the_text_or_its_length() {
        let shown = format!("{:?}", entry(1, "my private words"));
        assert!(!shown.contains("private"), "{shown}");
        assert!(!shown.contains("16"), "{shown}");
    }
}
