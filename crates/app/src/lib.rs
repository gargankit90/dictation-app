//! Orchestration: runs the core coordinator against the ports on an async runtime. No Tauri types.

#![forbid(unsafe_code)]

pub mod config;
pub mod debug_audio;
pub mod hotkey;
pub mod launch;
pub mod overlay;
pub mod pipeline;
pub mod stand_ins;
pub mod test_hooks;
pub mod ui_events;
