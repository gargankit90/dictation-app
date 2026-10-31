//! Runs the core coordinator against the ports on tokio.
//!
//! One task owns the coordinator [`State`]. It turns hotkey presses, clock
//! ticks and finished transcriptions and inserts into core [`Event`]s, feeds
//! them to [`step`], and carries out the returned [`Command`]s through the
//! ports. State changes and errors go to the UI as [`UiEvent`]s on a
//! channel.
//!
//! The audio source lives in a task of its own whose calls run on the
//! blocking pool (see [`AudioWorker`]), because a real microphone's `start`
//! and `stop` block (opening a Bluetooth device can take seconds) and must
//! not stall this task: a Cancel is handled while the microphone is still
//! opening.

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
use tokio::sync::{mpsc, oneshot, watch};
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
        let (audio, audio_failures) = AudioWorker::spawn(audio);
        let runner = Runner::new(
            hotkey,
            audio,
            transcriber,
            inserter,
            permissions,
            clock,
            ui,
            shutdown_rx,
            ticks,
        );
        let task = tokio::spawn(runner.run(keys, audio_failures));
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
    /// The recording the transcription was waiting for failed.
    AudioFailed(u64, AudioError),
    Inserted(u64, Result<(), InsertError>),
}

/// What the current background job does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Job {
    Transcribe,
    Insert,
}

/// What the pipeline task asks of the audio worker, carried out in order.
enum AudioCommand {
    /// Start recording `session`.
    Start(u64),
    /// Stop and send back what was captured (or why nothing was).
    Stop(oneshot::Sender<Result<Audio, AudioError>>),
    /// Stop, if recording, and drop the audio.
    Discard,
}

/// A `start` that failed in a way the coordinator must hear about while
/// still recording: the microphone permission is missing.
struct StartFailed {
    session: u64,
    error: AudioError,
}

/// Owns the [`AudioSource`] in a task of its own that runs each call on
/// tokio's blocking pool (`spawn_blocking`), one at a time and in the order
/// sent, so a slow `start` or `stop` never blocks the pipeline task.
///
/// A `start` error other than [`AudioError::PermissionDenied`] is kept and
/// returned by the next `Stop`, so it reaches the user when the recording
/// stops. If a call panics the source is gone: later stops report a device
/// error and the panic message (which could hold anything) isn't logged.
struct AudioWorker {
    commands: mpsc::UnboundedSender<AudioCommand>,
    task: JoinHandle<()>,
}

impl AudioWorker {
    /// Starts the task; the receiver gets permission failures from `start`.
    fn spawn<A: AudioSource + 'static>(audio: A) -> (Self, mpsc::UnboundedReceiver<StartFailed>) {
        let (commands, inbox) = mpsc::unbounded_channel();
        let (failures, failed) = mpsc::unbounded_channel();
        let task = tokio::spawn(Self::run(audio, inbox, failures));
        (Self { commands, task }, failed)
    }

    async fn run<A: AudioSource + 'static>(
        audio: A,
        mut inbox: mpsc::UnboundedReceiver<AudioCommand>,
        failures: mpsc::UnboundedSender<StartFailed>,
    ) {
        let mut audio = Some(audio);
        let mut start_error = None;
        // Ends, with a discard, when the pipeline drops its sender at
        // shutdown.
        while let Some(command) = inbox.recv().await {
            match command {
                AudioCommand::Start(session) => {
                    start_error = None;
                    let result = blocking(&mut audio, A::start).await;
                    match result {
                        Ok(()) => {}
                        Err(AudioError::PermissionDenied) => {
                            tracing::warn!("audio source failed to start: no permission");
                            let error = AudioError::PermissionDenied;
                            let _ = failures.send(StartFailed { session, error });
                        }
                        Err(error) => {
                            tracing::warn!(%error, "audio source failed to start");
                            start_error = Some(error);
                        }
                    }
                }
                AudioCommand::Stop(reply) => {
                    let captured = match start_error.take() {
                        Some(error) => Err(error),
                        None => blocking(&mut audio, A::stop).await,
                    };
                    let _ = reply.send(captured);
                }
                AudioCommand::Discard => {
                    start_error = None;
                    let _ = blocking(&mut audio, |audio| {
                        audio.discard();
                        Ok(())
                    })
                    .await;
                }
            }
        }
        let _ = blocking(&mut audio, |audio| {
            audio.discard();
            Ok(())
        })
        .await;
    }

    fn send(&self, command: AudioCommand) {
        // Fails only if the task is gone; a Stop's reply then reports it.
        let _ = self.commands.send(command);
    }

    /// Drops any recording and waits for the task to finish what it was
    /// doing (at most one blocking call) and the final discard.
    async fn shut_down(self) {
        let Self { commands, task } = self;
        drop(commands);
        let _ = task.await;
    }
}

