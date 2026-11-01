//! [`MacMicrophone`]: the default input device on macOS, through
//! `handy-recorder` (MIT, from Handy).
//!
//! handy-recorder owns the real-time path: the CoreAudio callback (through
//! cpal) only copies samples into a lock-free single-producer ring buffer
//! (`rtrb`) and never allocates, locks or blocks; a delivery thread drains
//! the ring, mixes to mono and resamples to 16 kHz (`rubato`, a sinc
//! resampler) and hands fixed-size chunks to our [`Collector`] sink, which
//! may allocate since it isn't on the audio thread.
//!
//! On top of that this adapter decides when the device is open:
//!
//! - **Warm-up.** Opening a device takes a few milliseconds for the built-in
//!   mic and up to seconds for Bluetooth (AirPods switch to their headset
//!   profile). The first `start` opens the device; after a recording it
//!   stays open for [`WARM_FOR`] so the next dictation starts at once, then
//!   closes so the microphone indicator goes off. Handy keeps its stream
//!   open the same way.
//! - **Route changes.** An open recorder is tied to the device it opened.
//!   When that device goes away or the default input changes (AirPods
//!   connect or disconnect), handy-recorder fails the recorder; the next
//!   `start` opens the new default device. Mid-recording, `stop` returns
//!   the audio captured up to the change instead of failing, and nothing
//!   panics. Like Muesli, the adapter never waits on CoreAudio from the
//!   pipeline task (the pipeline calls it on the blocking pool).
//! - **Latency.** Each recording measures the time from `start` to the first
//!   audio chunk, logged at debug level as `first_audio_ms`.
//!
//! Never logs audio contents.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use dictation_core::wav::SAMPLE_RATE;
use dictation_ports::{Audio, AudioError, AudioSource};
use handy_recorder::{
    AudioChunk, Channels, EndReason, Error, ErrorKind, Permission, Recorder, RecorderConfig, Sink,
    StartError,
};

use super::lock;

/// How long the device stays open after a recording.
pub const WARM_FOR: Duration = Duration::from_secs(30);

/// Called by handy-recorder, on its own thread, when an open recorder fails.
pub(crate) type FailureHandler = Box<dyn FnOnce(Error) + Send + 'static>;
/// Opens a recorder; the real one opens the default device.
pub(crate) type Opener =
    dyn Fn(RecorderConfig, FailureHandler) -> Result<Recorder<Collector>, Error> + Send + Sync;
/// Reads the microphone permission without prompting.
pub(crate) type PermissionCheck = dyn Fn() -> Permission + Send + Sync;

/// The recorder config: the default device, 16 kHz mono, ~10 ms chunks.
fn config() -> RecorderConfig {
    RecorderConfig {
        device: None,
        sample_rate: Some(SAMPLE_RATE),
        channels: Channels::MixToMono,
        // ~10 ms: small chunks make the first-audio latency precise.
        frames_per_chunk: None,
        // Playing output to pull AirPods over from another device would be a
        // surprise; the user picks the input in System Settings.
        take_headset: false,
    }
}

/// The sink one recording's audio goes into: 16 kHz mono samples, and when
/// the first chunk arrived. Runs on handy-recorder's delivery thread, not the
/// real-time one, so it may allocate.
pub struct Collector {
    samples: Vec<f32>,
    started: Instant,
    first_audio: Option<Duration>,
    /// Frames collected so far, readable while recording (tests).
    frames: Arc<AtomicU64>,
    /// Leading frames still to drop: the resampler's delay.
    skip: usize,
}

impl Collector {
    fn new(started: Instant, frames: Arc<AtomicU64>) -> Self {
        Self {
            // Ten seconds up front; dictations are usually shorter.
            samples: Vec::with_capacity(SAMPLE_RATE as usize * 10),
            started,
            first_audio: None,
            frames,
            skip: 0,
        }
    }

