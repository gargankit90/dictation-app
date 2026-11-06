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
//! The queues into and out of this task are bounded (#81, #84): hotkey events
//! ([`KEYS_QUEUED`]; a dropped key discards everything queued and cancels,
//! see [`Keys`]), the audio worker's commands (at most three: a cancelled
//! recording's `Start` is dropped before it begins, see [`AudioQueue`]), its
//! start failures, and the UI events (see [`crate::ui_events`] for what a UI
//! that stops reading gets).
//!
//! While recording, the task also reads the source's [`LevelMeter`] every
//! [`LEVEL_INTERVAL`] and publishes the input level on a `watch` channel
//! ([`Pipeline::levels`], U4) for the recording pill. A watch keeps only the
//! newest reading, so a reader that falls behind can't make anything grow.
//! A level is a number only: never logged at info level, stored or sent.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use dictation_core::audio_math::{self, Verdict};
use dictation_core::coordinator::{Phase, State, step};
use dictation_core::error::DictationError;
use dictation_core::events::{Command, Event};
use dictation_core::text::{Cleaned, Transcript, clean};
use dictation_ports::{
    Audio, AudioError, AudioSource, Clock, HotkeyError, HotkeyEvent, HotkeySource, InsertError,
    LevelMeter, Permission, PermissionStatus, Permissions, TextBeforeCursor, TextInserter,
    TranscribeError, Transcriber,
};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{Notify, mpsc, oneshot, watch};
use tokio::task::{self, JoinError, JoinHandle, JoinSet};
use tokio::time::{Interval, MissedTickBehavior};

use crate::ui_events::{UiReceiver, UiSender, ui_channel};

/// What the UI hears about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    /// The coordinator moved to this phase.
    StateChanged(Phase),
    /// A dictation failed; show this to the user.
    Error(DictationError),
    /// The hotkey listens again after it stopped working
    /// ([`DictationError::HotkeyFailed`]); stop showing that.
    HotkeyRestored,
    /// The recording that just ended had no sound and lasted long enough to
    /// say so (H2). Not an error: only the overlay shows it.
    NoSound,
}

/// The input level while recording (U4): what [`Pipeline::levels`]
/// publishes.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelReading {
    /// The RMS of the audio since the previous reading, in dBFS
    /// ([`dictation_core::level::rms_to_dbfs`]); the floor when no audio
    /// arrived.
    pub dbfs: f32,
    /// The input device's name, if the source knows it.
    pub device: Option<String>,
}

/// The pipeline's input levels: `Some` reading about every
/// [`LEVEL_INTERVAL`] while recording, `None` otherwise.
pub type LevelReceiver = watch::Receiver<Option<LevelReading>>;

/// How often the pipeline reads the input level while recording: 20 times a
/// second.
pub const LEVEL_INTERVAL: Duration = Duration::from_millis(50);

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

/// How long a dictation waits for the text before the cursor before it
/// pastes without a smart leading space. A healthy app answers in a few
/// milliseconds; this bounds what a hung one can add to the paste.
pub const CURSOR_READ_TIMEOUT: Duration = Duration::from_millis(200);

/// Reads the character before the cursor for the smart leading space (#93),
/// once per dictation, right after the transcript arrives and only if there
/// is text to paste. What it reads is used for that one transcript and
/// never logged.
#[derive(Clone)]
pub struct CursorReader {
    reader: Arc<dyn TextBeforeCursor>,
    timeout: Duration,
}

impl CursorReader {
    /// Reads through `reader`, waiting at most [`CURSOR_READ_TIMEOUT`].
    pub fn new(reader: impl TextBeforeCursor + 'static) -> Self {
        Self {
            reader: Arc::new(reader),
            timeout: CURSOR_READ_TIMEOUT,
        }
    }

    /// Waits at most `timeout` instead.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The character before the cursor, or `None` (no space is added) if
    /// the cursor is at the start of a field, the reader fails, or it
    /// doesn't answer within the timeout. The read runs on the blocking
    /// pool; one that times out is left to finish there and its answer is
    /// dropped.
    async fn preceding(&self) -> Option<char> {
        let reader = Arc::clone(&self.reader);
        let read = task::spawn_blocking(move || reader.char_before_cursor());
        match tokio::time::timeout(self.timeout, read).await {
            Ok(Ok(Ok(before))) => before.preceding(),
            Ok(Ok(Err(error))) => {
                tracing::debug!(%error, "no text before the cursor; no leading space");
                None
            }
            Ok(Err(_)) => {
                // The panic message isn't logged: it could hold text.
                tracing::warn!("reading the text before the cursor crashed; no leading space");
                None
            }
            Err(_) => {
                tracing::debug!(timeout = ?self.timeout, "the focused app did not answer in time; no leading space");
                None
            }
        }
    }
}

impl std::fmt::Debug for CursorReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CursorReader")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// Controls a running pipeline.
#[derive(Debug)]
pub struct Pipeline {
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
    levels: LevelReceiver,
}

