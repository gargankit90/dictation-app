//! OS adapters behind the ports (macOS and Windows) and fakes for tests. The only crate allowed to use unsafe and cfg(target_os).
//!
//! The [`fakes`] module exists only with the `testing` feature (and in this
//! crate's own unit tests), so no fake can end up in a release build:
#![cfg_attr(
    not(feature = "testing"),
    doc = "```compile_fail\nuse dictation_platform::fakes::FakeClock;\n```"
)]
#![cfg_attr(
    feature = "testing",
    doc = "```\nuse dictation_platform::fakes::FakeClock;\n```"
)]

#[cfg(any(test, feature = "testing"))]
pub mod fakes;

pub mod audio;
pub mod clock;
pub mod hotkey;
pub mod permissions;
pub mod secrets;
pub mod secure_input;
