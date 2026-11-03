//! In-memory fakes of every port except `Transcriber` (its mock lives in
//! `transcribe`). Each is cheap to clone; clones share state, so a test keeps
//! one handle to script or inspect the fake while the pipeline owns another.

mod clipboard;
mod clock;
mod cursor;
mod error_reporter;
mod hotkey;
mod inserter;
mod launch;
mod opener;
mod permissions;
mod secrets;
mod telemetry;

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Lives in [`crate::audio`] now, since the shell's `--debug-audio-file`
/// test hook uses it outside tests; still here for the tests that use it.
pub use crate::audio::FileAudioSource;
pub use clipboard::MemClipboard;
pub use clock::FakeClock;
pub use cursor::FakeTextBeforeCursor;
pub use error_reporter::FakeErrorReporter;
pub use hotkey::FakeHotkey;
pub use inserter::FakeInserter;
pub use launch::FakeLaunchAtLogin;
pub use opener::FakeOpener;
pub use permissions::FakePermissions;
pub use secrets::MemSecretStore;
pub use telemetry::FakeTelemetry;

/// Locks `mutex`, ignoring poisoning: a panicking test thread shouldn't make
/// every later call on a shared fake panic too.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
