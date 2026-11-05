//! Recent dictations (U7), for Settings → History and the menu's "Copy last
//! dictation".
//!
//! [`Recording`] wraps the text inserter: after each paste it hands the final
//! text and the outcome to [`History`], which keeps the newest entries up to
//! the user's limit in a file only they can read (`docs/privacy.md`).
//!
//! - A paste Secure Input refused (a password field has focus) is never
//!   kept, in the file or in memory: those words may be a password. Any other
//!   failed paste is kept with `pasted: false`.
//! - With history Off nothing is written. The last pasted text stays in
//!   memory for "Copy last dictation", as U5 had it, and is gone at quit.
//! - A file that can't be read (damaged, from a newer version, or a read that
//!   fails) is never written over, as it may hold the user's entries. Nothing
//!   is saved until it can be read or the user deletes it with Clear or Off.
//! - Nothing here logs an entry's text or length, or a parser's message
//!   (which can quote the file). Counts and the limit are fine.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use dictation_core::history::{self, Entry};
use dictation_ports::{HistoryStore, HistoryStoreError, InsertError, TextInserter};

/// The history file's name in the app's data folder.
pub const HISTORY_FILE: &str = "history.json";

/// What can be dated: milliseconds since the Unix epoch.
type WallClock = Box<dyn Fn() -> u64 + Send + Sync>;

fn system_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

struct State {
    entries: Vec<Entry>,
    limit: u32,
    /// The last pasted text while history is Off.
    in_memory: Option<String>,
    /// The newest id used in this run, so an id is never reused.
    last_id: u64,
    /// Whether the file has been read. Until it has, it is never written: it
    /// may hold entries this run can't see. See [`History::is_unreadable`].
    loaded: bool,
    /// Whether the last attempt to read the file failed.
    unreadable: bool,
}

/// The recent dictations and the file behind them. Shared with an `Arc`.
///
/// Two locks, always taken in this order: `persisting` is held across file
/// I/O and by everything that changes the history, so changes stay in order;
/// `state` is held only for moments and never across I/O. So reading the
/// history (the tray does, on the main thread) never waits for the disk.
pub struct History {
    store: Box<dyn HistoryStore>,
    now: WallClock,
    persisting: Mutex<()>,
    state: Mutex<State>,
}

impl std::fmt::Debug for History {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the text.
        let state = self.lock();
        f.debug_struct("History")
            .field("entries", &state.entries.len())
            .field("limit", &state.limit)
            .finish_non_exhaustive()
    }
}

impl History {
    /// The history in `store`, kept to `limit` entries: a file over the limit
    /// is trimmed and, at Off (0), deleted.
    ///
    /// `None` means the settings couldn't be read, so the user's choice isn't
    /// known. Then nothing is saved, in case it is Off, and the file is left
    /// alone, in case it isn't, until [`History::set_limit`] says.
    #[must_use]
    pub fn open(store: impl HistoryStore + 'static, limit: Option<u32>) -> Self {
        Self::with_clock(store, limit, Box::new(system_now))
    }

