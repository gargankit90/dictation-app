//! The global dictation hotkey.
//!
//! [`HotkeyMatcher`] holds the matching rules and runs everywhere; each OS
//! adapter feeds it that OS's key events.

mod matcher;

pub use matcher::{Hand, Held, HotkeyMatcher, KeyInput};