    /// Drops the first `frames` frames: the delay of `open`'s resampler.
    fn skipping(mut self, open: &Open) -> Self {
        self.skip = open.delay;
        self
    }
}

impl Sink for Collector {
    fn process_chunk(&mut self, chunk: AudioChunk<'_>) {
        if self.first_audio.is_none() {
            self.first_audio = Some(self.started.elapsed());
        }
        // Mono: one sample per frame.
        let real = &chunk.samples[..chunk.valid_frames.min(chunk.samples.len())];
        let skipped = self.skip.min(real.len());
        self.skip -= skipped;
        let real = &real[skipped..];
        self.samples.extend_from_slice(real);
        self.frames.fetch_add(real.len() as u64, Ordering::Relaxed);
    }
}

/// handy-recorder's resampler chunk size, in input frames
/// (`capture/resampler.rs`, `RESAMPLER_CHUNK_SIZE`).
const RESAMPLER_CHUNK_FRAMES: usize = 1024;

/// Leading frames the resampler adds before the first real output frame,
/// at 16 kHz. handy-recorder delivers them (its `finish` keeps the tail, so
/// a clip is `input * 16 kHz / rate + delay` frames long) but doesn't expose
/// the figure, so this builds the same rubato resampler handy-recorder does
/// (`FrameResampler::new`, pinned versions) and asks it. The adapter tests
/// check the resulting clip length and alignment, so a change in either
/// crate fails them.
fn resampler_delay(device_rate: u32) -> usize {
    use rubato::{Fft, FixedSync, Resampler, WindowFunction};
    if device_rate == SAMPLE_RATE {
        return 0;
    }
    Fft::<f32>::new_custom(
        device_rate as usize,
        SAMPLE_RATE as usize,
        RESAMPLER_CHUNK_FRAMES,
        1,
        1,
        WindowFunction::BlackmanHarris2,
        FixedSync::Input,
    )
    .map_or(0, |resampler| resampler.output_delay())
}

/// An open recorder, whether it has failed since it opened, and its
/// resampler's delay.
struct Open {
    recorder: Recorder<Collector>,
    failed: Arc<AtomicBool>,
    /// [`resampler_delay`] for the device's rate.
    delay: usize,
}

impl Open {
    fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    fn close(self) {
        if let Err(error) = self.recorder.close() {
            tracing::warn!(%error, "closing the microphone failed");
        }
    }
}

/// State the warm-close timer shares with the microphone.
struct Shared {
    open: Mutex<Option<Open>>,
    /// Bumped by every `start`; a timer only closes if it hasn't changed.
    generation: AtomicU64,
}

/// The default input device on macOS. See the module docs for warm-up,
/// route changes and latency.
///
/// Errors: [`AudioError::PermissionDenied`] unless microphone access is
/// granted, [`AudioError::NoDevice`] without an input device, and
/// [`AudioError::Device`] for anything else the device does wrong.
pub struct MacMicrophone {
    open: Box<Opener>,
    permission: Box<PermissionCheck>,
    warm_for: Duration,
    shared: Arc<Shared>,
    /// The current recording's frame count, while recording.
    recording: Option<Arc<AtomicU64>>,
    last_first_audio: Option<Duration>,
}

impl Default for MacMicrophone {
    fn default() -> Self {
        Self::new()
    }
}

impl MacMicrophone {
    /// The default input device. Nothing opens until [`start`](AudioSource::start).
    #[must_use]
    pub fn new() -> Self {
        Self::with(
            Box::new(Recorder::open_with_failure_handler),
            Box::new(handy_recorder::permission_status),
            WARM_FOR,
        )
    }

    pub(crate) fn with(
        open: Box<Opener>,
        permission: Box<PermissionCheck>,
        warm_for: Duration,
    ) -> Self {
        Self {
            open,
            permission,
            warm_for,
            shared: Arc::new(Shared {
                open: Mutex::new(None),
                generation: AtomicU64::new(0),
            }),
            recording: None,
            last_first_audio: None,
        }
    }