    /// [`History::open`] with its clock given (tests).
    #[must_use]
    pub fn with_clock(
        store: impl HistoryStore + 'static,
        limit: Option<u32>,
        now: WallClock,
    ) -> Self {
        let history = Self {
            store: Box::new(store),
            now,
            persisting: Mutex::new(()),
            state: Mutex::new(State {
                entries: Vec::new(),
                limit: 0,
                in_memory: None,
                last_id: 0,
                loaded: false,
                unreadable: false,
            }),
        };
        if let Some(limit) = limit
            && let Err(error) = history.set_limit(limit)
        {
            tracing::warn!(%error, "could not bring the history to its limit");
        }
        let (entries, limit) = {
            let state = history.lock();
            (state.entries.len(), state.limit)
        };
        tracing::info!(entries, limit, "history");
        history
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_persisting(&self) -> MutexGuard<'_, ()> {
        self.persisting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Reads the file and adds what it holds to what is in memory; `false` if
    /// it can't be read. A file that is damaged, from a newer version or
    /// failing to read may still hold the user's entries, so it is then left
    /// exactly as it is and not written: entries stay in memory, and the next
    /// dictation or limit change tries again. Only Clear or Off remove it.
    /// Call with `persisting` held.
    fn load(&self) -> bool {
        let read = match self.store.read() {
            Ok(None) => Some(Vec::new()),
            Ok(Some(json)) => history::from_json(&json).or_else(|| {
                // No parser message: it could quote the file.
                tracing::warn!("the history file can't be read; not saving over it");
                None
            }),
            Err(error) => {
                tracing::warn!(%error, "could not read the history; not saving over it");
                None
            }
        };
        let mut state = self.lock();
        let Some(stored) = read else {
            state.unreadable = true;
            return false;
        };
        // What waited in memory is newer than the file. New ids put it on top
        // and can't collide with the file's, whatever the clock did.
        let waiting = std::mem::replace(&mut state.entries, stored);
        for mut entry in waiting.into_iter().rev() {
            entry.id = history::next_id(&state.entries, state.last_id, entry.id);
            state.last_id = entry.id;
            state.entries.insert(0, entry);
        }
        state.loaded = true;
        state.unreadable = false;
        true
    }

    fn save(&self, entries: &[Entry]) -> Result<(), HistoryStoreError> {
        let json = history::to_json(entries)
            .ok_or_else(|| HistoryStoreError("the entries can't be written".to_owned()))?;
        self.store.write(&json)
    }

    /// Whether "Copy last dictation" has something to copy.
    #[must_use]
    pub fn has_last(&self) -> bool {
        let state = self.lock();
        !state.entries.is_empty() || state.in_memory.is_some()
    }

    /// The text "Copy last dictation" copies: the newest entry, or the one
    /// in memory.
    #[must_use]
    pub fn last_text(&self) -> Option<String> {
        let state = self.lock();
        state
            .entries
            .first()
            .map(|entry| entry.text.clone())
            .or_else(|| state.in_memory.clone())
    }

    /// The entries, newest first.
    #[must_use]
    pub fn entries(&self) -> Vec<Entry> {
        self.lock().entries.clone()
    }

    /// The text of entry `id`, if it is still there.
    #[must_use]
    pub fn text_of(&self, id: u64) -> Option<String> {
        self.lock()
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.text.clone())
    }

    /// Whether a history file exists that can't be read, so nothing is being
    /// saved: new dictations stay in memory until it can be, or until Clear
    /// or Off deletes it.
    #[must_use]
    pub fn is_unreadable(&self) -> bool {
        self.lock().unreadable
    }

    /// Keeps a finished dictation. The smart leading space is dropped: it
    /// belongs to where the text was pasted. A write that fails is logged and
    /// the entry stays in memory, so the next write carries it.
    pub fn record(&self, text: &str, engine: &str, pasted: bool) {
        let text = text.trim_start_matches(' ');
        if text.trim().is_empty() {
            return;
        }
        let _persisting = self.lock_persisting();
        let loaded = {
            let mut state = self.lock();
            if state.limit == 0 {
                // As U5: a failed paste keeps the one before.
                if pasted {
                    state.in_memory = Some(text.to_owned());
                }
                return;
            }
            state.in_memory = None;
            let now = (self.now)();
            let id = history::next_id(&state.entries, state.last_id, now);
            state.last_id = id;
            let limit = keep(state.limit);
            history::push(
                &mut state.entries,
                Entry {
                    id,
                    text: text.to_owned(),
                    created_at: now,
                    engine: engine.to_owned(),
                    pasted,
                },
                limit,
            );
            state.loaded
        };
        if !loaded && !self.load() {
            return;
        }
        let entries = {
            let mut state = self.lock();
            let limit = keep(state.limit);
            state.entries.truncate(limit);
            state.entries.clone()
        };
        if let Err(error) = self.save(&entries) {
            tracing::warn!(%error, "could not save the history");
        }
    }

