//! Traits for the boundaries to the OS and the network: hotkey, audio, transcriber, text inserter, clipboard, secrets, permissions, clock.

#![forbid(unsafe_code)]

/// THROWAWAY: deliberate clippy warning (needless_return). Never merge.
pub fn needless(x: &u32) -> u32 {
    return *x;
}

/// THROWAWAY: deliberate unwrap in non-test code. Never merge.
#[must_use]
pub fn parse(s: &str) -> u32 {
    s.parse().unwrap()
}
