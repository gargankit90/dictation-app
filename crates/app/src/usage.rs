//! Turns what the pipeline shows the UI into usage events (A1).
//!
//! The tracker reads [`UiEvent`]s, the same stream the overlay and tray get,
//! so the pipeline itself knows nothing about telemetry. It keeps only
//! timestamps and a flag; the text of an error is never read, only its kind
//! ([`ErrorType`]). Whatever the transcript, the key or an error's reason
//! holds, it can't reach an [`Event`], which has no field for it.

use std::sync::Arc;
use std::time::Duration;

use dictation_core::coordinator::Phase;
use dictation_ports::{DurationBucket, EngineKind, ErrorType, Event, LatencyBucket, Telemetry};

use crate::pipeline::UiEvent;

/// Observes a dictation's phases and records its outcome.
pub struct UsageTracker {
    telemetry: Arc<dyn Telemetry>,
    engine: EngineKind,
    recording_since: Option<Duration>,
    /// How long the recording was, once it ended.
    recorded: Option<Duration>,
    transcribing_since: Option<Duration>,
    /// The transcription's latency, once it ended; set while a paste may
    /// still fail.
    transcribed: Option<Duration>,
}

impl UsageTracker {
    /// A tracker recording to `telemetry`, for `engine`.
    #[must_use]
    pub fn new(telemetry: Arc<dyn Telemetry>, engine: EngineKind) -> Self {
        Self {
            telemetry,
            engine,
            recording_since: None,
            recorded: None,
            transcribing_since: None,
            transcribed: None,
        }
    }

    /// Sets the engine later dictations count under.
    pub fn set_engine(&mut self, engine: EngineKind) {
        self.engine = engine;
    }

    /// Observes `event`, which happened at `now` (the pipeline clock's time
    /// or any monotonic one).
    pub fn on_event(&mut self, event: &UiEvent, now: Duration) {
        match event {
            UiEvent::StateChanged(Phase::Recording) => {
                self.reset();
                self.recording_since = Some(now);
            }
            UiEvent::StateChanged(Phase::Transcribing) => {
                self.recorded = self.recording_since.take().map(|t| now.saturating_sub(t));
                self.transcribing_since = Some(now);
            }
            UiEvent::StateChanged(Phase::Inserting) => {
                self.transcribed = self
                    .transcribing_since
                    .take()
                    .map(|t| now.saturating_sub(t));
            }
            UiEvent::StateChanged(Phase::Idle) => {
                // A paste that failed showed its error first, which cleared
                // this; so a pending result here is a pasted dictation.
                if let (Some(recorded), Some(latency)) = (self.recorded, self.transcribed) {
                    self.telemetry.record(Event::DictationFinished {
                        engine_kind: self.engine,
                        duration_bucket: DurationBucket::of(recorded),
                        latency_bucket: LatencyBucket::of(latency),
                    });
                }
                self.reset();
            }
            UiEvent::Error(error) => {
                self.telemetry.record(Event::DictationFailed {
                    error_type: ErrorType::from(error),
                });
                self.reset();
            }
            UiEvent::HotkeyRestored => {}
        }
    }

    fn reset(&mut self) {
        self.recording_since = None;
        self.recorded = None;
        self.transcribing_since = None;
        self.transcribed = None;
    }
}

#[cfg(test)]
mod tests {
    use dictation_core::error::DictationError;
    use dictation_platform::fakes::FakeTelemetry;

    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn tracker() -> (UsageTracker, FakeTelemetry) {
        let fake = FakeTelemetry::new();
        (
            UsageTracker::new(Arc::new(fake.clone()), EngineKind::Cloud),
            fake,
        )
    }

    fn phase(tracker: &mut UsageTracker, phase: Phase, at: Duration) {
        tracker.on_event(&UiEvent::StateChanged(phase), at);
    }

    #[test]
    fn a_pasted_dictation_is_one_finished_event_with_buckets() {
        let (mut t, fake) = tracker();
        phase(&mut t, Phase::Recording, secs(10));
        phase(&mut t, Phase::Transcribing, secs(13));
        phase(
            &mut t,
            Phase::Inserting,
            secs(13) + Duration::from_millis(700),
        );
        phase(&mut t, Phase::Idle, secs(14));
        assert_eq!(
            fake.events(),
            [Event::DictationFinished {
                engine_kind: EngineKind::Cloud,
                duration_bucket: DurationBucket::From2To5s,
                latency_bucket: LatencyBucket::From500To1000ms,
            }]
        );
    }

    #[test]
    fn a_failure_is_one_failed_event_and_no_finished_one() {
        let (mut t, fake) = tracker();
        phase(&mut t, Phase::Recording, secs(0));
        phase(&mut t, Phase::Transcribing, secs(1));
        phase(&mut t, Phase::Inserting, secs(2));
        t.on_event(&UiEvent::Error(DictationError::SecureInput), secs(2));
        phase(&mut t, Phase::Idle, secs(2));
        assert_eq!(
            fake.events(),
            [Event::DictationFailed {
                error_type: ErrorType::SecureInput
            }]
        );
    }

    #[test]
    fn a_cancelled_or_empty_dictation_records_nothing() {
        let (mut t, fake) = tracker();
        phase(&mut t, Phase::Recording, secs(0));
        phase(&mut t, Phase::Idle, secs(1));
        phase(&mut t, Phase::Recording, secs(2));
        phase(&mut t, Phase::Transcribing, secs(3));
        phase(&mut t, Phase::Idle, secs(4));
        assert!(fake.events().is_empty());
    }

    #[test]
    fn a_missed_phase_records_nothing_rather_than_a_guess() {
        let (mut t, fake) = tracker();
        phase(&mut t, Phase::Inserting, secs(5));
        phase(&mut t, Phase::Idle, secs(6));
        assert!(fake.events().is_empty());
    }
}
