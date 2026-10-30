//! Runs the core coordinator against the ports on tokio.
//!
//! One task owns the coordinator [`State`] and the audio source. It turns
//! hotkey presses, clock ticks and finished transcriptions and inserts into
//! core [`Event`]s, feeds them to [`step`], and carries out the returned
//! [`Command`]s through the ports. State changes and errors go to the UI as
//! [`UiEvent`]s on a channel.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use dictation_core::coordinator::{Phase, State, step};
use dictation_core::error::DictationError;
use dictation_core::events::{Command, Event};
use dictation_ports::{
    Audio, AudioError, AudioSource, Clock, HotkeyError, HotkeyEvent, HotkeySource, InsertError,
    Permission, PermissionStatus, Permissions, TextInserter, TranscribeError, Transcriber,
};
use tokio::sync::{mpsc, watch};
use tokio::task::{self, JoinError, JoinHandle, JoinSet};
use tokio::time::{Interval, MissedTickBehavior};

/// What the UI hears about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    /// The coordinator moved to this phase.
    StateChanged(Phase),
    /// A dictation failed; show this to the user.
    Error(DictationError),
}

/// The ports the pipeline runs against.
pub struct Ports<H, A, T, I, P, C> {
    /// Global hotkey and Esc.
    pub hotkey: H,
    /// The microphone.
    pub audio: A,
    /// Speech to text.
    pub transcriber: T,
    /// Pastes into the focused app.
    pub inserter: I,
    /// Microphone and Accessibility checks on each press.
    pub permissions: P,
    /// Stamps every event with the time.
    pub clock: C,
}

/// Pipeline settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// How often the coordinator gets a `Tick` while recording, to enforce
    /// the maximum recording length. The first comes one interval after the
    /// recording starts. Zero counts as [`MIN_TICK_INTERVAL`].
    pub tick_interval: Duration,
}

/// The shortest [`Config::tick_interval`] the pipeline uses.
pub const MIN_TICK_INTERVAL: Duration = Duration::from_millis(1);

impl Default for Config {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_millis(250),
        }
    }
}

/// Controls a running pipeline.
#[derive(Debug)]
pub struct Pipeline {
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl Pipeline {
    /// Starts the hotkey and the pipeline task on the current tokio runtime
    /// and returns the UI event stream.
    ///
    /// # Errors
    ///
    /// The hotkey source's error if it can't start; nothing runs then.
    ///
    /// # Panics
    ///
    /// Outside a tokio runtime.
    pub fn spawn<H, A, T, I, P, C>(
        ports: Ports<H, A, T, I, P, C>,
        config: Config,
    ) -> Result<(Self, mpsc::UnboundedReceiver<UiEvent>), HotkeyError>
    where
        H: HotkeySource + 'static,
        A: AudioSource + 'static,
        T: Transcriber + 'static,
        I: TextInserter + 'static,
        P: Permissions + 'static,
        C: Clock + 'static,
    {
        let Ports {
            mut hotkey,
            audio,
            transcriber,
            inserter,
            permissions,
            clock,
        } = ports;
        let clock = Arc::new(clock);
        let mut ticks = tokio::time::interval(config.tick_interval.max(MIN_TICK_INTERVAL));
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // Stamp keys on the hotkey thread, when they happen, not when the
        // task gets to them.
        let (keys_tx, keys) = mpsc::unbounded_channel();
        let stamp = Arc::clone(&clock);
        hotkey.start(Box::new(move |key| {
            let _ = keys_tx.send((key, stamp.now()));
        }))?;

        let (ui, ui_events) = mpsc::unbounded_channel();
        let (shutdown, shutdown_rx) = watch::channel(false);
        let runner = Runner {
            state: State::Idle,
            hotkey,
            audio,
            transcriber: Arc::new(transcriber),
            inserter: Arc::new(inserter),
            permissions,
            clock,
            ui,
            shutdown: shutdown_rx,
            ticks,
            jobs: JoinSet::new(),
            running: None,
            captured: None,
            job: 0,
            pending: VecDeque::new(),
        };
        let task = tokio::spawn(runner.run(keys));
        Ok((Self { shutdown, task }, ui_events))
    }

    /// Stops listening, drops any recording and waits for the task to end.
    /// An in-flight transcription is dropped (cancelling its request) and
    /// nothing is inserted after this is called. An insert already pasting
    /// can't be stopped; its result is ignored.
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(true);
        let _ = self.task.await;
    }
}

