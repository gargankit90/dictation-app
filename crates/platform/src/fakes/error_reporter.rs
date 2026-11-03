//! An [`ErrorReporter`] that keeps what it was given.

use std::sync::{Arc, Mutex};

use dictation_core::diagnostics::ErrorCode;
use dictation_core::error::DictationError;
use dictation_ports::ErrorReporter;

use super::lock;

/// Records the codes of reported errors in memory. Clones share the list.
#[derive(Debug, Clone, Default)]
pub struct FakeErrorReporter {
    codes: Arc<Mutex<Vec<ErrorCode>>>,
}

impl FakeErrorReporter {
    /// An empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The codes of everything reported so far, in order.
    #[must_use]
    pub fn codes(&self) -> Vec<ErrorCode> {
        lock(&self.codes).clone()
    }
}

impl ErrorReporter for FakeErrorReporter {
    fn report(&self, error: &DictationError) {
        lock(&self.codes).push(ErrorCode::of(error));
    }
}