    /// Deletes one entry; the list changes only if the file did. An entry
    /// already gone is not an error. Until the file can be read the entries
    /// are only in memory, and the file isn't touched.
    ///
    /// # Errors
    ///
    /// If the file can't be changed.
    pub fn delete(&self, id: u64) -> Result<(), HistoryStoreError> {
        let _persisting = self.lock_persisting();
        let (kept, loaded) = {
            let state = self.lock();
            if !state.entries.iter().any(|entry| entry.id == id) {
                return Ok(());
            }
            let kept: Vec<Entry> = state
                .entries
                .iter()
                .filter(|entry| entry.id != id)
                .cloned()
                .collect();
            (kept, state.loaded)
        };
        if loaded {
            if kept.is_empty() {
                self.store.delete()?;
            } else {
                self.save(&kept)?;
            }
        }
        self.lock().entries = kept;
        Ok(())
    }

    /// Deletes every entry and the file, even one that can't be read, and
    /// forgets any text in memory, so "Copy last dictation" has nothing left.
    ///
    /// # Errors
    ///
    /// If the file can't be deleted; the entries then stay.
    pub fn clear(&self) -> Result<(), HistoryStoreError> {
        let _persisting = self.lock_persisting();
        self.lock().in_memory = None;
        self.store.delete()?;
        let mut state = self.lock();
        state.entries.clear();
        state.loaded = true;
        state.unreadable = false;
        Ok(())
    }

    /// Keeps at most `limit` entries from now on, trimming the oldest at
    /// once. Off (0) deletes them all and the file, even one that can't be
    /// read, and saves nothing more.
    ///
    /// # Errors
    ///
    /// If the file can't be changed. The new limit holds anyway, and the file
    /// is put right by the next write or at the next launch.
    pub fn set_limit(&self, limit: u32) -> Result<(), HistoryStoreError> {
        let _persisting = self.lock_persisting();
        let was_loaded = {
            let mut state = self.lock();
            state.limit = limit;
            if limit == 0 {
                state.entries.clear();
                state.in_memory = None;
            }
            state.loaded
        };
        if limit == 0 {
            self.store.delete()?;
            let mut state = self.lock();
            state.loaded = true;
            state.unreadable = false;
            return Ok(());
        }
        // Entries kept in memory while the file couldn't be read go in it now.
        let waiting = !was_loaded && !self.lock().entries.is_empty();
        if !was_loaded && !self.load() {
            let mut state = self.lock();
            state.entries.truncate(keep(limit));
            return Ok(());
        }
        let trimmed = {
            let mut state = self.lock();
            let over = state.entries.len() > keep(limit);
            state.entries.truncate(keep(limit));
            over
        };
        if trimmed || waiting {
            return self.save(&self.entries());
        }
        Ok(())
    }
}

/// The limit as a length.
fn keep(limit: u32) -> usize {
    usize::try_from(limit).unwrap_or(usize::MAX)
}

/// A [`TextInserter`] that keeps what it was given in the history.
pub struct Recording<I> {
    inner: I,
    history: std::sync::Arc<History>,
    engine: Box<dyn Fn() -> String + Send + Sync>,
}

impl<I> Recording<I> {
    /// Wraps `inner`; dictations go to `history` under the name `engine`
    /// gives at the time.
    pub fn new(
        inner: I,
        history: std::sync::Arc<History>,
        engine: impl Fn() -> String + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner,
            history,
            engine: Box::new(engine),
        }
    }
}