/// A finished background job, tagged with the job it belongs to.
enum JobResult {
    Transcribed(u64, Result<String, TranscribeError>),
    Inserted(u64, Result<(), InsertError>),
}

/// What the current background job does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Job {
    Transcribe,
    Insert,
}

struct Runner<H, A, T, I, P, C> {
    state: State,
    hotkey: H,
    audio: A,
    transcriber: Arc<T>,
    inserter: Arc<I>,
    permissions: P,
    clock: Arc<C>,
    ui: mpsc::UnboundedSender<UiEvent>,
    /// Becomes `true` (or closes) when the pipeline should stop.
    shutdown: watch::Receiver<bool>,
    /// Ticks while recording; reset when a recording starts.
    ticks: Interval,
    /// Transcriptions and inserts. Dropping the set (at shutdown) aborts
    /// what's left, so a transcription's request is cancelled.
    jobs: JoinSet<JobResult>,
    /// The task running the current job, to tell whether a task that
    /// panicked was the current job or a stale one.
    running: Option<(task::Id, Job)>,
    /// What the last `StopRecording` produced, for the `Transcribe` after it.
    captured: Option<Result<Audio, AudioError>>,
    /// Bumped for every transcription or insert; a result for any other job
    /// is stale and dropped, so a finished or abandoned dictation can never
    /// insert text later.
    job: u64,
    /// Events produced while carrying out commands, fed before the next
    /// outside event.
    pending: VecDeque<Event>,
}

