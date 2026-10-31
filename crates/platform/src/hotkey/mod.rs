//! The global dictation hotkey.
//!
//! [`HotkeyMatcher`] holds the matching rules and runs everywhere; each OS
//! adapter feeds it that OS's key events. macOS: [`MacHotkey`].
//! [`SystemHotkey`] picks this OS's adapter.

mod matcher;
mod system;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub use macos::MacHotkey;
pub use matcher::{Hand, Held, HotkeyMatcher, KeyInput};
pub use system::SystemHotkey;
