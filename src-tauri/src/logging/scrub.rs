//! The last line of defence against a key in the log: every formatted log
//! line passes through [`scrub`] before it is written.
//!
//! Keys should never get this far: the code logs through types that can't
//! print them (`Secret`, `redact::Hidden`) and error strings are summarised
//! (`redact::summary`). This catches what slips through, such as a
//! dependency logging a request header at debug level: anything that looks
//! like a Groq or `OpenAI` key or follows `Bearer `
//! ([`dictation_core::redact::secrets`]), and the exact value of every key
//! the app has read or been given ([`dictation_core::redact::register_secret`]).

use std::io::{self, Write};
use std::sync::{Arc, Mutex, PoisonError};

use dictation_core::redact;
use tracing_subscriber::fmt::MakeWriter;

/// `text` with registered keys and key-shaped tokens replaced.
#[must_use]
pub fn scrub(text: &str) -> String {
    redact::secrets(text).into_owned()
}

/// A [`MakeWriter`] that scrubs each log event before it reaches `sink`.
///
/// An event's text is collected whole and cleaned in one go when its writer is
/// dropped, so a key can't escape by being split across two writes.
pub struct ScrubbingWriter<W> {
    sink: Arc<Mutex<W>>,
}

impl<W> Clone for ScrubbingWriter<W> {
    fn clone(&self) -> Self {
        Self {
            sink: Arc::clone(&self.sink),
        }
    }
}

impl<W: Write> ScrubbingWriter<W> {
    /// Wraps `sink`.
    pub fn new(sink: W) -> Self {
        Self {
            sink: Arc::new(Mutex::new(sink)),
        }
    }
}

/// One event's text, on its way to the sink.
pub struct Event<W: Write> {
    text: Vec<u8>,
    sink: Arc<Mutex<W>>,
}

impl<W: Write> Event<W> {
    fn emit(&mut self) -> io::Result<()> {
        if self.text.is_empty() {
            return Ok(());
        }
        let clean = scrub(&String::from_utf8_lossy(&self.text));
        self.text.clear();
        let mut sink = self.sink.lock().unwrap_or_else(PoisonError::into_inner);
        sink.write_all(clean.as_bytes())?;
        sink.flush()
    }
}

impl<W: Write> Write for Event<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.text.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // The event is complete only when the writer is dropped.
        Ok(())
    }
}

impl<W: Write> Drop for Event<W> {
    fn drop(&mut self) {
        // Nowhere to report a failing log file; the app carries on.
        let _ = self.emit();
    }
}

impl<'a, W: Write + 'a> MakeWriter<'a> for ScrubbingWriter<W> {
    type Writer = Event<W>;

    fn make_writer(&'a self) -> Self::Writer {
        Event {
            text: Vec::new(),
            sink: Arc::clone(&self.sink),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sink the test can read back.
    #[derive(Clone, Default)]
    pub struct Shared(pub Arc<Mutex<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn key_shaped_tokens_are_hidden_without_registering() {
        assert_eq!(
            scrub("Authorization: Bearer abcdef0123 and gsk_dummy_NOT_A_REAL_KEY_0123456789"),
            "Authorization: Bearer [key hidden] and [key hidden]"
        );
    }

    #[test]
    fn an_event_is_cleaned_whole_when_its_writer_is_dropped() {
        let sink = Shared::default();
        let writer = ScrubbingWriter::new(sink.clone());
        {
            let mut event = writer.make_writer();
            // A key split across two writes is still caught.
            event.write_all(b"key=gsk_dummy_NOT_A_").unwrap();
            event.write_all(b"REAL_KEY_0123456789 done\n").unwrap();
            event.flush().unwrap();
            assert!(sink.0.lock().unwrap().is_empty(), "nothing written early");
        }
        let _ = writer.clone().make_writer(); // an empty event writes nothing
        assert_eq!(
            String::from_utf8(sink.0.lock().unwrap().clone()).unwrap(),
            "key=[key hidden] done\n"
        );
    }

    #[test]
    fn invalid_utf8_is_replaced_not_dropped() {
        let sink = Shared::default();
        let writer = ScrubbingWriter::new(sink.clone());
        writer.make_writer().write_all(b"ok \xff ok").unwrap();
        assert_eq!(
            String::from_utf8(sink.0.lock().unwrap().clone()).unwrap(),
            "ok \u{fffd} ok"
        );
    }
}