impl<H, A, T, I, P, C> Runner<H, A, T, I, P, C>
where
    H: HotkeySource,
    A: AudioSource,
    T: Transcriber + 'static,
    I: TextInserter + 'static,
    P: Permissions,
    C: Clock,
{
    async fn run(mut self, mut keys: mpsc::UnboundedReceiver<(HotkeyEvent, Duration)>) {
        loop {
            let recording = matches!(self.state, State::Recording { .. });
            // Biased, in this order: shutdown wins over anything else that
            // is ready, and keys already queued (stamped earlier) come
            // before a Tick.
            let event = tokio::select! {
                biased;
                _ = self.shutdown.changed() => break,
                Some((key, at)) = keys.recv() => self.key_event(key, at),
                Some(joined) = self.jobs.join_next_with_id() => self.job_event(joined),
                _ = self.ticks.tick(), if recording => Some(Event::Tick(self.clock.now())),
            };
            if let Some(event) = event {
                self.feed(event);
            }
        }
        self.jobs.abort_all();
        self.hotkey.stop();
        self.audio.discard();
    }

    fn shutting_down(&self) -> bool {
        // Err: the `Pipeline` was dropped, which also stops it.
        self.shutdown.has_changed().unwrap_or(true)
    }

    fn key_event(&self, key: HotkeyEvent, at: Duration) -> Option<Event> {
        Some(match key {
            HotkeyEvent::Pressed => match self.missing_permission() {
                Some(permission) => Event::PermissionMissing(permission),
                None => Event::HotkeyPressed(at),
            },
            HotkeyEvent::Released => Event::HotkeyReleased(at),
            HotkeyEvent::CancelPressed => Event::CancelPressed,
        })
    }

    /// The first permission dictation lacks, asking the OS to prompt for it
    /// if the user hasn't been asked yet. Checked only when idle: a press
    /// while busy is ignored by the coordinator anyway.
    fn missing_permission(&self) -> Option<Permission> {
        if self.state != State::Idle {
            return None;
        }
        [Permission::Microphone, Permission::Accessibility]
            .into_iter()
            .find(|&permission| match self.permissions.check(permission) {
                PermissionStatus::Granted => false,
                PermissionStatus::Denied => true,
                PermissionStatus::NotDetermined => {
                    if let Err(error) = self.permissions.request(permission) {
                        tracing::warn!(?permission, %error, "permission request failed");
                    }
                    true
                }
            })
    }

    fn job_event(&self, joined: Result<(task::Id, JobResult), JoinError>) -> Option<Event> {
        let result = match joined {
            Ok((_, result)) => result,
            Err(error) => return self.job_crashed(&error),
        };
        match result {
            JobResult::Transcribed(job, result) if job == self.job => Some(match result {
                Ok(text) => Event::TranscriptReady(text),
                Err(error) => Event::TranscriptionFailed(error.to_string()),
            }),
            JobResult::Inserted(job, result) if job == self.job => Some(match result {
                Ok(()) => Event::InsertDone,
                Err(error) => Event::InsertFailed(error.to_string()),
            }),
            JobResult::Transcribed(job, _) | JobResult::Inserted(job, _) => {
                tracing::debug!(job, current = self.job, "dropping a stale result");
                None
            }
        }
    }

    /// A job's task panicked (or was aborted). If it was the current job,
    /// the dictation fails like any other error, so the coordinator gets
    /// back to Idle. The panic message isn't logged: it could hold text.
    fn job_crashed(&self, error: &JoinError) -> Option<Event> {
        match self.running {
            Some((id, job)) if id == error.id() && error.is_panic() => {
                tracing::error!(?job, "a background job panicked");
                Some(match job {
                    Job::Transcribe => Event::TranscriptionFailed("the transcriber crashed".into()),
                    Job::Insert => Event::InsertFailed("the inserter crashed".into()),
                })
            }
            _ => {
                tracing::debug!(id = %error.id(), "dropping a stale job that crashed");
                None
            }
        }
    }

    /// Steps the coordinator with `event`, then with any events its commands
    /// produced, until none are left.
    fn feed(&mut self, event: Event) {
        self.pending.push_back(event);
        while let Some(event) = self.pending.pop_front() {
            let (state, commands) = step(self.state, event.clone());
            tracing::debug!(?event, ?commands, "step");
            self.state = state;
            for command in commands {
                self.execute(command);
            }
        }
    }

    fn execute(&mut self, command: Command) {
        match command {
            Command::StartRecording => {
                self.captured = None;
                // The first Tick comes a full interval from now, never at
                // once, ahead of keys that are still on their way.
                self.ticks.reset();
                if let Err(error) = self.audio.start() {
                    tracing::warn!(%error, "audio source failed to start");
                    match error {
                        AudioError::PermissionDenied => self
                            .pending
                            .push_back(Event::PermissionMissing(Permission::Microphone)),
                        // Reported when the recording stops (see Transcribe).
                        error => self.captured = Some(Err(error)),
                    }
                }
            }
            Command::StopRecording => {
                if self.captured.is_none() {
                    self.captured = Some(self.audio.stop());
                }
            }
            Command::DiscardRecording => {
                self.audio.discard();
                self.captured = None;
            }
            Command::Transcribe => self.transcribe(),
            Command::Insert(text) => self.insert(text),
            Command::ShowError(error) => {
                tracing::info!(%error, "dictation failed");
                let _ = self.ui.send(UiEvent::Error(error));
            }
            Command::StateChanged(phase) => {
                let _ = self.ui.send(UiEvent::StateChanged(phase));
            }
        }
    }

    fn transcribe(&mut self) {
        self.job += 1;
        let audio = match self.captured.take() {
            Some(Ok(audio)) => audio,
            Some(Err(error)) => {
                self.pending.push_back(Event::TranscriptionFailed(format!(
                    "the microphone failed ({error})"
                )));
                return;
            }
            None => {
                self.pending.push_back(Event::TranscriptionFailed(
                    "no audio was captured".to_owned(),
                ));
                return;
            }
        };
        tracing::debug!(job = self.job, duration = ?audio.duration(), "transcribing");
        let job = self.job;
        let transcriber = Arc::clone(&self.transcriber);
        let task = self.jobs.spawn(async move {
            JobResult::Transcribed(job, transcriber.transcribe(&audio).await)
        });
        self.running = Some((task.id(), Job::Transcribe));
    }

    fn insert(&mut self, text: String) {
        self.job += 1;
        // A shutdown that came in while this step ran: never paste after
        // it. The loop stops before the next event.
        if self.shutting_down() {
            tracing::debug!(job = self.job, "shutting down; not inserting");
            return;
        }
        let job = self.job;
        let inserter = Arc::clone(&self.inserter);
        // Real inserters block (paste, wait, restore the clipboard).
        let task = self
            .jobs
            .spawn_blocking(move || JobResult::Inserted(job, inserter.insert(&text)));
        self.running = Some((task.id(), Job::Insert));
    }
}