impl<I: TextInserter> TextInserter for Recording<I> {
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        let result = self.inner.insert(text);
        match &result {
            // A password field has focus: these words are never kept.
            Err(InsertError::SecureInput) => {}
            outcome => self.history.record(text, &(self.engine)(), outcome.is_ok()),
        }
        result
    }

    /// Forwarded: the wrapped inserter's quit-time clipboard restore must
    /// run, not the trait's no-op default (#241).
    fn finish(&self) {
        self.inner.finish();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use dictation_platform::fakes::{FakeInserter, MemHistoryStore};

    use super::*;

    /// A clock that moves on by a second at each reading.
    fn ticking() -> WallClock {
        let now = AtomicU64::new(1_000);
        Box::new(move || now.fetch_add(1_000, Ordering::Relaxed))
    }

    fn open(store: &MemHistoryStore, limit: u32) -> History {
        History::with_clock(store.clone(), Some(limit), ticking())
    }

    fn texts(history: &History) -> Vec<String> {
        history.entries().into_iter().map(|e| e.text).collect()
    }

    fn stored_texts(store: &MemHistoryStore) -> Vec<String> {
        store.contents().map_or_else(Vec::new, |json| {
            history::from_json(&json)
                .unwrap()
                .into_iter()
                .map(|entry| entry.text)
                .collect()
        })
    }

    fn fill(history: &History, count: usize) {
        for n in 0..count {
            history.record(&format!("dictation {n}"), "Groq", true);
        }
    }

    #[test]
    fn criterion_2_a_dictation_is_stored_with_every_field() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        history.record("hello there", "On this Mac (Parakeet English)", true);
        let entries = history::from_json(&store.contents().unwrap()).unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.text, "hello there");
        assert_eq!(entry.engine, "On this Mac (Parakeet English)");
        assert_eq!(entry.created_at, 1_000);
        assert!(entry.pasted);
        assert!(entry.id > 0);
    }

    #[test]
    fn criterion_2_the_list_never_exceeds_the_limit_and_the_oldest_go_first() {
        let store = MemHistoryStore::new();
        let history = open(&store, 10);
        fill(&history, 13);
        assert_eq!(history.entries().len(), 10);
        assert_eq!(texts(&history)[0], "dictation 12");
        assert_eq!(texts(&history)[9], "dictation 3");
        assert_eq!(stored_texts(&store), texts(&history));
    }

    #[test]
    fn criterion_2_empty_text_is_not_stored_and_the_smart_space_is_dropped() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        history.record("", "Groq", true);
        history.record("   ", "Groq", true);
        assert!(!history.has_last());
        assert_eq!(store.contents(), None);
        history.record(" and again", "Groq", true);
        assert_eq!(history.last_text().as_deref(), Some("and again"));
    }

    #[test]
    fn criterion_2_the_entries_come_back_after_a_relaunch_up_to_the_limit() {
        let store = MemHistoryStore::new();
        fill(&open(&store, 25), 3);
        let again = open(&store, 25);
        assert_eq!(texts(&again)[0], "dictation 2");
        assert!(again.has_last());
        // Lowered while it wasn't running: trimmed at launch.
        let lower = {
            let store = MemHistoryStore::new();
            fill(&open(&store, 25), 12);
            store
        };
        let trimmed = open(&lower, 10);
        assert_eq!(trimmed.entries().len(), 10);
        assert_eq!(stored_texts(&lower).len(), 10);
    }

    #[test]
    fn criterion_2_a_failed_paste_is_stored_as_not_pasted() {
        let store = MemHistoryStore::new();
        let history = Arc::new(open(&store, 25));
        let fake = FakeInserter::new();
        let inserter = Recording::new(fake.clone(), Arc::clone(&history), || "Groq".to_owned());
        inserter.insert("worked").unwrap();
        for error in [
            InsertError::Failed("no focus".into()),
            InsertError::PermissionDenied,
        ] {
            fake.fail_next(error);
            assert!(inserter.insert("failed").is_err());
            let newest = history.entries()[0].clone();
            assert_eq!(newest.text, "failed");
            assert!(!newest.pasted);
        }
        assert!(history.entries().last().unwrap().pasted);
        assert_eq!(history.entries().len(), 3);
    }

    /// Lead's decision on the spec: a paste refused by Secure Input keeps
    /// nothing, on disk or in memory, with history on or Off, and the error
    /// still reaches the pipeline.
    #[test]
    fn a_paste_refused_by_secure_input_is_never_kept() {
        for limit in [25, 0] {
            let store = MemHistoryStore::new();
            let history = Arc::new(open(&store, limit));
            let fake = FakeInserter::new();
            let inserter = Recording::new(fake.clone(), Arc::clone(&history), String::new);
            fake.fail_next(InsertError::SecureInput);
            assert_eq!(
                inserter.insert("my password"),
                Err(InsertError::SecureInput)
            );
            assert!(history.entries().is_empty(), "limit {limit}");
            assert!(!history.has_last(), "limit {limit}");
            assert_eq!(history.last_text(), None, "limit {limit}");
            assert_eq!(store.contents(), None, "limit {limit}");
        }
    }

    #[test]
    fn criterion_3_lowering_the_limit_trims_at_once() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        fill(&history, 25);
        history.set_limit(10).unwrap();
        assert_eq!(history.entries().len(), 10);
        assert_eq!(texts(&history)[0], "dictation 24");
        assert_eq!(texts(&history)[9], "dictation 15");
        assert_eq!(stored_texts(&store), texts(&history));
        // Raising it back brings nothing back and the next one is kept.
        history.set_limit(25).unwrap();
        assert_eq!(history.entries().len(), 10);
        history.record("new", "Groq", true);
        assert_eq!(history.entries().len(), 11);
    }

    #[test]
    fn criterion_3_off_deletes_the_entries_and_stores_nothing_more() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        fill(&history, 3);
        history.set_limit(0).unwrap();
        assert!(history.entries().is_empty());
        assert_eq!(store.contents(), None);
        assert!(!history.has_last());
        history.record("not kept", "Groq", true);
        assert!(history.entries().is_empty());
        assert_eq!(store.contents(), None);
    }

    #[test]
    fn criterion_3_a_file_left_behind_is_deleted_when_launched_with_off() {
        let store = MemHistoryStore::new();
        fill(&open(&store, 25), 3);
        let history = open(&store, 0);
        assert_eq!(store.contents(), None);
        assert!(history.entries().is_empty());
    }

    #[test]
    fn criterion_3_clear_deletes_everything() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        fill(&history, 3);
        history.clear().unwrap();
        assert!(history.entries().is_empty());
        assert_eq!(store.contents(), None);
        assert_eq!(history.last_text(), None);
        // The next dictation is kept as usual.
        history.record("next", "Groq", true);
        assert_eq!(stored_texts(&store), ["next"]);
    }

    #[test]
    fn criterion_3_a_failed_clear_or_delete_says_so_and_keeps_the_list_true() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        fill(&history, 2);
        let id = history.entries()[0].id;
        store.set_failing(true);
        assert!(history.clear().is_err());
        assert!(history.delete(id).is_err());
        assert_eq!(history.entries().len(), 2, "the file still holds them");
        store.set_failing(false);
        history.delete(id).unwrap();
        assert_eq!(texts(&history), ["dictation 0"]);
    }

    #[test]
    fn a_failed_off_still_stops_saving_and_the_file_goes_at_the_next_launch() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        fill(&history, 1);
        store.set_failing(true);
        assert!(history.set_limit(0).is_err());
        assert!(history.entries().is_empty());
        history.record("not kept", "Groq", true);
        store.set_failing(false);
        assert_eq!(stored_texts(&store), ["dictation 0"]);
        let _ = open(&store, 0);
        assert_eq!(store.contents(), None);
    }

    #[test]
    fn criterion_5_delete_removes_one_entry_and_the_last_one_removes_the_file() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        fill(&history, 2);
        let entries = history.entries();
        history.delete(entries[0].id).unwrap();
        assert_eq!(stored_texts(&store), ["dictation 0"]);
        // Copy last dictation follows.
        assert_eq!(history.last_text().as_deref(), Some("dictation 0"));
        // Gone already: not an error.
        history.delete(entries[0].id).unwrap();
        history.delete(entries[1].id).unwrap();
        assert_eq!(store.contents(), None);
        assert_eq!(store.deletes(), 1);
    }

    #[test]
    fn criterion_5_text_of_gives_the_full_text() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        let long = "line one\nline two\nline three ".repeat(50);
        history.record(&long, "Groq", true);
        let id = history.entries()[0].id;
        assert_eq!(history.text_of(id).as_deref(), Some(long.as_str()));
        assert_eq!(history.text_of(id + 1), None);
    }

    #[test]
    fn criterion_6_copy_last_is_the_newest_entry_even_if_its_paste_failed() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        assert!(!history.has_last());
        history.record("first", "Groq", true);
        history.record("second", "Groq", false);
        assert_eq!(history.last_text().as_deref(), Some("second"));
    }

    /// U5's behaviour with history Off: memory only, successful pastes only.
    #[test]
    fn criterion_6_with_history_off_copy_last_stays_in_memory_as_in_u5() {
        let store = MemHistoryStore::new();
        let history = Arc::new(open(&store, 0));
        let fake = FakeInserter::new();
        let inserter = Recording::new(fake.clone(), Arc::clone(&history), String::new);
        assert!(!history.has_last());
        inserter.insert("hello there").unwrap();
        assert_eq!(history.last_text().as_deref(), Some("hello there"));
        inserter.insert(" and again").unwrap();
        assert_eq!(history.last_text().as_deref(), Some("and again"));
        fake.fail_next(InsertError::Failed("no focus".into()));
        assert!(inserter.insert("second").is_err());
        assert_eq!(history.last_text().as_deref(), Some("and again"));
        assert_eq!(store.contents(), None);
        assert!(history.entries().is_empty());
        // Not across a relaunch.
        assert!(!open(&store, 0).has_last());
    }

    #[test]
    fn turning_history_on_after_off_keeps_the_text_in_memory_until_the_next_dictation() {
        let store = MemHistoryStore::new();
        let history = open(&store, 0);
        history.record("from memory", "Groq", true);
        history.set_limit(25).unwrap();
        assert_eq!(history.last_text().as_deref(), Some("from memory"));
        assert_eq!(store.contents(), None);
        history.record("kept", "Groq", true);
        assert_eq!(history.last_text().as_deref(), Some("kept"));
        history.delete(history.entries()[0].id).unwrap();
        assert_eq!(history.last_text(), None);
    }

    #[test]
    fn turning_history_off_forgets_the_text_it_deleted() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        history.record("gone", "Groq", true);
        history.set_limit(0).unwrap();
        assert_eq!(history.last_text(), None);
    }

    /// Unreadable settings: the user's choice isn't known, so nothing is
    /// saved (it may be Off) and nothing is deleted (it may not be).
    #[test]
    fn with_an_unknown_limit_nothing_is_saved_and_nothing_is_deleted() {
        let store = MemHistoryStore::new();
        fill(&open(&store, 25), 2);
        let history = History::with_clock(store.clone(), None, ticking());
        history.record("not kept", "Groq", true);
        assert_eq!(store.deletes(), 0);
        assert_eq!(stored_texts(&store).len(), 2);
        // Told later: the file's entries are read first, not overwritten.
        history.set_limit(25).unwrap();
        history.record("kept", "Groq", true);
        assert_eq!(stored_texts(&store).len(), 3);
    }

    /// Round 1 #1: a file this build can't read may hold the user's entries,
    /// so a later dictation must not replace it.
    #[test]
    fn a_file_that_cannot_be_read_is_never_saved_over() {
        for original in [
            "not json",
            r#"{"version":99,"entries":[{"from":"the future"}]}"#,
        ] {
            let store = MemHistoryStore::with_contents(original);
            let history = open(&store, 25);
            assert!(history.is_unreadable(), "{original}");
            history.record("next", "Groq", true);
            history.record("and next", "Groq", false);
            assert_eq!(store.contents().as_deref(), Some(original));
            assert_eq!(store.deletes(), 0);
            // The new ones work for this run, Copy last dictation included.
            assert_eq!(texts(&history), ["and next", "next"]);
            assert_eq!(history.last_text().as_deref(), Some("and next"));
            // Nor do Delete, or a limit change, touch the file.
            history.delete(history.entries()[0].id).unwrap();
            history.set_limit(10).unwrap();
            assert_eq!(store.contents().as_deref(), Some(original));
            assert!(history.is_unreadable());
        }
    }

    #[test]
    fn a_failed_read_is_retried_and_nothing_is_lost() {
        let store = MemHistoryStore::new();
        fill(&open(&store, 25), 2);
        let original = store.contents();
        store.set_failing(true);
        let history = open(&store, 25);
        assert!(history.is_unreadable());
        assert!(history.entries().is_empty());
        history.record("while failing", "Groq", true);
        assert_eq!(store.contents(), original);
        // It reads again: the file's entries and the new ones together.
        store.set_failing(false);
        history.record("after", "Groq", true);
        assert!(!history.is_unreadable());
        let want = ["after", "while failing", "dictation 1", "dictation 0"];
        assert_eq!(texts(&history), want);
        assert_eq!(stored_texts(&store), want);
    }

    /// The test clocks of the two runs start alike, so ids collide here, as
    /// they could after a clock change; neither entry may be lost.
    #[test]
    fn entries_with_the_same_id_as_the_file_are_all_kept() {
        let store = MemHistoryStore::new();
        fill(&open(&store, 25), 1);
        store.set_failing(true);
        let history = open(&store, 25);
        history.record("same id", "Groq", true);
        store.set_failing(false);
        history.record("after", "Groq", true);
        assert_eq!(texts(&history), ["after", "same id", "dictation 0"]);
        let ids: std::collections::BTreeSet<u64> =
            history.entries().iter().map(|entry| entry.id).collect();
        assert_eq!(ids.len(), 3);
    }

    /// A limit change retries too, and saves what was waiting.
    #[test]
    fn a_limit_change_retries_the_read_and_saves_within_the_limit() {
        let store = MemHistoryStore::new();
        fill(&open(&store, 25), 11);
        store.set_failing(true);
        let history = open(&store, 25);
        history.record("while failing", "Groq", true);
        store.set_failing(false);
        history.set_limit(10).unwrap();
        assert!(!history.is_unreadable());
        assert_eq!(history.entries().len(), 10);
        assert_eq!(texts(&history)[0], "while failing");
        assert_eq!(stored_texts(&store), texts(&history));
    }

    #[test]
    fn clear_and_off_still_delete_a_file_that_cannot_be_read() {
        let store = MemHistoryStore::with_contents("not json");
        let history = open(&store, 25);
        history.record("next", "Groq", true);
        history.clear().unwrap();
        assert_eq!(store.contents(), None);
        assert!(!history.is_unreadable());
        // And saving works again.
        history.record("saved", "Groq", true);
        assert_eq!(stored_texts(&store), ["saved"]);

        let store = MemHistoryStore::with_contents("not json");
        let history = open(&store, 25);
        history.set_limit(0).unwrap();
        assert_eq!(store.contents(), None);
        assert!(!history.is_unreadable());
        assert!(!history.has_last());

        // A launch with Off deletes it as well: the user chose that.
        let store = MemHistoryStore::with_contents("not json");
        let _ = open(&store, 0);
        assert_eq!(store.contents(), None);
    }

    #[test]
    fn a_failed_clear_of_an_unreadable_file_still_says_it_is_unreadable() {
        let store = MemHistoryStore::with_contents("not json");
        let history = open(&store, 25);
        store.set_failing(true);
        assert!(history.clear().is_err());
        assert!(history.is_unreadable());
        store.set_failing(false);
        assert_eq!(store.contents().as_deref(), Some("not json"));
    }

    /// A store whose first write stays in progress until the test lets it go.
    struct PausedWrite {
        started: Mutex<std::sync::mpsc::Sender<()>>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl HistoryStore for PausedWrite {
        fn read(&self) -> Result<Option<String>, HistoryStoreError> {
            Ok(None)
        }

        fn write(&self, _contents: &str) -> Result<(), HistoryStoreError> {
            let _ = self.started.lock().unwrap().send(());
            let _ = self.release.lock().unwrap().recv();
            Ok(())
        }

        fn delete(&self) -> Result<(), HistoryStoreError> {
            Ok(())
        }
    }

    /// Round 1 #2: the tray reads the history on the main thread, so a read
    /// must finish while a write is stuck on the disk.
    #[test]
    fn reading_the_history_never_waits_for_a_write() {
        let (started_send, started) = std::sync::mpsc::channel();
        let (release, release_receive) = std::sync::mpsc::channel();
        let history = Arc::new(History::open(
            PausedWrite {
                started: Mutex::new(started_send),
                release: Mutex::new(release_receive),
            },
            Some(25),
        ));
        let writer = {
            let history = Arc::clone(&history);
            std::thread::spawn(move || history.record("words", "Groq", true))
        };
        let wait = std::time::Duration::from_secs(10);
        started.recv_timeout(wait).unwrap();
        // On threads of their own, so a regression fails instead of hanging.
        let (done, finished) = std::sync::mpsc::channel();
        let reader = {
            let history = Arc::clone(&history);
            std::thread::spawn(move || {
                let id = history.entries()[0].id;
                let seen = (
                    history.has_last(),
                    history.last_text(),
                    history.text_of(id),
                    history.is_unreadable(),
                );
                let _ = done.send(seen);
            })
        };
        let seen = finished.recv_timeout(wait);
        release.send(()).unwrap();
        writer.join().unwrap();
        reader.join().unwrap();
        let word = Some("words".to_owned());
        assert_eq!(seen.unwrap(), (true, word.clone(), word, false));
    }

    #[test]
    fn a_failed_write_keeps_the_entry_and_the_next_write_carries_it() {
        let store = MemHistoryStore::new();
        let history = open(&store, 25);
        store.set_failing(true);
        history.record("one", "Groq", false);
        assert_eq!(history.last_text().as_deref(), Some("one"));
        store.set_failing(false);
        history.record("two", "Groq", true);
        assert_eq!(stored_texts(&store), ["two", "one"]);
    }

    #[test]
    fn ids_stay_unique_when_the_clock_stands_still_or_after_a_clear() {
        let store = MemHistoryStore::new();
        let history = History::with_clock(store, Some(25), Box::new(|| 5));
        history.record("one", "Groq", true);
        let first = history.entries()[0].id;
        history.clear().unwrap();
        history.record("two", "Groq", true);
        assert!(history.entries()[0].id > first);
    }

    #[test]
    fn the_inserter_passes_finish_through() {
        let fake = FakeInserter::new();
        let history = Arc::new(open(&MemHistoryStore::new(), 25));
        Recording::new(fake.clone(), Arc::clone(&history), String::new).finish();
        assert_eq!(fake.finished(), 1);
        // Exactly once more through the boxed inserter the app really wraps.
        let boxed: Box<dyn TextInserter> = Box::new(fake.clone());
        Recording::new(boxed, history, String::new).finish();
        assert_eq!(fake.finished(), 2);
    }

    #[test]
    fn the_engine_name_is_the_one_at_the_time() {
        let history = Arc::new(open(&MemHistoryStore::new(), 25));
        let inserter = Recording::new(FakeInserter::new(), Arc::clone(&history), || {
            "OpenAI".to_owned()
        });
        inserter.insert("words").unwrap();
        assert_eq!(history.entries()[0].engine, "OpenAI");
    }

    /// A log sink the test reads back.
    #[derive(Clone, Default)]
    struct LogBuffer(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for LogBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Criterion 4: everything history does, logged at every level, including
    /// its failures and a damaged file holding the text, shows neither an
    /// entry's text nor its length. The positive control proves the log works.
    #[test]
    fn criterion_4_no_log_line_at_any_level_holds_entry_text_or_its_length() {
        const SECRET: &str = "zebra quantum marmalade sentinel words!";
        let length = SECRET.len().to_string();
        let buffer = LogBuffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .without_time()
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            let store = MemHistoryStore::new();
            let history = Arc::new(open(&store, 25));
            let fake = FakeInserter::new();
            let inserter = Recording::new(fake.clone(), Arc::clone(&history), String::new);
            inserter.insert(SECRET).unwrap();
            fake.fail_next(InsertError::Failed("no focus".into()));
            let _ = inserter.insert(SECRET);
            fake.fail_next(InsertError::SecureInput);
            let _ = inserter.insert(SECRET);
            store.set_failing(true);
            history.record(SECRET, "Groq", true);
            let id = history.entries()[0].id;
            let _ = history.delete(id);
            let _ = history.clear();
            let _ = history.set_limit(10);
            let _ = history.set_limit(0);
            store.set_failing(false);
            let _ = format!("{history:?}");
            // serde's message for this would quote the text.
            let damaged = format!(r#"{{"version":1,"entries":[{{"id":"{SECRET}"}}]}}"#);
            let reopened = open(&MemHistoryStore::with_contents(&damaged), 25);
            tracing::debug!(?reopened, "control");
            assert_eq!(history.entries().len(), 0);
        });
        let log = String::from_utf8(buffer.0.lock().unwrap().clone()).unwrap();
        assert!(log.contains("could not save the history"), "{log}");
        assert!(log.contains("the history file can't be read"), "{log}");
        assert!(log.contains("control"), "{log}");
        assert!(!log.contains("zebra"), "{log}");
        assert!(!log.contains("sentinel"), "{log}");
        assert!(
            !log.split(|c: char| !c.is_ascii_digit())
                .any(|n| n == length),
            "a length in: {log}"
        );
    }
}
