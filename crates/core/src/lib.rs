//! Pure dictation logic: coordinator state machine, text cleanup, audio maths, settings, onboarding steps. No I/O, OS or Tauri.

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]

pub mod audio_math;
pub mod coordinator;
pub mod error;
pub mod events;
pub mod hotkey_spec;
pub mod local_models;
pub mod onboarding;
pub mod redact;
pub mod retry;
pub mod settings;
pub mod text;
pub mod wav;
