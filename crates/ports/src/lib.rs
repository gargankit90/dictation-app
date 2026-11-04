//! Traits for the boundaries to the OS and the network: hotkey, audio, transcriber, text inserter, text before the cursor, clipboard, secrets, permissions, launch at login, clock.
//!
//! `platform` implements them for each OS (and as fakes behind its `testing`
//! feature), `transcribe` implements [`Transcriber`], and `app` runs the
//! core coordinator against them.
//!
//! # Async
//!
//! Only [`Transcriber`] is async: it waits on the network for seconds. Every
//! other port is a short synchronous call (a keychain lookup, a clipboard
//! write, starting an audio stream), and each method says whether it may
//! block so the pipeline can move it onto a blocking thread.
//!
//! [`Transcriber::transcribe`] uses native `async fn` in traits, written as
//! `fn … -> impl Future<Output = …> + Send` so every implementation's future
//! is `Send` and can run on a multi-threaded tokio runtime. We chose this
//! over boxed futures (`async-trait` or `Pin<Box<dyn Future>>`) because:
//!
//! - it needs no extra dependency and no allocation per call;
//! - the pipeline is generic over its ports (static dispatch), so it doesn't
//!   need `dyn Transcriber`;
//! - choosing an engine at run time (`--transcriber mock`, Groq, OpenAI) can
//!   be an enum that implements the trait by delegating, which keeps static
//!   dispatch. If a real need for `dyn` comes up, a boxed adapter can be
//!   added next to the trait without changing implementations.
//!
//! The cost: `Transcriber` isn't dyn-compatible (object safe).
//!
//! # Time
//!
//! Times are [`Duration`](std::time::Duration)s since an arbitrary origin
//! fixed by the [`Clock`], the same convention as `dictation_core::events`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod audio;
mod clipboard;
mod clock;
mod cursor;
mod hotkey;
mod insert;
mod launch;
mod level;
mod opener;
mod permissions;
mod secrets;
mod telemetry;
mod transcribe;

pub use audio::{Audio, AudioError, AudioSource};
pub use clipboard::{
    Clipboard, ClipboardError, ClipboardItem, ClipboardMarkers, ClipboardSnapshot, SessionMarker,
};
pub use clock::Clock;
pub use cursor::{BeforeCursor, CursorReadError, TextBeforeCursor};
pub use dictation_core::error::Permission;
pub use dictation_core::text::Transcript;
pub use hotkey::{HotkeyError, HotkeyEvent, HotkeySink, HotkeySource};
pub use insert::{InsertError, TextInserter};
pub use launch::{LaunchAtLogin, LaunchAtLoginError, LaunchAtLoginState};
pub use level::LevelMeter;
pub use opener::Opener;
pub use permissions::{PermissionError, PermissionStatus, Permissions};
pub use secrets::{Secret, SecretError, SecretStore};
pub use telemetry::{
    Arch, DurationBucket, EngineChoice, EngineKind, ErrorReporter, ErrorType, Event, LatencyBucket,
    NoErrorReporter, NoTelemetry, OnboardingStep, SettingName, Telemetry, Version,
};
pub use transcribe::{TranscribeError, Transcriber};