    /// How long the last finished recording took from `start` to its first
    /// audio; `None` before any, or if no audio arrived.
    #[must_use]
    pub fn last_first_audio(&self) -> Option<Duration> {
        self.last_first_audio
    }

    /// The name of the open device; `None` while closed.
    #[must_use]
    pub fn device_name(&self) -> Option<String> {
        lock(&self.shared.open)
            .as_ref()
            .map(|open| open.recorder.info().device.name.clone())
    }

    /// Whether the device is open (recording, or warm).
    #[must_use]
    pub fn is_open(&self) -> bool {
        lock(&self.shared.open).is_some()
    }

    /// Frames captured so far in the current recording.
    #[cfg(test)]
    fn frames_so_far(&self) -> u64 {
        self.recording
            .as_ref()
            .map_or(0, |frames| frames.load(Ordering::Relaxed))
    }

    fn open_device(&self) -> Result<Open, AudioError> {
        let failed = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&failed);
        let handler: FailureHandler = Box::new(move |error| {
            tracing::warn!(%error, "the microphone failed");
            flag.store(true, Ordering::Release);
        });
        let opened_at = Instant::now();
        let recorder = (self.open)(config(), handler).map_err(|error| audio_error(&error))?;
        let info = recorder.info();
        let delay = resampler_delay(info.device_format.sample_rate);
        tracing::debug!(
            device = %info.device.name,
            device_rate = info.device_format.sample_rate,
            device_channels = info.device_format.channels,
            resampler_delay = delay,
            open_ms = opened_at.elapsed().as_millis(),
            "microphone opened"
        );
        Ok(Open {
            recorder,
            failed,
            delay,
        })
    }

    /// Closes the device after [`WARM_FOR`] unless another recording starts
    /// first.
    fn close_later(&self) {
        let generation = self.shared.generation.load(Ordering::Acquire);
        if self.warm_for.is_zero() {
            close_if_idle(&self.shared, generation);
            return;
        }
        let shared = Arc::downgrade(&self.shared);
        let warm_for = self.warm_for;
        let timer = thread::Builder::new()
            .name("mic-warm".into())
            .spawn(move || {
                thread::sleep(warm_for);
                close_if_idle_weak(&shared, generation);
            });
        if let Err(error) = timer {
            tracing::warn!(%error, "no warm-close timer; closing the microphone now");
            close_if_idle(&self.shared, generation);
        }
    }
}

fn close_if_idle_weak(shared: &Weak<Shared>, generation: u64) {
    if let Some(shared) = shared.upgrade() {
        close_if_idle(&shared, generation);
    }
}

/// Closes the device if no recording started since `generation`.
fn close_if_idle(shared: &Shared, generation: u64) {
    let open = {
        let mut open = lock(&shared.open);
        if shared.generation.load(Ordering::Acquire) != generation {
            return;
        }
        open.take()
    };
    if let Some(open) = open {
        open.close();
        tracing::debug!("microphone closed");
    }
}

impl AudioSource for MacMicrophone {
    fn start(&mut self) -> Result<(), AudioError> {
        if self.recording.is_some() {
            return Err(AudioError::AlreadyRecording);
        }
        // Without access CoreAudio records silence instead of failing, so
        // ask first. Unknown: let the device say.
        match (self.permission)() {
            Permission::Denied | Permission::NotDetermined => {
                return Err(AudioError::PermissionDenied);
            }
            _ => {}
        }
        let started = Instant::now();
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        let frames = Arc::new(AtomicU64::new(0));
        let sink = Collector::new(started, Arc::clone(&frames));

        let mut slot = lock(&self.shared.open);
        // A warm recorder whose device went away, or whose default device
        // changed, while idle: open the device that's there now.
        if slot.as_ref().is_some_and(Open::has_failed) {
            tracing::debug!("the microphone changed while idle; reopening");
            if let Some(stale) = slot.take() {
                stale.close();
            }
        }
        let open = match slot.take() {
            Some(open) => open,
            None => self.open_device()?,
        };
        let open = match open.recorder.start(sink.skipping(&open)) {
            Ok(()) => open,
            Err(StartError { error, sink }) if is_recorder_failure(error.kind()) => {
                // Failed after the check above: one fresh try.
                tracing::debug!(%error, "the warm microphone failed; reopening");
                open.close();
                let fresh = self.open_device()?;
                fresh
                    .recorder
                    .start(sink.skipping(&fresh))
                    .map_err(|start| audio_error(&start.error))?;
                fresh
            }
            Err(StartError { error, .. }) => {
                *slot = Some(open);
                return Err(audio_error(&error));
            }
        };
        *slot = Some(open);
        drop(slot);
        self.recording = Some(frames);
        Ok(())
    }

