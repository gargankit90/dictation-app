//! Orchestration: runs the core coordinator against the ports on an async runtime. No Tauri types.

#![forbid(unsafe_code)]

pub mod config;
pub mod debug_audio;
pub mod hotkey_setting;
pub mod launch;
pub mod mic_test;
pub mod overlay;
pub mod pipeline;
pub mod progress;
pub mod rename;
pub mod stand_ins;
pub mod test_hooks;
pub mod tray;
pub mod ui_events;
pub mod usage;
