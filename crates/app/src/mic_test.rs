//! Settings' "Test microphone" (U6): records from a microphone of its own
//! for a few seconds, only to show the live input level (U4's meter) and
//! say "No sound" if it stays quiet. The audio is dropped as it arrives:
//! nothing is kept, transcribed, logged or sent. Only the level (a number)
//! and the device's name leave this module.
//!
//! The test uses its own [`AudioSource`], not the pipeline's, so it can't
//! start, stop or swallow a dictation. A test ends when the window stops it,
//! when another test starts, or by itself after [`MAX_TEST`], so a window
//! closed mid-test can't leave the microphone on.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use dictation_ports::{AudioError, AudioSource, LevelMeter};

/// The longest a test records, even if nobody stops it. The window shows
/// five seconds; the rest is slack for a slow stop.
pub const MAX_TEST: Duration = Duration::from_secs(7);

/// Makes the microphone a test records from (a new one each time).
pub type MakeSource = Box<dyn Fn() -> Box<dyn AudioSource> + Send + Sync>;

/// What a running test reads: its level since the last read and the device.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// The level since the last reading, in dBFS; `None` once no test runs.
    pub level: Option<f32>,
    /// The input device's name, if the source knows it.
    pub device: Option<String>,
}

/// The microphone test. Clones share it.
#[derive(Clone)]
pub struct MicTest {
    make: Arc<MakeSource>,
    running: Arc<Mutex<Running>>,
}

#[derive(Default)]
struct Running {
    /// Bumped by every start and stop, so the time limit of an older test
    /// can't stop a newer one.
    generation: u64,
    test: Option<Test>,
}

struct Test {
    source: Box<dyn AudioSource>,
    meter: Option<LevelMeter>,
}

impl Drop for Test {
    fn drop(&mut self) {
        // The samples are never read; dropping them is the point.
        self.source.discard();
        if let Some(meter) = &self.meter {
            meter.end();
        }
    }
}

fn lock(running: &Mutex<Running>) -> MutexGuard<'_, Running> {
    running.lock().unwrap_or_else(PoisonError::into_inner)
}

impl MicTest {
    /// A test that records from sources `make` returns.
    #[must_use]
    pub fn new(make: MakeSource) -> Self {
        Self {
            make: Arc::new(make),
            running: Arc::default(),
        }
    }

    /// Starts a test (ending any running one) and returns its generation,
    /// for [`stop_after`](Self::stop_after). Blocks briefly while the device
    /// opens.
    ///
    /// # Errors
    ///
    /// The source's: no permission, no device, or a device failure. Then no
    /// test runs.
    pub fn start(&self) -> Result<u64, AudioError> {
        let mut running = lock(&self.running);
        running.generation += 1;
        running.test = None;
        let mut source = (self.make)();
        let meter = source.level_meter();
        source.start()?;
        running.test = Some(Test { source, meter });
        Ok(running.generation)
    }

    /// Ends the test of `generation` after `limit`, on a thread of its own,
    /// unless a newer test or a stop came first.
    pub fn stop_after(&self, generation: u64, limit: Duration) {
        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("mic-test-limit".to_owned())
            .spawn(move || {
                std::thread::sleep(limit);
                let mut running = lock(&this.running);
                if running.generation == generation {
                    running.test = None;
                }
            });
        if let Err(error) = spawned {
            // Without the limit, the window's own stop still ends it.
            tracing::warn!(%error, "could not start the microphone test's time limit");
        }
    }

    /// Ends the running test, if any.
    pub fn stop(&self) {
        let mut running = lock(&self.running);
        running.generation += 1;
        running.test = None;
    }

    /// The level since the last reading, and the device. Never blocks long.
    #[must_use]
    pub fn read(&self) -> Reading {
        let running = lock(&self.running);
        let meter = running.test.as_ref().and_then(|test| test.meter.as_ref());
        Reading {
            level: meter.and_then(LevelMeter::take),
            device: meter.and_then(LevelMeter::device),
        }
    }

    /// Whether a test runs.
    #[must_use]
    pub fn is_running(&self) -> bool {
        lock(&self.running).test.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use dictation_ports::Audio;

    use super::*;

    /// A microphone that feeds a constant level into its meter.
    struct FakeMic {
        meter: LevelMeter,
        recording: bool,
        refuse: Option<AudioError>,
        open: Arc<AtomicUsize>,
    }

    impl AudioSource for FakeMic {
        fn start(&mut self) -> Result<(), AudioError> {
            if let Some(error) = self.refuse.clone() {
                return Err(error);
            }
            self.recording = true;
            self.open.fetch_add(1, Ordering::SeqCst);
            self.meter.begin(Some("Test Microphone".to_owned()));
            self.meter.record(&[0.5; 160]);
            Ok(())
        }
        fn stop(&mut self) -> Result<Audio, AudioError> {
            self.discard();
            Ok(Audio {
                sample_rate: 16_000,
                samples: Vec::new(),
            })
        }
        fn discard(&mut self) {
            if self.recording {
                self.open.fetch_sub(1, Ordering::SeqCst);
            }
            self.recording = false;
            self.meter.end();
        }
        fn is_recording(&self) -> bool {
            self.recording
        }
        fn level_meter(&self) -> Option<LevelMeter> {
            Some(self.meter.clone())
        }
    }

    fn rig(refuse: Option<AudioError>) -> (MicTest, Arc<AtomicUsize>) {
        let open = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&open);
        let test = MicTest::new(Box::new(move || {
            Box::new(FakeMic {
                meter: LevelMeter::new(),
                recording: false,
                refuse: refuse.clone(),
                open: Arc::clone(&counter),
            })
        }));
        (test, open)
    }

    #[test]
    fn a_test_reads_the_level_and_the_device_then_stops() {
        let (test, open) = rig(None);
        test.start().unwrap();
        let reading = test.read();
        assert!(reading.level.unwrap() > -10.0, "{reading:?}");
        assert_eq!(reading.device.as_deref(), Some("Test Microphone"));
        assert_eq!(open.load(Ordering::SeqCst), 1);
        test.stop();
        assert!(!test.is_running());
        assert_eq!(open.load(Ordering::SeqCst), 0, "the microphone is closed");
        assert_eq!(test.read().level, None);
    }

    #[test]
    fn a_refused_microphone_is_an_error_and_nothing_runs() {
        let (test, open) = rig(Some(AudioError::PermissionDenied));
        assert_eq!(test.start(), Err(AudioError::PermissionDenied));
        assert!(!test.is_running());
        assert_eq!(open.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_new_test_closes_the_old_microphone() {
        let (test, open) = rig(None);
        test.start().unwrap();
        test.start().unwrap();
        assert_eq!(open.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_time_limit_ends_a_test_nobody_stopped() {
        let (test, open) = rig(None);
        let generation = test.start().unwrap();
        test.stop_after(generation, Duration::from_millis(20));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while test.is_running() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!test.is_running());
        assert_eq!(open.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_old_time_limit_leaves_a_newer_test_running() {
        let (test, _) = rig(None);
        let old = test.start().unwrap();
        test.start().unwrap();
        test.stop_after(old, Duration::ZERO);
        std::thread::sleep(Duration::from_millis(100));
        assert!(test.is_running());
    }
}
