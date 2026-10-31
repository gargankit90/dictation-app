//! Audio sources behind [`AudioSource`](dictation_ports::AudioSource).
//!
//! [`SystemMicrophone`] is this OS's microphone ([`MacMicrophone`] on
//! macOS). [`FileAudioSource`] plays a WAV file instead, for tests and the
//! shell's debug-only `--debug-audio-file` hook.

mod file;
mod system;

#[cfg(target_os = "macos")]
mod macos;

use std::sync::{Mutex, MutexGuard, PoisonError};

pub use file::FileAudioSource;
#[cfg(target_os = "macos")]
pub use macos::MacMicrophone;
pub use system::SystemMicrophone;

/// Locks `mutex`, ignoring poisoning: a panic elsewhere shouldn't make every
/// later call panic too.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
