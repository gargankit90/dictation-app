//! An in-memory history file.

use std::sync::{Arc, Mutex};

use dictation_ports::{HistoryStore, HistoryStoreError};

use super::lock;

/// A [`HistoryStore`] in memory, shared by clones, that can be told to fail.
#[derive(Clone, Default)]
pub struct MemHistoryStore {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    contents: Option<String>,
    failing: bool,
    deletes: u32,
}

impl MemHistoryStore {
    /// An empty store: no file.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A store whose file holds `contents`.
    #[must_use]
    pub fn with_contents(contents: &str) -> Self {
        let store = Self::new();
        lock(&store.state).contents = Some(contents.to_owned());
        store
    }

    /// What the file holds, if there is one.
    #[must_use]
    pub fn contents(&self) -> Option<String> {
        lock(&self.state).contents.clone()
    }

    /// Makes every later call fail, or work again.
    pub fn set_failing(&self, failing: bool) {
        lock(&self.state).failing = failing;
    }

    /// How many times the file was deleted.
    #[must_use]
    pub fn deletes(&self) -> u32 {
        lock(&self.state).deletes
    }

    fn check(state: &State) -> Result<(), HistoryStoreError> {
        if state.failing {
            return Err(HistoryStoreError("disk full".to_owned()));
        }
        Ok(())
    }
}

impl HistoryStore for MemHistoryStore {
    fn read(&self) -> Result<Option<String>, HistoryStoreError> {
        let state = lock(&self.state);
        Self::check(&state)?;
        Ok(state.contents.clone())
    }

    fn write(&self, contents: &str) -> Result<(), HistoryStoreError> {
        let mut state = lock(&self.state);
        Self::check(&state)?;
        state.contents = Some(contents.to_owned());
        Ok(())
    }

    fn delete(&self) -> Result<(), HistoryStoreError> {
        let mut state = lock(&self.state);
        Self::check(&state)?;
        state.contents = None;
        state.deletes += 1;
        Ok(())
    }
}