/// Runs `call` on the source on the blocking pool and puts the source back.
/// Without a source (an earlier call panicked) it's a device error.
async fn blocking<A, R>(
    audio: &mut Option<A>,
    call: impl FnOnce(&mut A) -> Result<R, AudioError> + Send + 'static,
) -> Result<R, AudioError>
where
    A: AudioSource + 'static,
    R: Send + 'static,
{
    let Some(mut source) = audio.take() else {
        return Err(AudioError::Device("the audio source crashed".into()));
    };
    match task::spawn_blocking(move || {
        let result = call(&mut source);
        (source, result)
    })
    .await
    {
        Ok((source, result)) => {
            *audio = Some(source);
            result
        }
        Err(_) => {
            tracing::error!("the audio source panicked");
            Err(AudioError::Device("the audio source crashed".into()))
        }
    }
}

struct Runner<H, T, I, P, C> {
    state: State,
    hotkey: H,
    /// Our side of the audio worker, which owns the source.
    audio: AudioWorker,
    /// The recording the last `StartRecording` began, so a late permission
    /// failure from an earlier one is dropped.
    session: u64,
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
    /// What the last `StopRecording` will produce, for the `Transcribe`
    /// after it.
    captured: Option<oneshot::Receiver<Result<Audio, AudioError>>>,
    /// Bumped for every transcription or insert; a result for any other job
    /// is stale and dropped, so a finished or abandoned dictation can never
    /// insert text later.
    job: u64,
    /// Events produced while carrying out commands, fed before the next
    /// outside event.
    pending: VecDeque<Event>,
}