impl Pipeline {
    /// Starts the hotkey and the pipeline task on the current tokio runtime
    /// and returns the UI event stream. Nothing reads the text before the
    /// cursor, so no smart leading space is added; see
    /// [`spawn_with_cursor`](Self::spawn_with_cursor).
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
    ) -> Result<(Self, UiReceiver), HotkeyError>
    where
        H: HotkeySource + 'static,
        A: AudioSource + 'static,
        T: Transcriber + 'static,
        I: TextInserter + 'static,
        P: Permissions + 'static,
        C: Clock + 'static,
    {
        Self::start(ports, None, config)
    }

    /// Like [`spawn`](Self::spawn), and each transcript gets a smart leading
    /// space from the character `cursor` reads before the cursor.
    ///
    /// # Errors
    ///
    /// The hotkey source's error if it can't start; nothing runs then.
    ///
    /// # Panics
    ///
    /// Outside a tokio runtime.
    pub fn spawn_with_cursor<H, A, T, I, P, C>(
        ports: Ports<H, A, T, I, P, C>,
        cursor: CursorReader,
        config: Config,
    ) -> Result<(Self, UiReceiver), HotkeyError>
    where
        H: HotkeySource + 'static,
        A: AudioSource + 'static,
        T: Transcriber + 'static,
        I: TextInserter + 'static,
        P: Permissions + 'static,
        C: Clock + 'static,
    {
        Self::start(ports, Some(cursor), config)
    }

    fn start<H, A, T, I, P, C>(
        ports: Ports<H, A, T, I, P, C>,
        cursor: Option<CursorReader>,
        config: Config,
    ) -> Result<(Self, UiReceiver), HotkeyError>
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
            overflow: Arc::new(Overflow::default()),
        };
        let overflow = Arc::clone(&keys.overflow);
        let stamp = Arc::clone(&clock);
        hotkey.start(Box::new(move |key| {
            if let Err(TrySendError::Full(_)) = keys_tx.try_send((key, stamp.now())) {
                if !overflow.dropped.swap(true, Ordering::AcqRel) {
                    tracing::warn!("hotkey events are queueing up; discarding them and cancelling");
                }
                overflow.noticed.notify_one();
            }
        }))?;

        let (ui, ui_events) = ui_channel();
        let (shutdown, shutdown_rx) = watch::channel(false);
        let meter = audio.level_meter();
        let (levels_tx, levels) = watch::channel(None);
        let (audio, audio_failures) = AudioWorker::spawn(audio);
        let mut runner = Runner::new(
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
        runner.cursor = cursor;
        runner.meter = meter;
        runner.levels = levels_tx;
        let task = tokio::spawn(runner.run(keys, audio_failures));
        Ok((
            Self {
                shutdown,
                task,
                levels,
            },
            ui_events,
        ))
    }

    /// The input level while recording (U4), for the recording pill: about
    /// 20 readings a second while recording, `None` otherwise. Nothing is
    /// published for a source without a [`LevelMeter`].
    #[must_use]
    pub fn levels(&self) -> LevelReceiver {
        self.levels.clone()
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

/// Ticks every [`LEVEL_INTERVAL`]; a late tick is skipped, not made up.
fn level_interval() -> Interval {
    let mut ticks = tokio::time::interval(LEVEL_INTERVAL);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ticks
}

/// The event for a transcript once cleaned up (`dictation_core::text`),
/// with `preceding` the character before the cursor if known (the smart
/// leading space): the text to insert, or "nothing to paste" (no error).
/// Never logs the text or `preceding`.
fn cleaned_event(transcript: &Transcript, preceding: Option<char>) -> Event {
    match clean(transcript, preceding) {
        Cleaned::Text(text) => Event::TranscriptReady(text),
        Cleaned::Dropped(why) => {
            tracing::debug!(?why, "transcript dropped; nothing to paste");
            Event::TranscriptDropped
        }
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
/// running, or a press that the queued keys no longer make sense without. So
/// once one is dropped none of the keys still queued can be trusted either:
/// the task throws them all away and acts as if Esc was pressed. A recording
/// in progress is discarded, and anything else is unaffected (Esc is ignored
/// while transcribing or inserting). The task is told as soon as the key is
/// dropped ([`Overflow`]), whether or not another key ever comes (#91).
struct Keys {
    rx: mpsc::Receiver<(HotkeyEvent, Duration)>,
    overflow: Arc<Overflow>,
}

/// Says a hotkey event was dropped: set by the hotkey thread, handled by the
/// pipeline task.
#[derive(Default)]
struct Overflow {
    /// A key was dropped and not yet handled.
    dropped: AtomicBool,
    /// Wakes the task; keeps a permit if it isn't waiting.
    noticed: Notify,
}

/// A finished background job, tagged with the job it belongs to.
enum JobResult {
    /// The transcription's result and, if there is text to paste and a
    /// [`CursorReader`], the character before the cursor right after it.
    Transcribed(u64, Result<Transcript, TranscribeError>, Option<char>),
    /// The audio was empty, too short or silent, so nothing was sent to the
    /// transcriber (#69, #72).
    NotTranscribed(u64),
    /// [`JobResult::NotTranscribed`], and the clip was silent for at least
    /// [`audio_math::SILENT_NOTICE_MIN`]: the person is told (H2).
    NotTranscribedSilent(u64),
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

/// What the permissions say about a hotkey press
/// ([`Runner::press_permissions`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PressPermissions {
    /// Everything is granted: the press records.
    Ready,
    /// The OS was just asked to prompt for the microphone; the user hasn't
    /// answered yet.
    Asking,
    /// This permission is missing: the press shows the error.
    Missing(Permission),
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
    /// Reads the character before the cursor, if set (the smart leading
    /// space).
    cursor: Option<CursorReader>,
    permissions: P,
    clock: Arc<C>,
    ui: UiSender,
    /// Becomes `true` (or closes) when the pipeline should stop.
    shutdown: watch::Receiver<bool>,
    /// Ticks while recording; reset when a recording starts.
    ticks: Interval,
    /// The source's input level, if it measures one.
    meter: Option<LevelMeter>,
    /// Where the levels go ([`Pipeline::levels`]).
    levels: watch::Sender<Option<LevelReading>>,
    /// Ticks every [`LEVEL_INTERVAL`] while recording; reset when a
    /// recording starts.
    level_ticks: Interval,
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
    /// How long the shutdown waits for the inserter to finish its
    /// clipboard work ([`FINISH_BOUND`]).
    finish_bound: Duration,
}

/// How long the shutdown waits for [`TextInserter::finish`]: the restore
/// delay (1.5 s) plus some room. The shell's main thread waits on the
/// shutdown, so this keeps Quit and Relaunch from ever hanging on it.
pub const FINISH_BOUND: Duration = Duration::from_secs(2);

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
        ui: UiSender,
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
            cursor: None,
            permissions,
            clock,
            ui,
            shutdown,
            ticks,
            meter: None,
            levels: watch::channel(None).0,
            level_ticks: level_interval(),
            jobs: JoinSet::new(),
            running: None,
            captured: None,
            job: 0,
            pending: VecDeque::new(),
            finish_bound: FINISH_BOUND,
        }
    }

    async fn run(mut self, mut keys: Keys, mut audio_failures: mpsc::Receiver<StartFailed>) {
        loop {
            let recording = matches!(self.state, State::Recording { .. });
            let metering = recording && self.meter.is_some();
            // Biased, in this order: shutdown wins over anything else that
            // is ready; audio failures and job results, at most a few per
            // dictation, come before keys so a flood of keys can't hold
            // them up (#81 M1); and keys already queued (stamped earlier)
            // come before a Tick.
            tokio::select! {
                biased;
                _ = self.shutdown.changed() => break,
                // Before the keys: those still queued are thrown away.
                () = keys.overflow.noticed.notified() => {
                    self.keys_overflowed(&mut keys);
                }
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
                Some((key, at)) = keys.rx.recv() => self.key(key, at).await,
                _ = self.ticks.tick(), if recording => self.feed(Event::Tick(self.clock.now())),
                _ = self.level_ticks.tick(), if metering => self.publish_level(),
            }
        }
        self.jobs.abort_all();
        self.hotkey.stop();
        self.audio.shut_down().await;
        // The user's clipboard may still be owed from the last paste (it
        // goes back a moment after the chord, on a timer the exit would
        // kill): finish it before the app exits (#241).
        // Bounded: the shell's main thread is blocked on this shutdown, so
        // nothing here may wait on it for long. If the bound passes, the
        // quit goes on and that one restore may be skipped.
        let inserter = Arc::clone(&self.inserter);
        let finish = task::spawn_blocking(move || inserter.finish());
        match tokio::time::timeout(self.finish_bound, finish).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => tracing::warn!("finishing the clipboard restore failed"),
            Err(_) => tracing::warn!(
                bound = ?self.finish_bound,
                "the clipboard restore didn't finish in time; quitting without it"
            ),
        }
    }

    /// Feeds a key, then a Tick that has come due: keys alone can't hold off
    /// the recording limit.
    async fn key(&mut self, key: HotkeyEvent, at: Duration) {
        if key == HotkeyEvent::Restored {
            // Nothing for the coordinator: the hotkey works again, so the
            // UI stops saying it doesn't.
            self.ui.send(UiEvent::HotkeyRestored);
            return;
        }
        if let Some(event) = self.key_event(key, at) {
            self.feed(event);
        }
        if matches!(self.state, State::Recording { .. }) && self.tick_due().await {
            self.feed(Event::Tick(self.clock.now()));
        }
    }

    /// A hotkey event was dropped (see [`Keys`]): throws away the keys still
    /// queued and cancels, so the pipeline ends without a recording or a
    /// transcription of its own, with no key press needed to get there.
    ///
    /// Returns how many queued keys it threw away, or `None` if no key was
    /// dropped after all (a wake-up left over from an overflow already
    /// handled: the hotkey thread wakes the task once per dropped key).
    fn keys_overflowed(&mut self, keys: &mut Keys) -> Option<usize> {
        if !keys.overflow.dropped.swap(false, Ordering::AcqRel) {
            return None;
        }
        let mut discarded = 0_usize;
        while keys.rx.try_recv().is_ok() {
            discarded += 1;
        }
        tracing::warn!(discarded, "hotkey events overflowed; cancelled");
        self.feed(Event::CancelPressed);
        Some(discarded)
    }

    /// Publishes the input level read since the last reading. Nothing while
    /// the source hasn't started measuring yet (its `start` still running).
    fn publish_level(&self) {
        let Some(meter) = &self.meter else { return };
        if let Some(dbfs) = meter.take() {
            self.levels.send_replace(Some(LevelReading {
                dbfs,
                device: meter.device(),
            }));
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
            HotkeyEvent::Pressed => match self.press_permissions() {
                PressPermissions::Ready => Event::HotkeyPressed(at),
                PressPermissions::Missing(permission) => Event::PermissionMissing(permission),
                // The macOS prompt is up: nothing to show yet (#161), and
                // the coordinator stays Idle, so nothing records or pastes.
                PressPermissions::Asking => return None,
            },
            HotkeyEvent::Released => Event::HotkeyReleased(at),
            // A shortcut on the hotkey's modifier (#172) drops the recording
            // exactly as Esc does: quietly, with nothing transcribed or
            // pasted; outside a recording it changes nothing.
            HotkeyEvent::CancelPressed | HotkeyEvent::ShortcutPressed => Event::CancelPressed,
            HotkeyEvent::Interrupted => Event::HotkeyInterrupted,
            HotkeyEvent::Failed => Event::HotkeyFailed,
            HotkeyEvent::Restored => return None,
        })
    }

    /// Whether a press may record, asking the OS to prompt for a
    /// permission the user hasn't been asked for yet. Checked only when
    /// idle: a press while busy is ignored by the coordinator anyway.
    ///
    /// An undetermined microphone is [`PressPermissions::Asking`], not
    /// missing (#161): the prompt is on screen and the user may still
    /// allow it, and then the next press records. Only a refusal (or a
    /// prompt that couldn't be shown) is the error. Accessibility has no
    /// such state on macOS (refused reads as undetermined), so it is
    /// missing either way.
    fn press_permissions(&self) -> PressPermissions {
        if self.state != State::Idle {
            return PressPermissions::Ready;
        }
        for permission in [Permission::Microphone, Permission::Accessibility] {
            match self.permissions.check(permission) {
                PermissionStatus::Granted => {}
                PermissionStatus::Denied => return PressPermissions::Missing(permission),
                PermissionStatus::NotDetermined => {
                    let asked = self.permissions.request(permission);
                    if let Err(error) = &asked {
                        tracing::warn!(?permission, %error, "permission request failed");
                    }
                    return match (permission, asked) {
                        (Permission::Microphone, Ok(())) => {
                            tracing::info!("waiting for the microphone prompt's answer");
                            PressPermissions::Asking
                        }
                        _ => PressPermissions::Missing(permission),
                    };
                }
            }
        }
        PressPermissions::Ready
    }

    fn job_event(&self, joined: Result<(task::Id, JobResult), JoinError>) -> Option<Event> {
        let result = match joined {
            Ok((_, result)) => result,
            Err(error) => return self.job_crashed(&error),
        };
        match result {
            JobResult::Transcribed(job, result, preceding) if job == self.job => {
                Some(match result {
                    Ok(transcript) => cleaned_event(&transcript, preceding),
                    Err(error) => Event::TranscriptionFailed(error.user_message()),
                })
            }
            // Nothing to paste, and no error to show: straight back to Idle.
            JobResult::NotTranscribed(job) if job == self.job => Some(Event::TranscriptDropped),
            JobResult::NotTranscribedSilent(job) if job == self.job => {
                Some(Event::TranscriptSilent)
            }
            JobResult::Inserted(job, result) if job == self.job => Some(match result {
                Ok(()) => Event::InsertDone,
                Err(InsertError::SecureInput) => Event::SecureInputBlocked,
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
            JobResult::Transcribed(job, ..)
            | JobResult::NotTranscribed(job)
            | JobResult::NotTranscribedSilent(job)
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
                self.level_ticks.reset();
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
                self.ui.send(UiEvent::Error(error));
            }
            Command::ShowNoSound => self.ui.send(UiEvent::NoSound),
            Command::StateChanged(phase) => {
                if phase != Phase::Recording {
                    // No level outside a recording.
                    self.levels.send_if_modified(|level| level.take().is_some());
                }
                self.ui.send(UiEvent::StateChanged(phase));
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
        let cursor = self.cursor.clone();
        // The job waits for the audio worker's stop, then transcribes, then
        // reads the character before the cursor.
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
            // Nothing worth sending: no request, no invented words (#69, #72).
            // The coordinator takes TranscriptDropped (no error) back to Idle.
            let verdict = audio_math::assess(
                &audio.samples,
                audio.sample_rate,
                audio_math::DEFAULT_SILENCE_THRESHOLD,
            );
            if verdict != Verdict::Transcribe {
                tracing::info!(job, ?verdict, duration = ?audio.duration(), "not transcribing");
                return if audio_math::silent_clip_is_worth_telling(verdict, audio.duration()) {
                    JobResult::NotTranscribedSilent(job)
                } else {
                    JobResult::NotTranscribed(job)
                };
            }
            tracing::debug!(job, duration = ?audio.duration(), "transcribing");
            let result = transcriber.transcribe_detailed(&audio).await;
            let preceding = match (&result, cursor) {
                // Only when there is text to paste: nothing is read for a
                // failure or a dropped transcript.
                (Ok(transcript), Some(cursor))
                    if matches!(clean(transcript, None), Cleaned::Text(_)) =>
                {
                    cursor.preceding().await
                }
                _ => None,
            };
            JobResult::Transcribed(job, result, preceding)
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

    use dictation_core::coordinator::Phase::{Idle, Recording, Transcribing};
    use dictation_platform::fakes::{FakeClock, FakeHotkey, FakeInserter, FakePermissions};

    use super::*;

    /// Captures 300 ms of constant signal, instantly: long and loud enough
    /// to reach the transcriber (silence and clips under 200 ms don't, and
    /// a job that ended at once would race with the results the tests send).
    struct BurstAudio(bool);

    impl AudioSource for BurstAudio {
        fn start(&mut self) -> Result<(), AudioError> {
            self.0 = true;
            Ok(())
        }

        fn stop(&mut self) -> Result<Audio, AudioError> {
            self.0 = false;
            Ok(Audio {
                sample_rate: 16_000,
                samples: vec![0.1; 4_800],
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
        let (ui, _) = ui_channel();
        let (shutdown, shutdown_rx) = watch::channel(false);
        let (audio, _) = AudioWorker::spawn(BurstAudio(false));
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

    /// An inserter whose `finish` never returns on its own: an insert in
    /// flight that needs a main-queue callback while the main thread waits
    /// on this shutdown (#241 round 4's deadlock). `release` lets it go at
    /// the end of the test.
    #[derive(Clone)]
    struct StuckFinish {
        entered: Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>>,
        release: Arc<std::sync::Mutex<std::sync::mpsc::Receiver<()>>>,
    }

    impl TextInserter for StuckFinish {
        fn insert(&self, _: &str) -> Result<(), InsertError> {
            Ok(())
        }

        fn finish(&self) {
            if let Some(entered) = self.entered.lock().unwrap().take() {
                let _ = entered.send(());
            }
            // Waits for a "main-queue callback" that can't come while the
            // main thread is blocked in the shutdown.
            let _ = self.release.lock().unwrap().recv();
        }
    }

    /// #241 round 4: the shutdown's wait on `finish` is bounded, so a stuck
    /// clipboard restore can't hang Quit or Relaunch.
    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_does_not_hang_on_a_stuck_finish() {
        let (entered_tx, entered) = std::sync::mpsc::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let inserter = StuckFinish {
            entered: Arc::new(std::sync::Mutex::new(Some(entered_tx))),
            release: Arc::new(std::sync::Mutex::new(release_rx)),
        };
        let (ui, _) = ui_channel();
        let (shutdown, shutdown_rx) = watch::channel(false);
        let (audio, failures) = AudioWorker::spawn(BurstAudio(false));
        let mut runner = Runner::new(
            FakeHotkey::new(),
            audio,
            SilentTranscriber,
            inserter,
            FakePermissions::all_granted(),
            Arc::new(FakeClock::new()),
            ui,
            shutdown_rx,
            tokio::time::interval(Duration::from_millis(250)),
        );
        assert_eq!(runner.finish_bound, FINISH_BOUND);
        assert_eq!(FINISH_BOUND, Duration::from_secs(2));
        runner.finish_bound = Duration::from_millis(100);
        let (_keys_tx, keys_rx) = mpsc::channel(KEYS_QUEUED);
        let keys = Keys {
            rx: keys_rx,
            overflow: Arc::new(Overflow::default()),
        };
        let started = std::time::Instant::now();
        let task = tokio::spawn(runner.run(keys, failures));
        shutdown.send(true).unwrap();
        // Ends after the bound, though `finish` is still stuck.
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("the shutdown hung on finish")
            .unwrap();
        assert!(entered.try_recv().is_ok(), "finish was called");
        assert!(started.elapsed() >= Duration::from_millis(100));
        release.send(()).unwrap();
    }

    /// #241 round 3: a Quit (or a relaunch) right after a dictation must
    /// not exit before the user's clipboard is back; the pipeline's
    /// shutdown has the inserter finish its pending restore.
    #[tokio::test(flavor = "multi_thread")]
    async fn shutdown_has_the_inserter_finish_its_clipboard_work() {
        let inserter = FakeInserter::new();
        let (runner, shutdown) = runner(&inserter);
        let (_keys_tx, keys_rx) = mpsc::channel(KEYS_QUEUED);
        let keys = Keys {
            rx: keys_rx,
            overflow: Arc::new(Overflow::default()),
        };
        let (_failures_tx, failures) = mpsc::channel(1);
        let task = tokio::spawn(runner.run(keys, failures));
        assert_eq!(inserter.finished(), 0);
        shutdown.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(inserter.finished(), 1);
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

    /// A log sink the test reads back.
    #[derive(Clone, Default)]
    struct LogBuffer(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for LogBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl LogBuffer {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    /// #65 and M5.3 criterion 1: a whole dictation, logged at every level
    /// (trace includes the debug "step" line that prints each event and its
    /// commands), never prints the transcript, and a failure's text carries
    /// no key. Checks the log text through the tracing subscriber.
    #[tokio::test]
    async fn criterion_1_the_step_log_and_error_logs_hold_no_transcript_or_key() {
        const SECRET_WORDS: &str = "private words about the merger";
        const KEY: &str = "gsk_dummy_NOT_A_REAL_KEY_0123456789";
        let buffer = LogBuffer::default();
        let writer = buffer.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let _logging = tracing::subscriber::set_default(subscriber);

        let inserter = FakeInserter::new();
        let (mut runner, _shutdown) = runner(&inserter);
        // A dictation that works: the transcript is the TranscriptReady
        // event and the Insert command, both logged by `step`.
        key(&mut runner, HotkeyEvent::Pressed, 0);
        key(&mut runner, HotkeyEvent::Released, 500);
        deliver(
            &mut runner,
            JobResult::Transcribed(1, Ok(SECRET_WORDS.into()), None),
        );
        let inserted = next_insert(&mut runner).await;
        deliver(&mut runner, inserted);
        assert_eq!(inserter.inserted(), [SECRET_WORDS]);
        // One that fails with a provider message holding a key. (An engine
        // never copies a reply body into its error; see
        // `dictation-transcribe`'s `provider_bodies_never_reach_the_error`.)
        key(&mut runner, HotkeyEvent::Pressed, 1_000);
        key(&mut runner, HotkeyEvent::Released, 1_500);
        let reply = format!("Invalid key {KEY}");
        deliver(
            &mut runner,
            JobResult::Transcribed(3, Err(TranscribeError::Engine(reply)), None),
        );
        assert_eq!(runner.state, State::Idle);

        let log = buffer.text();
        assert!(log.contains("step"), "the step line was logged:\n{log}");
        assert!(log.contains("dictation failed"), "{log}");
        assert!(!log.contains("merger"), "transcript in the log:\n{log}");
        assert!(
            !log.contains("private words"),
            "transcript in the log:\n{log}"
        );
        assert!(!log.contains(KEY), "key in the log:\n{log}");
        assert!(
            log.contains(&format!("len: {}", SECRET_WORDS.len())),
            "the length is shown instead:\n{log}"
        );
    }

    #[tokio::test]
    async fn a_late_transcript_from_an_earlier_dictation_is_never_inserted() {
        let inserter = FakeInserter::new();
        let (mut runner, _shutdown) = runner(&inserter);

        // Dictation 1 (job 1) completes.
        key(&mut runner, HotkeyEvent::Pressed, 0);
        key(&mut runner, HotkeyEvent::Released, 500);
        assert_eq!(runner.job, 1);
        deliver(
            &mut runner,
            JobResult::Transcribed(1, Ok("first".into()), None),
        );
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
        deliver(
            &mut runner,
            JobResult::Transcribed(1, Ok("stale".into()), None),
        );
        assert_eq!(runner.state, State::Transcribing);

        // A late insert result can't end the current dictation either.
        deliver(
            &mut runner,
            JobResult::Inserted(2, Err(InsertError::SecureInput)),
        );
        assert_eq!(runner.state, State::Transcribing);

        deliver(
            &mut runner,
            JobResult::Transcribed(3, Ok("third".into()), None),
        );
        let inserted = next_insert(&mut runner).await;
        deliver(&mut runner, inserted);
        assert_eq!(runner.state, State::Idle);
        assert_eq!(inserter.inserted(), ["first", "third"]);
    }

    /// M4.2 with M2.3: both ways a dictation ends without a paste (nothing to
    /// paste, Secure Input) leave the coordinator in Idle with the audio
    /// worker's bounded queue ready, so the next dictation starts and
    /// transcribes. Only the block is an error.
    #[tokio::test]
    async fn a_dropped_transcript_and_a_secure_input_block_both_leave_the_next_dictation_working() {
        let inserter = FakeInserter::new();
        let (mut runner, _shutdown) = runner(&inserter);
        let (ui, mut ui_events) = ui_channel();
        runner.ui = ui;

        // Dictation 1: a blank transcript goes straight to Idle, no error.
        key(&mut runner, HotkeyEvent::Pressed, 0);
        key(&mut runner, HotkeyEvent::Released, 500);
        deliver(
            &mut runner,
            JobResult::Transcribed(1, Ok("  ".into()), None),
        );
        assert_eq!(runner.state, State::Idle);
        assert_eq!(
            runner.jobs.len(),
            1,
            "no insert job for a dropped transcript"
        );

        // Dictation 2 (jobs 2 and 3: transcribe, insert): the paste is blocked by
        // Secure Input.
        key(&mut runner, HotkeyEvent::Pressed, 1_000);
        key(&mut runner, HotkeyEvent::Released, 1_500);
        deliver(
            &mut runner,
            JobResult::Transcribed(2, Ok("hello".into()), None),
        );
        assert_eq!(runner.state, State::Inserting);
        deliver(
            &mut runner,
            JobResult::Inserted(3, Err(InsertError::SecureInput)),
        );
        assert_eq!(runner.state, State::Idle);

        // Dictation 3 starts and reaches Transcribing as usual.
        key(&mut runner, HotkeyEvent::Pressed, 2_000);
        key(&mut runner, HotkeyEvent::Released, 2_500);
        assert_eq!((runner.state, runner.job), (State::Transcribing, 4));

        let mut errors = Vec::new();
        while let Ok(event) = ui_events.try_recv() {
            if let UiEvent::Error(error) = event {
                errors.push(error);
            }
        }
        assert_eq!(errors, [DictationError::SecureInput]);
        runner.jobs.abort_all();
        while runner.jobs.join_next().await.is_some() {}
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
        deliver(
            &mut runner,
            JobResult::Transcribed(1, Ok("late".into()), None),
        );
        assert_eq!(runner.state, State::Inserting);
        // Only the (never-ending) transcription; no insert was started.
        assert_eq!(runner.jobs.len(), 1);
        runner.jobs.abort_all();
        while runner.jobs.join_next().await.is_some() {}
        assert!(inserter.inserted().is_empty());
    }

    /// M5.9 criterion 1 (#172): a shortcut on the hotkey's modifier drops
    /// the recording it started, at any hold length: back to Idle with no
    /// transcription, no insert and no error; its release then does
    /// nothing, and the next hold dictates as usual.
    #[tokio::test]
    async fn criterion_1_a_shortcut_drops_the_recording_quietly() {
        let inserter = FakeInserter::new();
        let (mut runner, _shutdown) = runner(&inserter);
        let (ui, mut ui_events) = ui_channel();
        runner.ui = ui;
        for (start, shortcut_at, release_at) in [(0, 100, 150), (1_000, 1_500, 1_700)] {
            key(&mut runner, HotkeyEvent::Pressed, start);
            assert!(matches!(runner.state, State::Recording { .. }));
            key(&mut runner, HotkeyEvent::ShortcutPressed, shortcut_at);
            assert_eq!(runner.state, State::Idle);
            key(&mut runner, HotkeyEvent::Released, release_at);
            assert_eq!(runner.state, State::Idle);
            assert_eq!(runner.job, 0, "nothing was transcribed");
        }
        let mut phases = Vec::new();
        while let Ok(event) = ui_events.try_recv() {
            match event {
                UiEvent::StateChanged(phase) => phases.push(phase),
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(
            phases,
            [Phase::Recording, Phase::Idle, Phase::Recording, Phase::Idle]
        );
        // A shortcut outside a recording changes nothing.
        key(&mut runner, HotkeyEvent::ShortcutPressed, 2_000);
        assert_eq!(runner.state, State::Idle);
        // The next hold dictates.
        key(&mut runner, HotkeyEvent::Pressed, 3_000);
        key(&mut runner, HotkeyEvent::Released, 3_500);
        assert_eq!((runner.state, runner.job), (State::Transcribing, 1));
        runner.jobs.abort_all();
        while runner.jobs.join_next().await.is_some() {}
        assert!(inserter.inserted().is_empty());
    }

    /// M5.9 criterion 5 (#172): a transcript with no letter or digit (the
    /// provider's "." for a short, near-silent recording) is dropped like
    /// silence: back to Idle, nothing inserted (so the clipboard is never
    /// touched), no error shown.
    #[tokio::test]
    async fn criterion_5_a_transcript_with_no_words_is_never_inserted() {
        let inserter = FakeInserter::new();
        let (mut runner, _shutdown) = runner(&inserter);
        let (ui, mut ui_events) = ui_channel();
        runner.ui = ui;
        for (n, text) in [".", " ... ", "?!", "\u{3002}"].into_iter().enumerate() {
            let at = n as u64 * 1_000;
            key(&mut runner, HotkeyEvent::Pressed, at);
            key(&mut runner, HotkeyEvent::Released, at + 500);
            let job = runner.job;
            deliver(
                &mut runner,
                JobResult::Transcribed(job, Ok(Transcript::from_text(text)), Some('a')),
            );
            assert_eq!(runner.state, State::Idle, "{text:?}");
        }
        while let Ok(event) = ui_events.try_recv() {
            assert!(!matches!(event, UiEvent::Error(_)), "{event:?}");
        }
        // Only the (never-ending) transcriptions ran; no insert ever did.
        runner.jobs.abort_all();
        while runner.jobs.join_next().await.is_some() {}
        assert!(inserter.inserted().is_empty());
    }

    /// Every kind of job result, as a function of the job it claims to be.
    type MakeResult = fn(u64) -> JobResult;

    /// `(name, the result for job n, the event the current job's gives)`.
    fn every_result() -> Vec<(&'static str, MakeResult, Event)> {
        vec![
            (
                "transcribed",
                |n| JobResult::Transcribed(n, Ok(Transcript::from_text("hello")), None),
                Event::TranscriptReady("hello".into()),
            ),
            (
                "transcription failed",
                |n| JobResult::Transcribed(n, Err(TranscribeError::Timeout), None),
                Event::TranscriptionFailed(TranscribeError::Timeout.user_message()),
            ),
            (
                "not transcribed",
                JobResult::NotTranscribed,
                Event::TranscriptDropped,
            ),
            (
                "inserted",
                |n| JobResult::Inserted(n, Ok(())),
                Event::InsertDone,
            ),
            (
                "insert blocked by Secure Input",
                |n| JobResult::Inserted(n, Err(InsertError::SecureInput)),
                Event::SecureInputBlocked,
            ),
            (
                "insert failed",
                |n| JobResult::Inserted(n, Err(InsertError::Failed("no".into()))),
                Event::InsertFailed(InsertError::Failed("no".into()).to_string()),
            ),
            (
                "audio failed: permission",
                |n| JobResult::AudioFailed(n, AudioError::PermissionDenied),
                Event::PermissionMissing(Permission::Microphone),
            ),
            (
                "audio failed: device",
                |n| JobResult::AudioFailed(n, AudioError::NoDevice),
                Event::TranscriptionFailed(format!(
                    "the microphone failed ({})",
                    AudioError::NoDevice
                )),
            ),
        ]
    }

    /// A result for any job but the current one is dropped, for every kind
    /// of result; the current job's is handled. Each row fails if its guard
    /// is removed (P1-04).
    #[tokio::test]
    async fn all_job_result_variants_ignore_stale_ids() {
        let (mut runner, _shutdown) = runner(&FakeInserter::new());
        runner.job = 7;
        let id = tokio::spawn(async {}).id();
        for (name, make, current) in every_result() {
            // Older, newer and extreme ids are all "not this job".
            for stale in [0, 1, 6, 8, u64::MAX] {
                assert_eq!(
                    runner.job_event(Ok((id, make(stale)))),
                    None,
                    "{name}: job {stale} while the current job is 7"
                );
            }
            assert_eq!(
                runner.job_event(Ok((id, make(7)))),
                Some(current),
                "{name}: the current job"
            );
        }
    }

    /// The same through the whole step: stale results of every kind, while a
    /// dictation is in each state a result can end, change nothing the user
    /// sees: not the phase, no error.
    #[tokio::test]
    async fn stale_results_change_neither_the_phase_nor_the_errors() {
        let inserter = FakeInserter::new();
        let (mut runner, _shutdown) = runner(&inserter);
        let (ui, mut ui_events) = ui_channel();
        runner.ui = ui;

        // Job 1 is transcribing.
        key(&mut runner, HotkeyEvent::Pressed, 0);
        key(&mut runner, HotkeyEvent::Released, 500);
        assert_eq!((runner.state, runner.job), (State::Transcribing, 1));
        while ui_events.try_recv().is_ok() {}
        for (name, make, _) in every_result() {
            deliver(&mut runner, make(0));
            assert_eq!(runner.state, State::Transcribing, "{name}");
        }
        assert!(ui_events.try_recv().is_err(), "no event for a stale result");

        // Job 2 is inserting.
        deliver(
            &mut runner,
            JobResult::Transcribed(1, Ok("hello".into()), None),
        );
        assert_eq!((runner.state, runner.job), (State::Inserting, 2));
        while ui_events.try_recv().is_ok() {}
        for (name, make, _) in every_result() {
            deliver(&mut runner, make(1));
            assert_eq!(runner.state, State::Inserting, "{name}");
        }
        assert!(ui_events.try_recv().is_err(), "no event for a stale result");
        let inserted = next_insert(&mut runner).await;
        deliver(&mut runner, inserted);
        assert_eq!(runner.state, State::Idle);
        assert_eq!(inserter.inserted(), ["hello"]);
    }

    /// #84: a UI that stops reading for 300 failed dictations keeps the
    /// queue bounded; when it reads again it finds the current phase last
    /// and the first errors, with the rest counted; and it keeps working.
    #[tokio::test]
    async fn stalled_ui_consumer_keeps_bounded_state_and_errors() {
        use crate::ui_events::{UI_ERRORS_QUEUED, UI_EVENTS_QUEUED};

        let (mut runner, _shutdown) = runner(&FakeInserter::new());
        let (ui, mut ui_events) = ui_channel();
        runner.ui = ui;

        const DICTATIONS: u64 = 300;
        for n in 1..=DICTATIONS {
            key(&mut runner, HotkeyEvent::Pressed, n * 1_000);
            key(&mut runner, HotkeyEvent::Released, n * 1_000 + 500);
            assert_eq!((runner.state, runner.job), (State::Transcribing, n));
            deliver(
                &mut runner,
                JobResult::Transcribed(n, Err(TranscribeError::Timeout), None),
            );
            assert_eq!(runner.state, State::Idle);
            assert!(
                ui_events.queued() <= UI_EVENTS_QUEUED,
                "{} events after dictation {n}",
                ui_events.queued()
            );
        }
        assert_eq!(
            ui_events.dropped_errors(),
            DICTATIONS - UI_ERRORS_QUEUED as u64
        );

        // The UI reads again: errors first (the oldest), the phase it must
        // end on, Idle, last.
        let mut read = Vec::new();
        while let Ok(event) = ui_events.try_recv() {
            read.push(event);
        }
        assert!(read.len() <= UI_EVENTS_QUEUED);
        assert_eq!(read.last(), Some(&UiEvent::StateChanged(Idle)));
        let errors = read
            .iter()
            .filter(|event| matches!(event, UiEvent::Error(_)))
            .count();
        assert_eq!(errors, UI_ERRORS_QUEUED);

        // And it recovers: the next dictation arrives whole and in order.
        let n = DICTATIONS + 1;
        key(&mut runner, HotkeyEvent::Pressed, n * 1_000);
        key(&mut runner, HotkeyEvent::Released, n * 1_000 + 500);
        deliver(
            &mut runner,
            JobResult::Transcribed(n, Err(TranscribeError::Timeout), None),
        );
        let mut after = Vec::new();
        while let Ok(event) = ui_events.try_recv() {
            after.push(event);
        }
        assert_eq!(
            after,
            [
                UiEvent::StateChanged(Recording),
                UiEvent::StateChanged(Transcribing),
                UiEvent::Error(DictationError::TranscriptionFailed {
                    reason: TranscribeError::Timeout.user_message()
                }),
                UiEvent::StateChanged(Idle),
            ]
        );
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

    use tokio::sync::mpsc::error::TryRecvError;

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
        ui: UiReceiver,
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

    /// Says when the source is dropped (the device closed).
    struct DropReport<A>(A, std_mpsc::Sender<()>);

    impl<A: AudioSource> AudioSource for DropReport<A> {
        fn start(&mut self) -> Result<(), AudioError> {
            self.0.start()
        }

        fn stop(&mut self) -> Result<Audio, AudioError> {
            self.0.stop()
        }

        fn discard(&mut self) {
            self.0.discard();
        }

        fn is_recording(&self) -> bool {
            self.0.is_recording()
        }
    }

    impl<A> Drop for DropReport<A> {
        fn drop(&mut self) {
            let _ = self.1.send(());
        }
    }

    /// A [`ScriptedAudio`] that measures its level like the microphone
    /// does: the meter runs from `start` to `stop` or `discard`. The test
    /// feeds it samples, as the capture thread would.
    #[derive(Clone)]
    struct MeteredAudio(ScriptedAudio, LevelMeter);

    impl AudioSource for MeteredAudio {
        fn start(&mut self) -> Result<(), AudioError> {
            self.0.start()?;
            self.1.begin(Some("Test Microphone".into()));
            Ok(())
        }

        fn stop(&mut self) -> Result<Audio, AudioError> {
            self.1.end();
            self.0.stop()
        }

        fn discard(&mut self) {
            self.1.end();
            self.0.discard();
        }

        fn is_recording(&self) -> bool {
            self.1.is_recording()
        }

        fn level_meter(&self) -> Option<LevelMeter> {
            Some(self.1.clone())
        }
    }

    fn metered_harness() -> (Harness, MeteredAudio) {
        let audio = MeteredAudio(ScriptedAudio::new(Vec::new()), LevelMeter::new());
        let hotkey = FakeHotkey::new();
        let clock = FakeClock::new();
        let ports = Ports {
            hotkey: hotkey.clone(),
            audio: audio.clone(),
            transcriber: FixedTranscriber,
            inserter: FakeInserter::new(),
            permissions: FakePermissions::all_granted(),
            clock: clock.clone(),
        };
        let (pipeline, ui) = Pipeline::spawn(ports, Config::default()).unwrap();
        (
            Harness {
                hotkey,
                clock,
                pipeline,
                ui,
            },
            audio,
        )
    }

    /// 10 ms of a sine whose RMS is `dbfs`, at 16 kHz.
    fn sine_chunk(dbfs: f32) -> Vec<f32> {
        let amplitude = 10f32.powf(dbfs / 20.0) * std::f32::consts::SQRT_2;
        (0..160_u16)
            .map(|i| amplitude * (f32::from(i) * 440.0 * std::f32::consts::TAU / 16_000.0).sin())
            .collect()
    }

    /// U4 criterion 1: while recording the pipeline publishes the level
    /// 15 to 30 times a second, with the device's name, and nothing once
    /// the recording ends.
    #[tokio::test(flavor = "multi_thread")]
    async fn criterion_u4_1_levels_are_published_while_recording_only() {
        let (mut harness, audio) = metered_harness();
        let mut levels = harness.pipeline.levels();
        assert_eq!(*levels.borrow_and_update(), None);

        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        wait_for("the meter to start", || audio.1.is_recording()).await;

        // The capture thread: a -20 dBFS sine in 10 ms chunks.
        let feeding = Arc::new(AtomicBool::new(true));
        let feeder = {
            let meter = audio.1.clone();
            let feeding = Arc::clone(&feeding);
            std::thread::spawn(move || {
                while feeding.load(Ordering::Relaxed) {
                    meter.record(&sine_chunk(-20.0));
                    std::thread::sleep(Duration::from_millis(10));
                }
            })
        };

        // Skip the first reading (it may predate the feeder), then count
        // readings for a second.
        tokio::time::timeout(WAIT, levels.changed())
            .await
            .unwrap()
            .unwrap();
        levels.borrow_and_update();
        let started = Instant::now();
        let mut readings = Vec::new();
        while started.elapsed() < Duration::from_secs(1) {
            let left = Duration::from_secs(1).saturating_sub(started.elapsed());
            if let Ok(changed) = tokio::time::timeout(left, levels.changed()).await {
                changed.unwrap();
                readings.push(levels.borrow_and_update().clone());
            }
        }
        feeding.store(false, Ordering::Relaxed);
        feeder.join().unwrap();
        assert!(
            (15..=30).contains(&readings.len()),
            "{} readings in a second",
            readings.len()
        );
        for reading in &readings {
            let reading = reading.as_ref().expect("a level while recording");
            assert_eq!(reading.device.as_deref(), Some("Test Microphone"));
            assert!((reading.dbfs + 20.0).abs() <= 2.0, "{reading:?}");
        }

        // Released: the level goes away and stays away.
        harness.clock.advance(Duration::from_millis(1_500));
        harness.emit(HotkeyEvent::Released);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Transcribing));
        assert_eq!(*levels.borrow_and_update(), None);
        tokio::time::sleep(LEVEL_INTERVAL * 4).await;
        assert!(!levels.has_changed().unwrap(), "{:?}", *levels.borrow());
        assert!(!audio.1.is_recording());
        harness.pipeline.shutdown().await;
    }

    /// A recording with no audio arriving reads the floor (no sound), and a
    /// cancelled recording stops publishing too.
    #[tokio::test(flavor = "multi_thread")]
    async fn criterion_u4_1_no_audio_reads_the_floor_and_cancel_clears_it() {
        let (mut harness, audio) = metered_harness();
        let mut levels = harness.pipeline.levels();
        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        wait_for("the meter to start", || audio.1.is_recording()).await;
        tokio::time::timeout(WAIT, levels.changed())
            .await
            .unwrap()
            .unwrap();
        let reading = levels.borrow_and_update().clone().unwrap();
        assert_eq!(reading.dbfs, dictation_core::level::LEVEL_FLOOR_DBFS);

        harness.emit(HotkeyEvent::CancelPressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
        assert_eq!(*levels.borrow_and_update(), None);
        harness.pipeline.shutdown().await;
    }

    /// A source without a meter (file playback) publishes nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn criterion_u4_1_a_source_without_a_meter_publishes_nothing() {
        let mut harness = Harness::start(ScriptedAudio::new(Vec::new()));
        let levels = harness.pipeline.levels();
        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        tokio::time::sleep(LEVEL_INTERVAL * 4).await;
        assert!(!levels.has_changed().unwrap());
        assert_eq!(*levels.borrow(), None);
        harness.pipeline.shutdown().await;
    }

    /// A runner that is abandoned (its task is aborted, as a panic would
    /// end it) while a `start` is blocked still closes its audio worker:
    /// once the call returns the source is discarded and dropped.
    #[tokio::test(flavor = "multi_thread")]
    async fn abandoned_runner_closes_audio_worker() {
        let (release, gate) = std_mpsc::channel();
        let audio = ScriptedAudio::new(vec![OnStart::Block(gate)]);
        let (dropped_tx, dropped) = std_mpsc::channel();
        let (worker, failures) = AudioWorker::spawn(DropReport(audio.clone(), dropped_tx));
        let (ui, mut ui_events) = ui_channel();
        let (_shutdown, shutdown_rx) = watch::channel(false);
        let runner = Runner::new(
            FakeHotkey::new(),
            worker,
            FixedTranscriber,
            FakeInserter::new(),
            FakePermissions::all_granted(),
            Arc::new(FakeClock::new()),
            ui,
            shutdown_rx,
            tokio::time::interval(Duration::from_millis(250)),
        );
        let (keys_tx, keys_rx) = mpsc::channel(KEYS_QUEUED);
        let keys = Keys {
            rx: keys_rx,
            overflow: Arc::new(Overflow::default()),
        };
        let task = tokio::spawn(runner.run(keys, failures));

        keys_tx
            .send((HotkeyEvent::Pressed, Duration::ZERO))
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(WAIT, ui_events.recv()).await.unwrap(),
            Some(UiEvent::StateChanged(Recording))
        );
        audio.entered(1).await;

        // Abandon the runner with the call held, then let the call finish.
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(dropped.try_recv().is_err(), "the call is still running");
        release.send(()).unwrap();

        // Bounded: if nothing closes the worker this fails after WAIT
        // instead of hanging.
        let closed = tokio::task::spawn_blocking(move || dropped.recv_timeout(WAIT))
            .await
            .unwrap();
        assert!(closed.is_ok(), "the audio source was never dropped");
        assert_eq!(audio.calls(), ["start", "discard"]);
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
        // The pipeline runs now, on one thread: it throws away all of the
        // queued keys (#91), so none of them starts a recording.
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            harness.ui.try_recv(),
            Err(TryRecvError::Empty),
            "no recording from the queued keys"
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
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        // A current-thread runtime: the pipeline can't run during this.
        for _ in 0..(2 * KEYS_QUEUED) {
            // Repeats while recording: ignored, but they fill the queue.
            harness.emit(HotkeyEvent::Pressed);
        }
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
        harness.pipeline.shutdown().await;
        // Depending on when the worker ran, the Start was dropped by the
        // cancel or ran and was discarded; either way it ends discarded.
        let calls = audio.calls();
        assert_eq!(calls.last(), Some(&"discard"), "{calls:?}");
        assert!(!calls.contains(&"stop"), "{calls:?}");
    }

    /// M1.6 (#80) with M2.3 (#81): an `Interrupted` or `Failed` hotkey ends
    /// a recording like Esc does, so the `Start` queued behind a blocked one
    /// is dropped, never run.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_interrupted_or_failed_hotkey_drops_the_queued_start() {
        let (release, gate) = std_mpsc::channel();
        let audio = ScriptedAudio::new(vec![OnStart::Block(gate)]);
        let mut harness = Harness::start(audio.clone());

        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        audio.entered(1).await;
        harness.emit(HotkeyEvent::Interrupted);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));

        // The Start of each of these waits behind the blocked one.
        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        harness.emit(HotkeyEvent::Interrupted);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Idle));
        harness.emit(HotkeyEvent::Pressed);
        assert_eq!(harness.next().await, UiEvent::StateChanged(Recording));
        harness.emit(HotkeyEvent::Failed);
        let mut after_failed = [harness.next().await, harness.next().await];
        after_failed.sort_by_key(|event| matches!(event, UiEvent::StateChanged(_)));
        assert_eq!(
            after_failed,
            [
                UiEvent::Error(DictationError::HotkeyFailed),
                UiEvent::StateChanged(Idle)
            ]
        );

        release.send(()).unwrap();
        audio.finished(2).await;
        assert_eq!(audio.calls(), ["start", "discard"], "no queued start ran");
        assert_eq!(audio.entered.load(Ordering::SeqCst), 1);
        harness.pipeline.shutdown().await;
    }

    /// M1.6 with M2.3: a key dropped by the full queue calls for an Esc
    /// after the key that follows it. When that key is `Interrupted`, the
    /// recording is already gone: the Esc finds the coordinator idle and
    /// adds nothing, so the recording is discarded once and nothing else
    /// changes.
    #[tokio::test]
    async fn an_interrupted_key_after_a_dropped_one_discards_once() {
        let (ui, mut ui_events) = ui_channel();
        let (_shutdown, shutdown_rx) = watch::channel(false);
        let (release, gate) = std_mpsc::channel();
        let audio = ScriptedAudio::new(vec![OnStart::Block(gate)]);
        let (worker, _failed) = AudioWorker::spawn(audio.clone());
        let mut runner = Runner::new(
            FakeHotkey::new(),
            worker,
            FixedTranscriber,
            FakeInserter::new(),
            FakePermissions::all_granted(),
            Arc::new(FakeClock::new()),
            ui,
            shutdown_rx,
            tokio::time::interval(Duration::from_millis(250)),
        );
        let (_keys_tx, keys_rx) = mpsc::channel(KEYS_QUEUED);
        let mut keys = Keys {
            rx: keys_rx,
            overflow: Arc::new(Overflow::default()),
        };
        runner.key(HotkeyEvent::Pressed, Duration::ZERO).await;
        assert!(matches!(runner.state, State::Recording { .. }));
        audio.entered(1).await;

        // The interruption discards the recording; the overflow's cancel
        // then finds the coordinator idle.
        runner.key(HotkeyEvent::Interrupted, Duration::ZERO).await;
        assert_eq!(runner.state, State::Idle);
        keys.overflow.dropped.store(true, Ordering::SeqCst);
        assert_eq!(runner.keys_overflowed(&mut keys), Some(0));
        assert_eq!(runner.state, State::Idle);
        assert!(
            !keys.overflow.dropped.load(Ordering::SeqCst),
            "the flag is consumed"
        );

        release.send(()).unwrap();
        audio.finished(2).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(audio.calls(), ["start", "discard"], "one discard");
        assert_eq!(ui_events.try_recv(), Ok(UiEvent::StateChanged(Recording)));
        assert_eq!(ui_events.try_recv(), Ok(UiEvent::StateChanged(Idle)));
        assert!(ui_events.try_recv().is_err(), "nothing else");
    }

    /// #91: an overflow throws away every queued key and cancels the
    /// recording; a leftover wake-up with no dropped key does nothing.
    #[tokio::test]
    async fn keys_overflowed_discards_the_queue_and_cancels_only_when_a_key_was_dropped() {
        let (ui, mut ui_events) = ui_channel();
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
        let (keys_tx, keys_rx) = mpsc::channel(KEYS_QUEUED);
        let mut keys = Keys {
            rx: keys_rx,
            overflow: Arc::new(Overflow::default()),
        };
        runner.key(HotkeyEvent::Pressed, Duration::ZERO).await;
        assert!(matches!(runner.state, State::Recording { .. }));
        for _ in 0..5 {
            keys_tx
                .try_send((HotkeyEvent::Released, Duration::from_secs(1)))
                .unwrap();
        }

        // No key dropped: nothing is discarded and the recording goes on.
        assert_eq!(runner.keys_overflowed(&mut keys), None);
        assert!(matches!(runner.state, State::Recording { .. }));
        assert!(keys.rx.try_recv().is_ok(), "the queue was left alone");

        // A key was dropped: the other four go too, and the recording ends.
        keys.overflow.dropped.store(true, Ordering::SeqCst);
        assert_eq!(runner.keys_overflowed(&mut keys), Some(4));
        assert_eq!(runner.state, State::Idle);
        assert!(keys.rx.try_recv().is_err());
        assert_eq!(ui_events.try_recv(), Ok(UiEvent::StateChanged(Recording)));
        assert_eq!(ui_events.try_recv(), Ok(UiEvent::StateChanged(Idle)));
    }

    /// #81 M1: keys arriving nonstop don't hold off the recording limit:
    /// each key also feeds a Tick that has come due.
    #[tokio::test]
    async fn a_key_feeds_a_tick_that_came_due() {
        let (ui, _ui) = ui_channel();
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
        runner.key(HotkeyEvent::Pressed, Duration::ZERO).await;
        assert!(matches!(runner.state, State::Recording { .. }));

        clock.advance(dictation_core::coordinator::MAX_RECORDING);
        tokio::time::sleep(Duration::from_millis(5)).await;
        // A repeat, ignored by itself; the Tick due ends the recording.
        runner.key(HotkeyEvent::Pressed, clock.now()).await;
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
        // A discard right behind a discard adds nothing.
        queue.push(Discard);
        queue.push(Discard);
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
        let (ui, _ui) = ui_channel();
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
