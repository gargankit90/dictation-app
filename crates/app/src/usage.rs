//! Turns what the pipeline shows the UI into usage events (A1).
//!
//! The tracker reads [`UiEvent`]s, the same stream the overlay and tray get,
//! so the pipeline itself knows nothing about telemetry. It keeps only
//! timestamps and a flag; the text of an error is never read, only its kind
//! ([`ErrorType`]). Whatever the transcript, the key or an error's reason
//! holds, it can't reach an [`Event`], which has no field for it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dictation_core::coordinator::Phase;
use dictation_ports::{
    DurationBucket, EngineKind, ErrorReporter, ErrorType, Event, LatencyBucket, NoErrorReporter,
    Telemetry,
};

use crate::pipeline::UiEvent;

/// Lets usage events through while sharing is on (U10): from the first moment
/// of a launch, and only the stored choice or the switch changes that.
pub struct SharingGate {
    telemetry: Arc<dyn Telemetry>,
    share: AtomicBool,
}

impl SharingGate {
    /// A gate in front of `telemetry`, starting from the stored choice.
    #[must_use]
    pub fn new(telemetry: Arc<dyn Telemetry>, share: bool) -> Self {
        Self {
            telemetry,
            share: AtomicBool::new(share),
        }
    }

    /// The user turned sharing on or off.
    pub fn set_share(&self, share: bool) {
        self.share.store(share, Ordering::SeqCst);
    }

    /// Whether anything may be sent now.
    #[must_use]
    pub fn allows(&self) -> bool {
        self.share.load(Ordering::SeqCst)
    }
}

impl Telemetry for SharingGate {
    fn record(&self, event: Event) {
        if self.allows() {
            self.telemetry.record(event);
        }
    }
}

/// Observes a dictation's phases and records its outcome.
pub struct UsageTracker {
    telemetry: Arc<dyn Telemetry>,
    reporter: Arc<dyn ErrorReporter>,
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
            reporter: Arc::new(NoErrorReporter),
            engine,
            recording_since: None,
            recorded: None,
            transcribing_since: None,
            transcribed: None,
        }
    }

    /// Also reports each error to `reporter` (A2): its code and type only.
    #[must_use]
    pub fn with_reporter(mut self, reporter: Arc<dyn ErrorReporter>) -> Self {
        self.reporter = reporter;
        self
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
                self.reporter.report(error);
                self.reset();
            }
            // Not a failure and not a dictation: nothing to count (H2).
            UiEvent::HotkeyRestored | UiEvent::NoSound => {}
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

    fn opened() -> Event {
        Event::OnboardingStepReached {
            step: dictation_ports::OnboardingStep::Speech,
        }
    }

    /// U10 criterion 2: no hold, so the first event goes through.
    #[test]
    fn with_sharing_on_the_first_event_goes_through() {
        let fake = FakeTelemetry::new();
        let gate = SharingGate::new(Arc::new(fake.clone()), true);
        assert!(gate.allows());
        gate.record(opened());
        assert_eq!(fake.events().len(), 1);
    }

    /// U10 criterion 3.
    #[test]
    fn with_sharing_stored_off_nothing_goes_through() {
        let fake = FakeTelemetry::new();
        let gate = SharingGate::new(Arc::new(fake.clone()), false);
        assert!(!gate.allows());
        gate.record(opened());
        assert!(fake.events().is_empty());
    }

    #[test]
    fn the_gate_follows_the_switch() {
        let fake = FakeTelemetry::new();
        let gate = SharingGate::new(Arc::new(fake.clone()), true);
        gate.record(opened());
        gate.set_share(false);
        gate.record(opened());
        gate.set_share(true);
        gate.record(opened());
        assert_eq!(fake.events().len(), 2);
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

    #[test]
    fn an_error_is_also_reported_with_its_code_only() {
        use dictation_core::diagnostics::ErrorCode;
        use dictation_platform::fakes::FakeErrorReporter;

        let reporter = FakeErrorReporter::new();
        let mut t = UsageTracker::new(Arc::new(FakeTelemetry::new()), EngineKind::Cloud)
            .with_reporter(Arc::new(reporter.clone()));
        t.on_event(&UiEvent::Error(DictationError::SecureInput), secs(1));
        t.on_event(&UiEvent::StateChanged(Phase::Idle), secs(2));
        assert_eq!(reporter.codes(), [ErrorCode::SecureInput]);
    }
}
