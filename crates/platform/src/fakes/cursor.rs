use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_ports::{BeforeCursor, CursorReadError, TextBeforeCursor};

use super::lock;

/// A [`TextBeforeCursor`] that answers what the test set, optionally after
/// a delay (to make the caller's timeout fire), and counts the reads.
#[derive(Debug, Clone)]
pub struct FakeTextBeforeCursor {
    state: Arc<Mutex<State>>,
}

#[derive(Debug)]
struct State {
    answer: Result<BeforeCursor, CursorReadError>,
    delay: Duration,
    reads: usize,
}

impl FakeTextBeforeCursor {
    /// A reader that always answers `answer`.
    #[must_use]
    pub fn answering(answer: Result<BeforeCursor, CursorReadError>) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                answer,
                delay: Duration::ZERO,
                reads: 0,
            })),
        }
    }

    /// A reader that finds `c` before the cursor.
    #[must_use]
    pub fn char(c: char) -> Self {
        Self::answering(Ok(BeforeCursor::Char(c)))
    }

    /// Answers `answer` from now on.
    pub fn set(&self, answer: Result<BeforeCursor, CursorReadError>) {
        lock(&self.state).answer = answer;
    }

    /// Blocks the calling thread for `delay` before each answer, like an
    /// app that is slow to reply.
    pub fn delay(&self, delay: Duration) {
        lock(&self.state).delay = delay;
    }

    /// How many times it was read.
    #[must_use]
    pub fn reads(&self) -> usize {
        lock(&self.state).reads
    }
}

impl TextBeforeCursor for FakeTextBeforeCursor {
    fn char_before_cursor(&self) -> Result<BeforeCursor, CursorReadError> {
        let (answer, delay) = {
            let mut state = lock(&self.state);
            state.reads += 1;
            (state.answer.clone(), state.delay)
        };
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn answers_what_was_set_and_counts_reads() {
        let reader = FakeTextBeforeCursor::char('a');
        let handle = reader.clone();
        assert_eq!(reader.char_before_cursor(), Ok(BeforeCursor::Char('a')));
        handle.set(Err(CursorReadError::SecureField));
        assert_eq!(
            reader.char_before_cursor(),
            Err(CursorReadError::SecureField)
        );
        assert_eq!(handle.reads(), 2);
    }

    #[test]
    fn a_delay_blocks_before_answering() {
        let reader = FakeTextBeforeCursor::answering(Ok(BeforeCursor::StartOfField));
        reader.delay(Duration::from_millis(30));
        let started = Instant::now();
        assert_eq!(reader.char_before_cursor(), Ok(BeforeCursor::StartOfField));
        assert!(started.elapsed() >= Duration::from_millis(30));
    }
}
