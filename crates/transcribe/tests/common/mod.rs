//! Shared by the leak tests: capture every log line at every level.

#![allow(dead_code, reason = "each test binary uses part of this")]
#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::io::Write;
use std::sync::{Arc, Mutex};

/// A `MakeWriter` that appends everything to a shared buffer.
#[derive(Clone, Default)]
pub struct Captured(pub Arc<Mutex<Vec<u8>>>);

impl Captured {
    /// Logs everything (trace and up) on this thread until the guard drops.
    pub fn install() -> (Self, tracing::subscriber::DefaultGuard) {
        let captured = Self::default();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(captured.clone())
            .finish();
        (captured, tracing::subscriber::set_default(subscriber))
    }

    /// Everything logged so far.
    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
