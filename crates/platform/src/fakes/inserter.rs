use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use dictation_ports::{InsertError, TextInserter};

use super::lock;

/// A [`TextInserter`] that records what it was asked to insert.
#[derive(Debug, Clone, Default)]
pub struct FakeInserter {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    inserted: Vec<String>,
    failures: VecDeque<InsertError>,
    finished: u32,
}

impl FakeInserter {
    /// An inserter that succeeds until told otherwise.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every text inserted so far, oldest first. Failed inserts aren't
    /// listed.
    #[must_use]
    pub fn inserted(&self) -> Vec<String> {
        lock(&self.state).inserted.clone()
    }

    /// How many times [`TextInserter::finish`] was called.
    #[must_use]
    pub fn finished(&self) -> u32 {
        lock(&self.state).finished
    }

    /// Makes the next insert fail with `error`. Queued failures are used in
    /// order, one per call.
    pub fn fail_next(&self, error: InsertError) {
        lock(&self.state).failures.push_back(error);
    }
}

impl TextInserter for FakeInserter {
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        let mut state = lock(&self.state);
        if let Some(error) = state.failures.pop_front() {
            return Err(error);
        }
        state.inserted.push(text.to_owned());
        Ok(())
    }

    fn finish(&self) {
        lock(&self.state).finished += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_inserted_text_in_order() {
        let inserter = FakeInserter::new();
        let handle = inserter.clone();
        inserter.insert("hello").unwrap();
        inserter.insert("world").unwrap();
        assert_eq!(handle.inserted(), ["hello", "world"]);
    }

    #[test]
    fn queued_failures_insert_nothing_then_it_recovers() {
        let inserter = FakeInserter::new();
        inserter.fail_next(InsertError::SecureInput);
        inserter.fail_next(InsertError::PermissionDenied);
        assert_eq!(inserter.insert("a"), Err(InsertError::SecureInput));
        assert_eq!(inserter.insert("b"), Err(InsertError::PermissionDenied));
        inserter.insert("c").unwrap();
        assert_eq!(inserter.inserted(), ["c"]);
    }
}
