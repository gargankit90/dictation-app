//! The last dictation, for the menu's "Copy last dictation" (U5).
//!
//! [`Remembering`] wraps the text inserter: when an insert succeeds, the text
//! the pipeline handed it (the final, cleaned transcript, not whatever the
//! clipboard holds) is kept in [`LastDictation`]. It lives in memory only:
//! never written to disk, logged, shown in the menu, reported or sent as an
//! analytics event, and gone when the app quits. Neither type prints the text
//! in `Debug`.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dictation_ports::{InsertError, TextInserter};

/// The text of the last successful dictation in this run. Clones share it.
#[derive(Clone, Default)]
pub struct LastDictation {
    text: Arc<Mutex<Option<String>>>,
}

impl std::fmt::Debug for LastDictation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the text.
        f.debug_struct("LastDictation")
            .field("held", &self.is_held())
            .finish()
    }
}

impl LastDictation {
    fn lock(&self) -> MutexGuard<'_, Option<String>> {
        self.text.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether a dictation succeeded in this run.
    #[must_use]
    pub fn is_held(&self) -> bool {
        self.lock().is_some()
    }

    /// Keeps `text` as the last dictation, replacing the one before.
    pub fn set(&self, text: &str) {
        *self.lock() = Some(text.to_owned());
    }

    /// The text to copy, if a dictation succeeded. The smart leading space
    /// the pipeline adds after a letter is dropped: it belongs to where the
    /// text was pasted.
    #[must_use]
    pub fn text(&self) -> Option<String> {
        self.lock()
            .as_deref()
            .map(|text| text.trim_start_matches(' ').to_owned())
    }
}

/// A [`TextInserter`] that remembers what it inserted successfully.
pub struct Remembering<I> {
    inner: I,
    last: LastDictation,
}

impl<I> Remembering<I> {
    /// Wraps `inner`; successful inserts go to `last`.
    pub fn new(inner: I, last: LastDictation) -> Self {
        Self { inner, last }
    }
}

impl<I: TextInserter> TextInserter for Remembering<I> {
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        self.inner.insert(text)?;
        self.last.set(text);
        Ok(())
    }

    /// Forwarded: the wrapped inserter's quit-time clipboard restore must
    /// run, not the trait's no-op default (#241).
    fn finish(&self) {
        self.inner.finish();
    }
}

#[cfg(test)]
mod tests {
    use dictation_platform::fakes::FakeInserter;

    use super::*;

    #[test]
    fn criterion_5_nothing_is_held_until_an_insert_succeeds() {
        let last = LastDictation::default();
        let inserter = Remembering::new(FakeInserter::new(), last.clone());
        assert!(!last.is_held());
        assert_eq!(last.text(), None);
        inserter.insert("hello there").unwrap();
        assert!(last.is_held());
        assert_eq!(last.text().as_deref(), Some("hello there"));
        // The next one replaces it.
        inserter.insert(" and again").unwrap();
        assert_eq!(last.text().as_deref(), Some("and again"));
    }

    #[test]
    fn criterion_5_a_failed_insert_keeps_the_one_before() {
        let last = LastDictation::default();
        let fake = FakeInserter::new();
        let inserter = Remembering::new(fake.clone(), last.clone());
        inserter.insert("first").unwrap();
        fake.fail_next(InsertError::Failed("no focus".into()));
        assert!(inserter.insert("second").is_err());
        assert_eq!(last.text().as_deref(), Some("first"));

        let none = LastDictation::default();
        let failing = FakeInserter::new();
        failing.fail_next(InsertError::SecureInput);
        let inserter = Remembering::new(failing, none.clone());
        assert!(inserter.insert("secret").is_err());
        assert!(!none.is_held());
    }

    #[test]
    fn finish_reaches_the_wrapped_inserter() {
        let fake = FakeInserter::new();
        let inserter = Remembering::new(fake.clone(), LastDictation::default());
        inserter.finish();
        assert_eq!(fake.finished(), 1);
    }

    #[test]
    fn criterion_5_debug_never_prints_the_text() {
        let last = LastDictation::default();
        last.set("my private words");
        let shown = format!("{last:?}");
        assert!(!shown.contains("private"), "{shown}");
    }
}
