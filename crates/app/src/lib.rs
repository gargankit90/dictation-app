//! Orchestration: runs the core coordinator against the ports on an async runtime. No Tauri types.

#![forbid(unsafe_code)]

pub mod pipeline;
pub mod stand_ins;
pub mod test_hooks;