#[cfg(test)]
mod tests {
    use std::future::{Future, pending};

    use dictation_platform::fakes::{FakeClock, FakeHotkey, FakeInserter, FakePermissions};

    use super::*;

    /// Captures nothing, instantly.
    struct SilentAudio(bool);

    impl AudioSource for SilentAudio {
        fn start(&mut self) -> Result<(), AudioError> {
            self.0 = true;
            Ok(())
        }

        fn stop(&mut self) -> Result<Audio, AudioError> {
            self.0 = false;
            Ok(Audio {
                sample_rate: 16_000,
                samples: Vec::new(),
            })
        }

        fn discard(&mut self) {
            self.0 = false;
        }

        fn is_recording(&self) -> bool {
            self.0
        }
    }

    /// Never answers, so the only transcription results are the ones the
    /// test sends.
    struct SilentTranscriber;

    impl Transcriber for SilentTranscriber {
        fn transcribe(
            &self,
            _audio: &Audio,
        ) -> impl Future<Output = Result<String, TranscribeError>> + Send {
            pending()
        }
    }

    type TestRunner = Runner<
        FakeHotkey,
        SilentAudio,
        SilentTranscriber,
        FakeInserter,
        FakePermissions,
        FakeClock,
    >;

    fn runner(inserter: &FakeInserter) -> (TestRunner, watch::Sender<bool>) {
        let (ui, _) = mpsc::unbounded_channel();
        let (shutdown, shutdown_rx) = watch::channel(false);
        let runner = Runner {
            state: State::Idle,
            hotkey: FakeHotkey::new(),
            audio: SilentAudio(false),
            transcriber: Arc::new(SilentTranscriber),
            inserter: Arc::new(inserter.clone()),
            permissions: FakePermissions::all_granted(),
            clock: Arc::new(FakeClock::new()),
            ui,
            shutdown: shutdown_rx,
            ticks: tokio::time::interval(Duration::from_millis(250)),
            jobs: JoinSet::new(),
            running: None,
            captured: None,
            job: 0,
            pending: VecDeque::new(),
        };
        (runner, shutdown)
    }

    fn key(runner: &mut TestRunner, key: HotkeyEvent, at_ms: u64) {
        if let Some(event) = runner.key_event(key, Duration::from_millis(at_ms)) {
            runner.feed(event);
        }
    }

    fn deliver(runner: &mut TestRunner, result: JobResult) {
        let id = runner.running.unwrap().0;
        if let Some(event) = runner.job_event(Ok((id, result))) {
            runner.feed(event);
        }
    }

