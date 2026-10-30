//! In-memory fakes of every port except `Transcriber` (its mock lives in
//! `transcribe`). Each is cheap to clone; clones share state, so a test keeps
//! one handle to script or inspect the fake while the pipeline owns another.

mod audio;
mod clipboard;
mod clock;
mod hotkey;
mod inserter;
mod permissions;
mod secrets;

use std::sync::{Mutex, MutexGuard, PoisonError};

pub use audio::FileAudioSource;
pub use clipboard::MemClipboard;
pub use clock::FakeClock;
pub use hotkey::FakeHotkey;
pub use inserter::FakeInserter;
pub use permissions::FakePermissions;
pub use secrets::MemSecretStore;

/// Locks `mutex`, ignoring poisoning: a panicking test thread shouldn't make
/// every later call on a shared fake panic too.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
