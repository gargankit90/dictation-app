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
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

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
    /// the maximum recording length.
    pub tick_interval: Duration,
}

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
    shutdown: mpsc::UnboundedSender<()>,
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

        // Stamp keys on the hotkey thread, when they happen, not when the
        // task gets to them.
        let (keys_tx, keys) = mpsc::unbounded_channel();
        let stamp = Arc::clone(&clock);
        hotkey.start(Box::new(move |key| {
            let _ = keys_tx.send((key, stamp.now()));
        }))?;

        let (ui, ui_events) = mpsc::unbounded_channel();
        let (shutdown, shutdown_rx) = mpsc::unbounded_channel();
        let (results_tx, results) = mpsc::unbounded_channel();
        let runner = Runner {
            state: State::Idle,
            hotkey,
            audio,
            transcriber: Arc::new(transcriber),
            inserter: Arc::new(inserter),
            permissions,
            clock,
            ui,
            results_tx,
            captured: None,
            job: 0,
            pending: VecDeque::new(),
        };
        let task = tokio::spawn(runner.run(config, keys, results, shutdown_rx));
        Ok((Self { shutdown, task }, ui_events))
    }

    /// Stops listening, drops any recording and waits for the task to end.
    /// In-flight transcriptions and inserts are abandoned.
    pub async fn shutdown(self) {
        let _ = self.shutdown.send(());
        let _ = self.task.await;
    }
}

/// A finished background job, tagged with the job it belongs to.
enum JobResult {
    Transcribed(u64, Result<String, TranscribeError>),
    Inserted(u64, Result<(), InsertError>),
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
    results_tx: mpsc::UnboundedSender<JobResult>,
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
    async fn run(
        mut self,
        config: Config,
        mut keys: mpsc::UnboundedReceiver<(HotkeyEvent, Duration)>,
        mut results: mpsc::UnboundedReceiver<JobResult>,
        mut shutdown: mpsc::UnboundedReceiver<()>,
    ) {
        let mut ticks = tokio::time::interval(config.tick_interval);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let recording = matches!(self.state, State::Recording { .. });
            let event = tokio::select! {
                _ = shutdown.recv() => break,
                Some((key, at)) = keys.recv() => self.key_event(key, at),
                Some(result) = results.recv() => self.job_event(result),
                _ = ticks.tick(), if recording => Some(Event::Tick(self.clock.now())),
            };
            if let Some(event) = event {
                self.feed(event);
            }
        }
        self.hotkey.stop();
        self.audio.discard();
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

    fn job_event(&self, result: JobResult) -> Option<Event> {
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
        let results = self.results_tx.clone();
        tokio::spawn(async move {
            let result = transcriber.transcribe(&audio).await;
            let _ = results.send(JobResult::Transcribed(job, result));
        });
    }

    fn insert(&mut self, text: String) {
        self.job += 1;
        let job = self.job;
        let inserter = Arc::clone(&self.inserter);
        let results = self.results_tx.clone();
        // Real inserters block (paste, wait, restore the clipboard).
        tokio::task::spawn_blocking(move || {
            let result = inserter.insert(&text);
            let _ = results.send(JobResult::Inserted(job, result));
        });
    }
}
