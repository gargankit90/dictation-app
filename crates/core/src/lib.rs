//! Pure dictation logic: coordinator state machine, text cleanup, audio maths, settings. No I/O, OS or Tauri.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod coordinator;
pub mod error;
pub mod events;
pub mod hotkey_spec;
