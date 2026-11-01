//! Pure dictation logic: coordinator state machine, text cleanup, audio maths, settings. No I/O, OS or Tauri.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod audio_math;
pub mod coordinator;
pub mod error;
pub mod events;
pub mod hotkey_spec;
pub mod retry;
pub mod settings;
pub mod text;
pub mod wav;