impl<H, T, I, P, C> Runner<H, T, I, P, C>
where
    H: HotkeySource,
    T: Transcriber + 'static,
    I: TextInserter + 'static,
    P: Permissions,
    C: Clock,
{
    #[allow(clippy::too_many_arguments, reason = "the ports and channels, once")]
    fn new(
        hotkey: H,
        audio: AudioWorker,
        transcriber: T,
        inserter: I,
        permissions: P,
        clock: Arc<C>,
        ui: mpsc::UnboundedSender<UiEvent>,
        shutdown: watch::Receiver<bool>,
        ticks: Interval,
    ) -> Self {
        Self {
            state: State::Idle,
            hotkey,
            audio,
            session: 0,
            transcriber: Arc::new(transcriber),
            inserter: Arc::new(inserter),
            permissions,
            clock,
            ui,
            shutdown,
            ticks,
            jobs: JoinSet::new(),
            running: None,
            captured: None,
            job: 0,
            pending: VecDeque::new(),
        }
    }

    async fn run(
        mut self,
        mut keys: mpsc::UnboundedReceiver<(HotkeyEvent, Duration)>,
        mut audio_failures: mpsc::UnboundedReceiver<StartFailed>,
    ) {
        loop {
            let recording = matches!(self.state, State::Recording { .. });
            // Biased, in this order: shutdown wins over anything else that
            // is ready, and keys already queued (stamped earlier) come
            // before a Tick.
            let event = tokio::select! {
                biased;
                _ = self.shutdown.changed() => break,
                Some((key, at)) = keys.recv() => self.key_event(key, at),
                Some(failed) = audio_failures.recv() => self.start_failed(&failed),
                Some(joined) = self.jobs.join_next_with_id() => self.job_event(joined),
                _ = self.ticks.tick(), if recording => Some(Event::Tick(self.clock.now())),
            };
            if let Some(event) = event {
                self.feed(event);
            }
        }
        self.jobs.abort_all();
        self.hotkey.stop();
        self.audio.shut_down().await;
    }

    /// The microphone permission was missing when the recording started.
    /// Only the current recording, still running, hears about it.
    fn start_failed(&self, failed: &StartFailed) -> Option<Event> {
        let current =
            failed.session == self.session && matches!(self.state, State::Recording { .. });
        if !current {
            tracing::debug!(error = %failed.error, "dropping a stale start failure");
            return None;
        }
        Some(Event::PermissionMissing(Permission::Microphone))
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
            JobResult::AudioFailed(job, error) if job == self.job => Some(
                Event::TranscriptionFailed(format!("the microphone failed ({error})")),
            ),
            JobResult::Transcribed(job, _)
            | JobResult::Inserted(job, _)
            | JobResult::AudioFailed(job, _) => {
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
                self.session += 1;
                self.audio.send(AudioCommand::Start(self.session));
            }
            Command::StopRecording => {
                if self.captured.is_none() {
                    let (reply, captured) = oneshot::channel();
                    self.audio.send(AudioCommand::Stop(reply));
                    self.captured = Some(captured);
                }
            }
            Command::DiscardRecording => {
                self.audio.send(AudioCommand::Discard);
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
        let Some(captured) = self.captured.take() else {
            self.pending.push_back(Event::TranscriptionFailed(
                "no audio was captured".to_owned(),
            ));
            return;
        };
        let job = self.job;
        let transcriber = Arc::clone(&self.transcriber);
        // The job waits for the audio worker's stop, then transcribes.
        let task = self.jobs.spawn(async move {
            let audio = match captured.await {
                Ok(Ok(audio)) => audio,
                Ok(Err(error)) => return JobResult::AudioFailed(job, error),
                Err(_) => {
                    return JobResult::AudioFailed(
                        job,
                        AudioError::Device("the audio worker stopped".into()),
                    );
                }
            };
            tracing::debug!(job, duration = ?audio.duration(), "transcribing");
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

    type TestRunner =
        Runner<FakeHotkey, SilentTranscriber, FakeInserter, FakePermissions, FakeClock>;

    fn runner(inserter: &FakeInserter) -> (TestRunner, watch::Sender<bool>) {
        let (ui, _) = mpsc::unbounded_channel();
        let (shutdown, shutdown_rx) = watch::channel(false);
        let (audio, _) = AudioWorker::spawn(SilentAudio(false));
        let runner = Runner::new(
            FakeHotkey::new(),
            audio,
            SilentTranscriber,
            inserter.clone(),
            FakePermissions::all_granted(),
            Arc::new(FakeClock::new()),
            ui,
            shutdown_rx,
            tokio::time::interval(Duration::from_millis(250)),
        );
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

#[cfg(test)]
mod audio_worker_tests {
    use std::sync::Mutex;
    use std::sync::mpsc as std_mpsc;

    use dictation_core::coordinator::Phase::{Idle, Recording, Transcribing};
    use dictation_platform::fakes::{FakeClock, FakeHotkey, FakeInserter, FakePermissions};

    use super::*;
    use crate::stand_ins::FixedTranscriber;

    const WAIT: Duration = Duration::from_secs(5);

    /// How [`ScriptedAudio`]'s next `start` behaves.
    enum OnStart {
        Succeed,
        /// Blocks until the sender is dropped or sends.
        Block(std_mpsc::Receiver<()>),
        Fail(AudioError),
    }

    /// An audio source whose `start` can block, fail or let `stop` panic,
    /// and that logs every call.
    #[derive(Clone)]
    struct ScriptedAudio {
        on_start: Arc<Mutex<Vec<OnStart>>>,
        panic_on_stop: bool,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl ScriptedAudio {
        fn new(on_start: Vec<OnStart>) -> Self {
            Self {
                on_start: Arc::new(Mutex::new(on_start)),
                panic_on_stop: false,
                calls: Arc::default(),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl AudioSource for ScriptedAudio {
        fn start(&mut self) -> Result<(), AudioError> {
            let next = {
                let mut script = self.on_start.lock().unwrap();
                if script.is_empty() {
                    OnStart::Succeed
                } else {
                    script.remove(0)
                }
            };
            let result = match next {
                OnStart::Succeed => Ok(()),
                OnStart::Block(gate) => {
                    let _ = gate.recv();
                    Ok(())
                }
                OnStart::Fail(error) => Err(error),
            };
            self.calls.lock().unwrap().push("start");
            result
        }

        fn stop(&mut self) -> Result<Audio, AudioError> {
            self.calls.lock().unwrap().push("stop");
            assert!(!self.panic_on_stop, "scripted panic");
            Ok(Audio {
                sample_rate: 16_000,
                samples: vec![0.25; 4_000],
            })
        }

        fn discard(&mut self) {
            self.calls.lock().unwrap().push("discard");
        }

        fn is_recording(&self) -> bool {
            false
        }
    }

    struct Harness {
        hotkey: FakeHotkey,
        clock: FakeClock,
        pipeline: Pipeline,
        ui: mpsc::UnboundedReceiver<UiEvent>,
    }

    impl Harness {
        fn start(audio: ScriptedAudio) -> Self {
            let hotkey = FakeHotkey::new();
            let clock = FakeClock::new();
            let ports = Ports {
                hotkey: hotkey.clone(),
                audio,
                transcriber: FixedTranscriber,
                inserter: FakeInserter::new(),
                permissions: FakePermissions::all_granted(),
                clock: clock.clone(),
            };
            let (pipeline, ui) = Pipeline::spawn(ports, Config::default()).unwrap();
            Self {
                hotkey,
                clock,
                pipeline,
                ui,
            }
        }

        fn emit(&self, event: HotkeyEvent) {
            assert!(self.hotkey.emit(event));
        }

        async fn next(&mut self) -> UiEvent {
            tokio::time::timeout(WAIT, self.ui.recv())
                .await
                .expect("no UI event in time")
                .expect("pipeline stopped")
        }

        async fn dictate(&mut self) -> Vec<UiEvent> {
            self.emit(HotkeyEvent::Pressed);
            self.clock.advance(Duration::from_millis(500));
            self.emit(HotkeyEvent::Released);
            let mut events = Vec::new();
            loop {
                let event = self.next().await;
                let idle = event == UiEvent::StateChanged(Idle);
                events.push(event);
                if idle {
                    return events;
                }
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn issue_40_a_blocking_start_does_not_stall_cancel() {
        let (release, gate) = std_mpsc::channel();
        let audio = ScriptedAudio::new(vec![OnStart::Block(gate)]);
        let mut harness = Harness::start(audio.clone());

        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        harness.emit(HotkeyEvent::CancelPressed);
        // Handled while `start` is still blocked.
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
        assert_eq!(audio.calls(), Vec::<&str>::new(), "start is still running");

        release.send(()).unwrap();
        harness.pipeline.shutdown().await;
        // In order: the discard ran after the start it cancels.
        assert_eq!(audio.calls(), ["start", "discard", "discard"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_dictation_after_a_blocked_start_gets_its_audio() {
        let (release, gate) = std_mpsc::channel();
        let audio = ScriptedAudio::new(vec![OnStart::Block(gate)]);
        let mut harness = Harness::start(audio.clone());

        harness.emit(HotkeyEvent::Pressed);
        harness.clock.advance(Duration::from_millis(500));
        harness.emit(HotkeyEvent::Released);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        assert_eq!(harness.next().await, UiEvent::StateChanged(Transcribing));
        // The transcription waits for the stop, which waits for the start.
        release.send(()).unwrap();
        assert_eq!(
            harness.next().await,
            UiEvent::StateChanged(dictation_core::coordinator::Phase::Inserting)
        );
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
        harness.pipeline.shutdown().await;
        assert_eq!(audio.calls(), ["start", "stop", "discard"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_missing_microphone_permission_at_start_is_shown_at_once() {
        let audio = ScriptedAudio::new(vec![OnStart::Fail(AudioError::PermissionDenied)]);
        let mut harness = Harness::start(audio.clone());

        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        // No release needed: the failure ends the recording.
        assert_eq!(
            harness.next().await,
            UiEvent::Error(DictationError::PermissionMissing(Permission::Microphone))
        );
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
        harness.pipeline.shutdown().await;
        assert_eq!(audio.calls(), ["start", "discard", "discard"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn another_start_error_is_reported_when_the_recording_stops() {
        let audio = ScriptedAudio::new(vec![OnStart::Fail(AudioError::NoDevice)]);
        let mut harness = Harness::start(audio.clone());
        assert_eq!(
            harness.dictate().await,
            [
                UiEvent::StateChanged(Recording),
                UiEvent::StateChanged(Transcribing),
                UiEvent::Error(DictationError::TranscriptionFailed {
                    reason: "the microphone failed (no microphone found)".into()
                }),
                UiEvent::StateChanged(Idle),
            ]
        );
        // The failed start's error isn't returned twice.
        assert!(matches!(
            harness.dictate().await[2],
            UiEvent::StateChanged(dictation_core::coordinator::Phase::Inserting)
        ));
        harness.pipeline.shutdown().await;
        assert_eq!(audio.calls(), ["start", "start", "stop", "discard"]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_panicking_source_fails_dictations_but_not_the_pipeline() {
        let mut audio = ScriptedAudio::new(Vec::new());
        audio.panic_on_stop = true;
        let mut harness = Harness::start(audio.clone());
        let crashed = UiEvent::Error(DictationError::TranscriptionFailed {
            reason: "the microphone failed (audio device error: the audio source crashed)".into(),
        });
        assert_eq!(harness.dictate().await[2], crashed);
        // The source is gone; the next dictation fails the same way.
        assert_eq!(harness.dictate().await[2], crashed);
        harness.pipeline.shutdown().await;
        assert_eq!(audio.calls(), ["start", "stop"]);
    }

    #[tokio::test]
    async fn a_start_failure_from_an_earlier_recording_is_dropped() {
        let (ui, _ui) = mpsc::unbounded_channel();
        let (_shutdown, shutdown_rx) = watch::channel(false);
        let (audio, _) = AudioWorker::spawn(ScriptedAudio::new(Vec::new()));
        let mut runner = Runner::new(
            FakeHotkey::new(),
            audio,
            FixedTranscriber,
            FakeInserter::new(),
            FakePermissions::all_granted(),
            Arc::new(FakeClock::new()),
            ui,
            shutdown_rx,
            tokio::time::interval(Duration::from_millis(250)),
        );
        let failed = |session| StartFailed {
            session,
            error: AudioError::PermissionDenied,
        };
        // Idle: nothing is recording.
        assert_eq!(runner.start_failed(&failed(0)), None);
        runner.feed(Event::HotkeyPressed(Duration::ZERO));
        assert_eq!(runner.session, 1);
        assert_eq!(runner.start_failed(&failed(0)), None);
        assert_eq!(
            runner.start_failed(&failed(1)),
            Some(Event::PermissionMissing(Permission::Microphone))
        );
    }
}