    fn stop(&mut self) -> Result<Audio, AudioError> {
        if self.recording.take().is_none() {
            return Err(AudioError::NotRecording);
        }
        let stopped = {
            let mut slot = lock(&self.shared.open);
            let Some(open) = slot.as_ref() else {
                return Err(AudioError::Device("the microphone closed".into()));
            };
            let stopped = open.recorder.stop();
            let failed = match &stopped {
                Ok(stopped) => matches!(stopped.end_reason, EndReason::RecorderFailed(_)),
                Err(_) => true,
            };
            if failed && let Some(open) = slot.take() {
                open.close();
            }
            stopped
        };
        self.close_later();
        let stopped = stopped.map_err(|error| audio_error(&error))?;

        if stopped.dropped_frames > 0 {
            tracing::warn!(
                dropped_frames = stopped.dropped_frames,
                "audio was lost while recording"
            );
        }
        let Collector {
            samples,
            first_audio,
            ..
        } = stopped.sink;
        self.last_first_audio = first_audio;
        match first_audio {
            Some(latency) => tracing::debug!(
                first_audio_ms = latency.as_secs_f64() * 1_000.0,
                "time from start to the first audio"
            ),
            None => tracing::debug!("no audio arrived"),
        }
        match stopped.end_reason {
            EndReason::StopCalled => {}
            EndReason::RecorderFailed(error) => {
                tracing::warn!(%error, "the microphone failed mid-recording");
                // Keep what was captured before the change.
                if samples.is_empty() {
                    return Err(audio_error(&error));
                }
            }
            EndReason::SinkPanicked(_) => {
                return Err(AudioError::Device("audio capture crashed".into()));
            }
        }
        Ok(Audio {
            sample_rate: SAMPLE_RATE,
            samples,
        })
    }

    fn discard(&mut self) {
        if self.recording.take().is_none() {
            return;
        }
        {
            let mut slot = lock(&self.shared.open);
            let failed = slot
                .as_ref()
                .is_none_or(|open| open.recorder.stop().is_err() || open.has_failed());
            if failed && let Some(open) = slot.take() {
                open.close();
            }
        }
        self.close_later();
    }

    fn is_recording(&self) -> bool {
        self.recording.is_some()
    }
}

/// Errors after which handy-recorder's recorder stays failed and a new one
/// must be opened.
fn is_recorder_failure(kind: ErrorKind) -> bool {
    !matches!(
        kind,
        ErrorKind::PermissionDenied
            | ErrorKind::AlreadyRecording
            | ErrorKind::NotRecording
            | ErrorKind::StopFromSink
    )
}

fn audio_error(error: &Error) -> AudioError {
    match error.kind() {
        ErrorKind::PermissionDenied => AudioError::PermissionDenied,
        ErrorKind::DeviceUnavailable => AudioError::NoDevice,
        ErrorKind::AlreadyRecording => AudioError::AlreadyRecording,
        ErrorKind::NotRecording => AudioError::NotRecording,
        _ => AudioError::Device(error.to_string()),
    }
}

#[cfg(test)]
mod tests;
