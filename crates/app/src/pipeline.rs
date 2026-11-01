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
//!
//! The queues into this task are bounded (#81): hotkey events
//! ([`KEYS_QUEUED`]; a dropped key cancels a recording, see [`Keys`]), the
//! audio worker's commands (at most three: a cancelled recording's `Start`
//! is dropped before it begins, see [`AudioQueue`]) and its start failures.
//! The UI channel isn't bounded yet (#84).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use dictation_core::coordinator::{Phase, State, step};
use dictation_core::error::DictationError;
use dictation_core::events::{Command, Event};
use dictation_ports::{
    Audio, AudioError, AudioSource, Clock, HotkeyError, HotkeyEvent, HotkeySource, InsertError,
    Permission, PermissionStatus, Permissions, TextInserter, TranscribeError, Transcriber,
};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Notify, mpsc, oneshot, watch};
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
        // task gets to them. Never blocks that thread: a full queue drops
        // the key and flags it (see [`Keys`]).
        let (keys_tx, keys_rx) = mpsc::channel(KEYS_QUEUED);
        let keys = Keys {
            rx: keys_rx,
            overflowed: Arc::new(AtomicBool::new(false)),
        };
        let overflowed = Arc::clone(&keys.overflowed);
        let stamp = Arc::clone(&clock);
        hotkey.start(Box::new(move |key| {
            if let Err(TrySendError::Full(_)) = keys_tx.try_send((key, stamp.now()))
                && !overflowed.swap(true, Ordering::AcqRel)
            {
                tracing::warn!("hotkey events are queueing up; dropping them and cancelling");
            }
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

/// Hotkey events the pipeline task may have unread. The task never blocks,
/// so a person can't fill this; only a stuck task or a flood of synthetic
/// events can.
const KEYS_QUEUED: usize = 64;

/// The hotkey events, and whether one was dropped because [`KEYS_QUEUED`]
/// were waiting.
///
/// A dropped event could be a release, which would leave a recording
/// running, so after one the pipeline acts as if Esc was pressed: a
/// recording in progress is discarded, and anything else is unaffected
/// (Esc is ignored while transcribing or inserting). The flag is read after
/// each key, and the queue was full when it was set, so it's seen within
/// [`KEYS_QUEUED`] keys.
struct Keys {
    rx: mpsc::Receiver<(HotkeyEvent, Duration)>,
    overflowed: Arc<AtomicBool>,
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

/// How many [`StartFailed`] the pipeline task may have unread. Each comes
/// from a `start` the worker ran, one at a time, and only the latest
/// session's matters, so a full queue drops the new one (its error also
/// reaches the next `Stop`).
const START_FAILURES_QUEUED: usize = 4;

/// Commands the audio worker hasn't begun, and whether it's shutting down.
#[derive(Default)]
struct AudioQueue {
    commands: VecDeque<AudioCommand>,
    closed: bool,
}

impl AudioQueue {
    /// Queues `command`, dropping work it makes obsolete (#81): a `Discard`
    /// behind a `Start` that hasn't begun removes that `Start` (the
    /// recording was abandoned before the device was touched, so there's
    /// nothing to discard either), and a `Discard` behind a `Discard` adds
    /// nothing.
    ///
    /// The coordinator alternates `Start` with one `Stop` or `Discard`, and
    /// can't start again after a `Stop` until that `Stop` has run (the
    /// transcription waits for its audio). So with this the queue holds at
    /// most three commands, `Discard`, `Start` and `Stop`, however fast keys
    /// come while a `start` blocks.
    fn push(&mut self, command: AudioCommand) {
        if self.closed {
            return;
        }
        if matches!(command, AudioCommand::Discard) {
            match self.commands.back() {
                Some(AudioCommand::Start(session)) => {
                    tracing::debug!(session, "recording abandoned before it started");
                    self.commands.pop_back();
                    return;
                }
                Some(AudioCommand::Discard) => return,
                _ => {}
            }
        }
        self.commands.push_back(command);
    }
}

/// The audio worker's inbox, shared with the pipeline task.
#[derive(Default)]
struct AudioInbox {
    queue: std::sync::Mutex<AudioQueue>,
    ready: Notify,
}

impl AudioInbox {
    fn lock(&self) -> std::sync::MutexGuard<'_, AudioQueue> {
        // Nothing panics while holding it; recover the queue if it did.
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push(&self, command: AudioCommand) {
        self.lock().push(command);
        self.ready.notify_one();
    }

    /// Stops taking commands and drops the ones not begun. A dropped
    /// `Stop`'s receiver sees the worker gone.
    fn close(&self) {
        let dropped = {
            let mut queue = self.lock();
            queue.closed = true;
            std::mem::take(&mut queue.commands)
        };
        drop(dropped);
        self.ready.notify_one();
    }

    /// The next command; `None` once closed.
    async fn next(&self) -> Option<AudioCommand> {
        loop {
            {
                let mut queue = self.lock();
                if queue.closed {
                    return None;
                }
                if let Some(command) = queue.commands.pop_front() {
                    return Some(command);
                }
            }
            // `notify_one` keeps a permit if nobody waits yet, so a push
            // between the check above and here isn't missed.
            self.ready.notified().await;
        }
    }
}

/// Owns the [`AudioSource`] in a task of its own that runs each call on
/// tokio's blocking pool (`spawn_blocking`), one at a time and in the order
/// sent, so a slow `start` or `stop` never blocks the pipeline task.
/// Commands wait in an [`AudioQueue`], which drops a cancelled recording's
/// `Start` that hasn't begun and stays small.
///
/// A `start` error is kept and returned by the next `Stop` (the source's
/// `stop` isn't called then), so it reaches the user when the recording
/// stops; [`AudioError::PermissionDenied`] is also reported at once, for a
/// recording still running. If a call panics the source is gone: later
/// stops report a device error and the panic message (which could hold
/// anything) isn't logged.
struct AudioWorker {
    inbox: Arc<AudioInbox>,
    task: Option<JoinHandle<()>>,
}

impl AudioWorker {
    /// Starts the task; the receiver gets permission failures from `start`.
    fn spawn<A: AudioSource + 'static>(audio: A) -> (Self, mpsc::Receiver<StartFailed>) {
        let inbox = Arc::new(AudioInbox::default());
        let (failures, failed) = mpsc::channel(START_FAILURES_QUEUED);
        let task = tokio::spawn(Self::run(audio, Arc::clone(&inbox), failures));
        let worker = Self {
            inbox,
            task: Some(task),
        };
        (worker, failed)
    }

    async fn run<A: AudioSource + 'static>(
        audio: A,
        inbox: Arc<AudioInbox>,
        failures: mpsc::Sender<StartFailed>,
    ) {
        let mut audio = Some(audio);
        let mut start_error = None;
        // Ends, with a discard, when the pipeline shuts down; commands not
        // begun by then are dropped.
        while let Some(command) = inbox.next().await {
            match command {
                AudioCommand::Start(session) => {
                    start_error = None;
                    let result = blocking(&mut audio, A::start).await;
                    match result {
                        Ok(()) => {}
                        Err(AudioError::PermissionDenied) => {
                            tracing::warn!("audio source failed to start: no permission");
                            let error = AudioError::PermissionDenied;
                            // A release may have come first: then only the
                            // Stop hears about it (#81 M2).
                            start_error = Some(error.clone());
                            let _ = failures.try_send(StartFailed { session, error });
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
        // The last call: discard and drop the source on the blocking pool.
        // Dropping a warm microphone closes the device, which can take
        // seconds, so it must not happen on this async worker.
        if let Some(mut source) = audio.take() {
            let teardown = task::spawn_blocking(move || {
                source.discard();
                drop(source);
            });
            if teardown.await.is_err() {
                tracing::error!("the audio source panicked");
            }
        }
    }

    fn send(&self, command: AudioCommand) {
        self.inbox.push(command);
    }

    /// Drops any recording and the commands not begun, and waits for the
    /// task to finish what it was doing (at most one blocking call), the
    /// final discard and the source's drop, both on the blocking pool.
    async fn shut_down(mut self) {
        self.inbox.close();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for AudioWorker {
    /// Without [`shut_down`](Self::shut_down) (the pipeline task panicked),
    /// the worker still ends after its current call, with the discard.
    fn drop(&mut self) {
        self.inbox.close();
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

    async fn run(mut self, mut keys: Keys, mut audio_failures: mpsc::Receiver<StartFailed>) {
        loop {
            let recording = matches!(self.state, State::Recording { .. });
            // Biased, in this order: shutdown wins over anything else that
            // is ready; audio failures and job results, at most a few per
            // dictation, come before keys so a flood of keys can't hold
            // them up (#81 M1); and keys already queued (stamped earlier)
            // come before a Tick.
            tokio::select! {
                biased;
                _ = self.shutdown.changed() => break,
                Some(failed) = audio_failures.recv() => {
                    if let Some(event) = self.start_failed(&failed) {
                        self.feed(event);
                    }
                }
                Some(joined) = self.jobs.join_next_with_id() => {
                    if let Some(event) = self.job_event(joined) {
                        self.feed(event);
                    }
                }
                Some((key, at)) = keys.rx.recv() => self.key(key, at, &keys.overflowed).await,
                _ = self.ticks.tick(), if recording => self.feed(Event::Tick(self.clock.now())),
            }
        }
        self.jobs.abort_all();
        self.hotkey.stop();
        self.audio.shut_down().await;
    }

    /// Feeds a key, then the cancel a dropped key calls for (see [`Keys`]),
    /// then a Tick that has come due: keys alone can't hold off the
    /// recording limit.
    async fn key(&mut self, key: HotkeyEvent, at: Duration, overflowed: &AtomicBool) {
        if let Some(event) = self.key_event(key, at) {
            self.feed(event);
        }
        if overflowed.swap(false, Ordering::AcqRel) {
            self.feed(Event::CancelPressed);
        }
        if matches!(self.state, State::Recording { .. }) && self.tick_due().await {
            self.feed(Event::Tick(self.clock.now()));
        }
    }

    /// Whether a Tick is due now, taking it if so; doesn't wait.
    async fn tick_due(&mut self) -> bool {
        tokio::select! {
            biased;
            _ = self.ticks.tick() => true,
            () = std::future::ready(()) => false,
        }
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
            // The start failed for want of the permission after the release
            // (#81 M2): the same error as when it fails while recording.
            JobResult::AudioFailed(job, AudioError::PermissionDenied) if job == self.job => {
                Some(Event::PermissionMissing(Permission::Microphone))
            }
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc as std_mpsc;
    use std::time::Instant;

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
        /// Blocks like `Block`, then fails.
        BlockThenFail(std_mpsc::Receiver<()>, AudioError),
    }

    /// An audio source whose `start` can block, fail or let `stop` panic,
    /// and that logs every call.
    #[derive(Clone)]
    struct ScriptedAudio {
        on_start: Arc<Mutex<Vec<OnStart>>>,
        panic_on_stop: bool,
        calls: Arc<Mutex<Vec<&'static str>>>,
        /// `start`s begun, counted on entry.
        entered: Arc<AtomicUsize>,
    }

    impl ScriptedAudio {
        fn new(on_start: Vec<OnStart>) -> Self {
            Self {
                on_start: Arc::new(Mutex::new(on_start)),
                panic_on_stop: false,
                calls: Arc::default(),
                entered: Arc::default(),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }

        /// Waits until `n` `start`s have begun.
        async fn entered(&self, n: usize) {
            wait_for("a start to begin", || {
                self.entered.load(Ordering::SeqCst) >= n
            })
            .await;
        }

        /// Waits until `n` calls have finished.
        async fn finished(&self, n: usize) {
            wait_for("audio calls", || self.calls().len() >= n).await;
        }
    }

    /// Polls `done` until it holds, failing after [`WAIT`].
    async fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + WAIT;
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    impl AudioSource for ScriptedAudio {
        fn start(&mut self) -> Result<(), AudioError> {
            self.entered.fetch_add(1, Ordering::SeqCst);
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
                OnStart::BlockThenFail(gate, error) => {
                    let _ = gate.recv();
                    Err(error)
                }
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
            Self::with_config(audio, Config::default())
        }

        fn with_config(audio: ScriptedAudio, config: Config) -> Self {
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
            let (pipeline, ui) = Pipeline::spawn(ports, config).unwrap();
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
        audio.entered(1).await;
        harness.emit(HotkeyEvent::CancelPressed);
        // Handled while `start` is still blocked.
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
        assert_eq!(audio.calls(), Vec::<&str>::new(), "start is still running");

        release.send(()).unwrap();
        audio.finished(2).await;
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
        audio.finished(2).await;
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

    /// A source whose drop blocks, like a warm microphone closing its
    /// device, until released or [`WAIT`] runs out.
    struct SlowTeardown {
        release: std_mpsc::Receiver<()>,
        dropped: Arc<AtomicBool>,
    }

    impl AudioSource for SlowTeardown {
        fn start(&mut self) -> Result<(), AudioError> {
            Ok(())
        }

        fn stop(&mut self) -> Result<Audio, AudioError> {
            Err(AudioError::NotRecording)
        }

        fn discard(&mut self) {}

        fn is_recording(&self) -> bool {
            false
        }
    }

    impl Drop for SlowTeardown {
        fn drop(&mut self) {
            let _ = self.release.recv_timeout(WAIT);
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    /// M2.3 criterion 3 (#77 M1). A current-thread runtime: had the source
    /// been dropped on the executor, the timer couldn't fire until the
    /// teardown ended.
    #[tokio::test]
    async fn m2_3_criterion_3_a_slow_teardown_does_not_stall_the_runtime() {
        let (release, gate) = std_mpsc::channel();
        let dropped = Arc::new(AtomicBool::new(false));
        let (worker, _failed) = AudioWorker::spawn(SlowTeardown {
            release: gate,
            dropped: Arc::clone(&dropped),
        });
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            let _ = release.send(());
        });

        let shutdown = tokio::spawn(worker.shut_down());
        let timer = Instant::now();
        tokio::time::sleep(Duration::from_millis(20)).await;
        let observed = timer.elapsed();

        shutdown.await.unwrap();
        releaser.join().unwrap();
        assert!(dropped.load(Ordering::SeqCst), "the source was dropped");
        assert!(
            observed < Duration::from_millis(250),
            "a 20 ms timer fired after {observed:?}"
        );
    }

    /// M2.3 criterion 6 (#81 B1): recordings cancelled while the first
    /// `start` blocks never start, and shutdown drops queued work: it waits
    /// only for the call in flight, then discards.
    #[tokio::test(flavor = "multi_thread")]
    async fn m2_3_criterion_6_cancelled_recordings_never_start_and_shutdown_drops_queued_work() {
        let (release, gate) = std_mpsc::channel();
        let audio = ScriptedAudio::new(vec![OnStart::Block(gate)]);
        let mut harness = Harness::start(audio.clone());

        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        audio.entered(1).await;
        for n in 0..6 {
            harness.emit(HotkeyEvent::CancelPressed);
            assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
            if n != 5 {
                harness.emit(HotkeyEvent::Released);
                harness.emit(HotkeyEvent::Pressed);
                assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
            }
        }

        let Harness {
            hotkey, pipeline, ..
        } = harness;
        let shutdown = tokio::spawn(pipeline.shutdown());
        wait_for("the hotkey to stop", || !hotkey.is_listening()).await;
        assert!(!shutdown.is_finished(), "waits for the start in flight");
        release.send(()).unwrap();
        tokio::time::timeout(WAIT, shutdown).await.unwrap().unwrap();
        assert_eq!(audio.calls(), ["start", "discard"]);
        assert_eq!(audio.entered.load(Ordering::SeqCst), 1);
    }

    /// M2.3 criterion 7 (#81 M1): a synchronous flood of keys while a
    /// `start` blocks queues at most [`KEYS_QUEUED`] keys and no audio work
    /// for the recordings it cancels; the dictation after it works.
    #[tokio::test]
    async fn m2_3_criterion_7_a_key_flood_is_bounded_and_queues_no_audio_work() {
        let (release, gate) = std_mpsc::channel();
        let audio = ScriptedAudio::new(vec![OnStart::Block(gate)]);
        let mut harness = Harness::start(audio.clone());
        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        audio.entered(1).await;
        harness.emit(HotkeyEvent::CancelPressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));

        // A current-thread runtime: the pipeline can't run during this.
        for _ in 0..10_000 {
            harness.emit(HotkeyEvent::Released);
            harness.emit(HotkeyEvent::Pressed);
            harness.emit(HotkeyEvent::CancelPressed);
        }
        // Let it work through what was queued.
        let mut recordings = 0;
        while let Ok(Some(event)) =
            tokio::time::timeout(Duration::from_millis(100), harness.ui.recv()).await
        {
            if event == UiEvent::StateChanged(Recording) {
                recordings += 1;
            }
        }
        assert!(
            (1..=KEYS_QUEUED / 3 + 1).contains(&recordings),
            "{recordings} recordings from the queued keys"
        );

        release.send(()).unwrap();
        audio.finished(2).await;
        assert_eq!(
            audio.calls(),
            ["start", "discard"],
            "no cancelled start ran"
        );
        let events = harness.dictate().await;
        assert_eq!(events[0], UiEvent::StateChanged(Recording));
        assert_eq!(events.last(), Some(&UiEvent::StateChanged(Idle)));
        harness.pipeline.shutdown().await;
        assert_eq!(
            audio.calls(),
            ["start", "discard", "start", "stop", "discard"]
        );
    }

    /// A dropped key could be the release: the recording is cancelled
    /// rather than left running.
    #[tokio::test]
    async fn a_dropped_key_cancels_the_recording() {
        let audio = ScriptedAudio::new(Vec::new());
        let mut harness = Harness::start(audio.clone());
        harness.emit(HotkeyEvent::Pressed);
        for _ in 0..(2 * KEYS_QUEUED) {
            // Repeats while recording: ignored, but they fill the queue.
            harness.emit(HotkeyEvent::Pressed);
        }
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
        harness.pipeline.shutdown().await;
        // Depending on when the worker ran, the Start was dropped by the
        // cancel or ran and was discarded; either way it ends discarded.
        let calls = audio.calls();
        assert_eq!(calls.last(), Some(&"discard"), "{calls:?}");
        assert!(!calls.contains(&"stop"), "{calls:?}");
    }

    /// #81 M1: keys arriving nonstop don't hold off the recording limit:
    /// each key also feeds a Tick that has come due.
    #[tokio::test]
    async fn a_key_feeds_a_tick_that_came_due() {
        let (ui, _ui) = mpsc::unbounded_channel();
        let (_shutdown, shutdown_rx) = watch::channel(false);
        let (audio, _) = AudioWorker::spawn(ScriptedAudio::new(Vec::new()));
        let clock = FakeClock::new();
        let mut runner = Runner::new(
            FakeHotkey::new(),
            audio,
            FixedTranscriber,
            FakeInserter::new(),
            FakePermissions::all_granted(),
            Arc::new(clock.clone()),
            ui,
            shutdown_rx,
            tokio::time::interval(Duration::from_millis(1)),
        );
        let overflowed = AtomicBool::new(false);
        runner
            .key(HotkeyEvent::Pressed, Duration::ZERO, &overflowed)
            .await;
        assert!(matches!(runner.state, State::Recording { .. }));

        clock.advance(dictation_core::coordinator::MAX_RECORDING);
        tokio::time::sleep(Duration::from_millis(5)).await;
        // A repeat, ignored by itself; the Tick due ends the recording.
        runner
            .key(HotkeyEvent::Pressed, clock.now(), &overflowed)
            .await;
        assert_eq!(runner.state, State::Transcribing);
    }

    /// M2.3 criterion 8 (#81 M2): the release comes before the `start`
    /// fails for want of the permission. The user still hears that the
    /// permission is missing, and `stop` isn't called on a failed start.
    #[tokio::test(flavor = "multi_thread")]
    async fn m2_3_criterion_8_a_permission_denied_start_after_the_release_is_reported() {
        let (release, gate) = std_mpsc::channel();
        let audio = ScriptedAudio::new(vec![OnStart::BlockThenFail(
            gate,
            AudioError::PermissionDenied,
        )]);
        let mut harness = Harness::start(audio.clone());
        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        audio.entered(1).await;
        harness.clock.advance(Duration::from_millis(500));
        harness.emit(HotkeyEvent::Released);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Transcribing));

        release.send(()).unwrap();
        assert_eq!(
            harness.next().await,
            UiEvent::Error(DictationError::PermissionMissing(Permission::Microphone))
        );
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
        harness.pipeline.shutdown().await;
        assert_eq!(audio.calls(), ["start", "discard"]);
        assert!(harness.ui.try_recv().is_err(), "one error only");
    }

    #[test]
    fn the_audio_queue_drops_abandoned_starts_and_stays_small() {
        use AudioCommand::{Discard, Start, Stop};
        let kinds = |queue: &AudioQueue| {
            queue
                .commands
                .iter()
                .map(|command| match command {
                    Start(_) => "start",
                    Stop(_) => "stop",
                    Discard => "discard",
                })
                .collect::<Vec<_>>()
        };
        let mut queue = AudioQueue::default();
        // The first start is in flight (taken); the user cancels, then
        // presses and cancels again and again.
        queue.push(Discard);
        for session in 2..100 {
            queue.push(Start(session));
            queue.push(Discard);
        }
        assert_eq!(kinds(&queue), ["discard"]);
        // Then a dictation that stops.
        let (reply, _captured) = oneshot::channel();
        queue.push(Start(100));
        queue.push(Stop(reply));
        assert_eq!(kinds(&queue), ["discard", "start", "stop"]);

        let mut closed = AudioQueue {
            closed: true,
            ..AudioQueue::default()
        };
        closed.push(Start(1));
        assert!(closed.commands.is_empty());
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