    /// The insert job's result. The transcription jobs never finish, so
    /// it is the only one that can.
    async fn next_insert(runner: &mut TestRunner) -> JobResult {
        let joined = tokio::time::timeout(Duration::from_secs(5), runner.jobs.join_next())
            .await
            .unwrap()
            .unwrap();
        joined.unwrap()
    }

    #[tokio::test]
    async fn a_late_transcript_from_an_earlier_dictation_is_never_inserted() {
        let inserter = FakeInserter::new();
        let (mut runner, _shutdown) = runner(&inserter);

        // Dictation 1 (job 1) completes.
        key(&mut runner, HotkeyEvent::Pressed, 0);
        key(&mut runner, HotkeyEvent::Released, 500);
        assert_eq!(runner.job, 1);
        deliver(&mut runner, JobResult::Transcribed(1, Ok("first".into())));
        let inserted = next_insert(&mut runner).await;
        deliver(&mut runner, inserted);
        assert_eq!(runner.state, State::Idle);

        // Dictation 2 is cancelled; dictation 3 is transcribing (job 3).
        key(&mut runner, HotkeyEvent::Pressed, 1_000);
        key(&mut runner, HotkeyEvent::CancelPressed, 1_100);
        assert_eq!(runner.state, State::Idle);
        key(&mut runner, HotkeyEvent::Pressed, 2_000);
        key(&mut runner, HotkeyEvent::Released, 2_500);
        assert_eq!((runner.state, runner.job), (State::Transcribing, 3));

        // A late copy of job 1's transcript arrives while the coordinator
        // would accept a transcript: the job id drops it.
        deliver(&mut runner, JobResult::Transcribed(1, Ok("stale".into())));
        assert_eq!(runner.state, State::Transcribing);

        // A late insert result can't end the current dictation either.
        deliver(
            &mut runner,
            JobResult::Inserted(2, Err(InsertError::SecureInput)),
        );
        assert_eq!(runner.state, State::Transcribing);

        deliver(&mut runner, JobResult::Transcribed(3, Ok("third".into())));
        let inserted = next_insert(&mut runner).await;
        deliver(&mut runner, inserted);
        assert_eq!(runner.state, State::Idle);
        assert_eq!(inserter.inserted(), ["first", "third"]);
    }

    #[tokio::test]
    async fn a_shutdown_that_comes_in_mid_step_starts_no_insert() {
        let inserter = FakeInserter::new();
        let (mut runner, shutdown) = runner(&inserter);
        key(&mut runner, HotkeyEvent::Pressed, 0);
        key(&mut runner, HotkeyEvent::Released, 500);
        // The transcript and the shutdown arrive together; the transcript
        // is stepped first.
        shutdown.send(true).unwrap();
        deliver(&mut runner, JobResult::Transcribed(1, Ok("late".into())));
        assert_eq!(runner.state, State::Inserting);
        // Only the (never-ending) transcription; no insert was started.
        assert_eq!(runner.jobs.len(), 1);
        runner.jobs.abort_all();
        while runner.jobs.join_next().await.is_some() {}
        assert!(inserter.inserted().is_empty());
    }

    #[tokio::test]
    async fn a_dropped_pipeline_counts_as_shutting_down() {
        let (runner, shutdown) = runner(&FakeInserter::new());
        assert!(!runner.shutting_down());
        drop(shutdown);
        assert!(runner.shutting_down());
    }

    #[tokio::test]
    async fn a_crash_of_a_stale_job_is_dropped() {
        let (mut runner, _shutdown) = runner(&FakeInserter::new());
        key(&mut runner, HotkeyEvent::Pressed, 0);
        key(&mut runner, HotkeyEvent::Released, 500);
        // Some other task (not the current transcription) crashes.
        let mut other = JoinSet::new();
        other.spawn(async { std::panic::panic_any("stale") });
        let crashed = other.join_next().await.unwrap().unwrap_err();
        assert_eq!(runner.job_event(Err(crashed)), None);
        assert_eq!(runner.state, State::Transcribing);
    }
}
