//! The adapter over handy-recorder's fake microphone, which runs the real
//! capture engine (ring buffer, mono mix, resampler) with a test thread
//! playing the device.

use std::sync::atomic::AtomicUsize;
use std::thread::JoinHandle;

use handy_recorder::testing::{FakeError, FakeMic};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

/// The ramp's value grows by `1 / RAMP_FRAMES` per input frame, so a
/// sample's value says which input frame it came from.
const RAMP_FRAMES: f32 = 10_000_000.0;

/// handy-recorder's id for every fake device.
const FAKE_ID: &str = "fake:mic";

struct Device {
    fake: FakeMic,
    opens: Arc<AtomicUsize>,
}

fn device(sample_rate: u32, channels: u16) -> Device {
    Device {
        fake: FakeMic::new(sample_rate, channels),
        opens: Arc::default(),
    }
}

impl Device {
    fn microphone(&self, permission: Permission, warm_for: Duration) -> MacMicrophone {
        let fake = self.fake.clone();
        let opens = Arc::clone(&self.opens);
        MacMicrophone::with(
            Box::new(move |config, handler| {
                opens.fetch_add(1, Ordering::SeqCst);
                let recorder = fake.open(config, Some(handler), WAIT)?;
                let device = recorder.info().device.id.clone();
                Ok(Opened { recorder, device })
            }),
            Box::new(move || permission),
            Box::new(|| Some(FAKE_ID.into())),
            warm_for,
        )
    }

    fn opens(&self) -> usize {
        self.opens.load(Ordering::SeqCst)
    }

    /// Plays the device: a ramp, in 10 ms blocks, about five times faster
    /// than real time, until the pump is dropped. `left_only` puts the ramp,
    /// doubled, on the first channel and silence on the others.
    fn pump(&self, sample_rate: u32, channels: u16, left_only: bool) -> Pump {
        let fake = self.fake.clone();
        let running = Arc::new(AtomicBool::new(true));
        let keep_going = Arc::clone(&running);
        let thread = thread::spawn(move || {
            let frames_per_block = sample_rate as usize / 100;
            let mut frame = 0u64;
            let mut block = vec![0.0f32; frames_per_block * usize::from(channels)];
            while keep_going.load(Ordering::Relaxed) {
                for samples in block.chunks_mut(usize::from(channels)) {
                    #[allow(clippy::cast_precision_loss, reason = "test ramp")]
                    let value = frame as f32 / RAMP_FRAMES;
                    for (channel, sample) in samples.iter_mut().enumerate() {
                        *sample = match (left_only, channel) {
                            (false, _) => value,
                            (true, 0) => value * f32::from(channels),
                            (true, _) => 0.0,
                        };
                    }
                    frame += 1;
                }
                // Only frames a running stream took count.
                if !fake.push(&block) {
                    frame -= frames_per_block as u64;
                }
                thread::sleep(Duration::from_millis(2));
            }
        });
        Pump {
            running,
            thread: Some(thread),
        }
    }
}

/// Plays `fake`, a 16 kHz mono device, at a constant `value` until dropped.
fn constant_pump(fake: FakeMic, value: f32) -> Pump {
    let running = Arc::new(AtomicBool::new(true));
    let keep_going = Arc::clone(&running);
    let thread = thread::spawn(move || {
        let block = [value; 160];
        while keep_going.load(Ordering::Relaxed) {
            fake.push(&block);
            thread::sleep(Duration::from_millis(2));
        }
    });
    Pump {
        running,
        thread: Some(thread),
    }
}

/// Two connected 16 kHz inputs, "A" playing +0.25 and "B" -0.25, and
/// which one is the default. Opening opens the default, as CoreAudio does.
struct Route {
    a: FakeMic,
    b: FakeMic,
    /// The default's id; `None`: it can't be read.
    default: Arc<Mutex<Option<&'static str>>>,
    /// The devices opened, in order.
    opened: Arc<Mutex<Vec<&'static str>>>,
    _pumps: [Pump; 2],
}

impl Route {
    fn new() -> Self {
        let (a, b) = (FakeMic::new(16_000, 1), FakeMic::new(16_000, 1));
        Self {
            _pumps: [
                constant_pump(a.clone(), 0.25),
                constant_pump(b.clone(), -0.25),
            ],
            a,
            b,
            default: Arc::new(Mutex::new(Some("A"))),
            opened: Arc::default(),
        }
    }

    fn microphone(&self) -> MacMicrophone {
        let (a, b) = (self.a.clone(), self.b.clone());
        let default = Arc::clone(&self.default);
        let opened = Arc::clone(&self.opened);
        let current = Arc::clone(&self.default);
        MacMicrophone::with(
            Box::new(move |config, handler| {
                let device = lock(&default).unwrap_or("A");
                lock(&opened).push(device);
                let fake = if device == "A" { &a } else { &b };
                let recorder = fake.open(config, Some(handler), WAIT)?;
                Ok(Opened {
                    recorder,
                    device: device.into(),
                })
            }),
            Box::new(|| Permission::Granted),
            Box::new(move || lock(&current).map(Into::into)),
            WAIT * 10,
        )
    }

    fn set_default(&self, device: Option<&'static str>) {
        *lock(&self.default) = device;
    }

    fn opened(&self) -> Vec<&'static str> {
        lock(&self.opened).clone()
    }
}

/// Which device a clip came from, by its samples.
fn source_of(audio: &Audio) -> &'static str {
    assert!(!audio.samples.is_empty());
    if audio.samples.iter().all(|&s| (s - 0.25).abs() < 1e-6) {
        "A"
    } else if audio.samples.iter().all(|&s| (s + 0.25).abs() < 1e-6) {
        "B"
    } else {
        "mixed"
    }
}

struct Pump {
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Pump {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(1));
    }
}

/// Records until at least `frames` 16 kHz frames are in, then stops.
fn record(mic: &mut MacMicrophone, frames: u64) -> Audio {
    mic.start().unwrap();
    wait_until("audio", || mic.frames_so_far() >= frames);
    mic.stop().unwrap()
}

/// Input frames per output sample, from the ramp's slope away from the
/// edges (where the resampler's filter is still filling or emptying).
fn input_frames_per_output_sample(samples: &[f32]) -> f64 {
    let edge = 400;
    assert!(samples.len() > 4 * edge, "{} samples", samples.len());
    let first = f64::from(samples[edge]);
    let last = f64::from(samples[samples.len() - 1 - edge]);
    #[allow(clippy::cast_precision_loss, reason = "a few thousand samples")]
    let span = (samples.len() - 1 - 2 * edge) as f64;
    (last - first) * f64::from(RAMP_FRAMES) / span
}

/// Records exactly `input_frames` frames of the ramp, starting at frame 0
/// on every channel, in 10 ms blocks, then stops.
fn record_exact(
    device: &Device,
    mic: &mut MacMicrophone,
    sample_rate: u32,
    channels: u16,
    input_frames: usize,
) -> Audio {
    mic.start().unwrap();
    let block = sample_rate as usize / 100;
    let mut frame = 0;
    while frame < input_frames {
        let frames = block.min(input_frames - frame);
        #[allow(clippy::cast_precision_loss, reason = "test ramp")]
        let samples: Vec<f32> = (frame..frame + frames)
            .flat_map(|f| std::iter::repeat_n(f as f32 / RAMP_FRAMES, usize::from(channels)))
            .collect();
        assert!(device.fake.push(&samples), "the stream took the block");
        frame += frames;
    }
    // The clips are under the 2 s ring, so nothing is dropped without
    // pacing, and the stop drains them all.
    stop_acknowledged(&device.fake, mic).unwrap()
}

/// M2.3 criterion 4 (#77 M2): a clip is as long as its input, counting the
/// valid frames the collector kept, and starts with the first input frame.
#[test]
fn m2_3_criterion_4_the_clip_is_as_long_as_the_input() {
    for (sample_rate, channels) in [(48_000, 1), (44_100, 2)] {
        for millis in [200, 1_000] {
            let device = device(sample_rate, channels);
            let mut mic = device.microphone(Permission::Granted, Duration::ZERO);
            let input_frames = sample_rate as usize * millis / 1_000;

            let audio = record_exact(&device, &mut mic, sample_rate, channels, input_frames);

            let case = format!("{sample_rate} Hz x{channels}, {millis} ms");
            assert_eq!(audio.sample_rate, 16_000, "{case}");
            let expected = input_frames * 16_000 / sample_rate as usize;
            let actual = audio.samples.len();
            assert!(
                actual.abs_diff(expected) * 100 <= expected,
                "{case}: {actual} frames, expected {expected} within 1%"
            );
            // Aligned: output frame k holds input frame k * rate / 16 kHz,
            // so the leading delay is gone, not just shortened.
            for k in [expected / 4, expected / 2, expected * 3 / 4] {
                let input_frame = f64::from(audio.samples[k]) * f64::from(RAMP_FRAMES);
                #[allow(clippy::cast_precision_loss, reason = "a few thousand frames")]
                let want = k as f64 * f64::from(sample_rate) / 16_000.0;
                assert!(
                    (input_frame - want).abs() < 2.0,
                    "{case}: output frame {k} holds input frame {input_frame}, expected {want}"
                );
            }
        }
    }
}

/// Stops `mic` while a stand-in audio thread acknowledges the recorder's
/// pause with empty blocks, as a running device's next callback would, so
/// the stop ends cleanly (`StopCalled`, not a stall) and adds no audio.
/// Every frame pushed before it is the recording's: handy-recorder drains
/// its ring and flushes the resampler's tail before `stop` returns, so the
/// stop is the consumption fence. Bounded: the stand-in gives up after
/// [`WAIT`] (the stop then stalls and fails, and the caller sees it).
fn stop_acknowledged(fake: &FakeMic, mic: &mut MacMicrophone) -> Result<Audio, AudioError> {
    let done = AtomicBool::new(false);
    thread::scope(|scope| {
        let acknowledger = scope.spawn(|| {
            let deadline = Instant::now() + WAIT;
            while !done.load(Ordering::Acquire) {
                if Instant::now() >= deadline {
                    return false;
                }
                fake.push(&[]);
                thread::yield_now();
            }
            true
        });
        let audio = mic.stop();
        done.store(true, Ordering::Release);
        assert!(
            acknowledger.join().unwrap(),
            "the stop took longer than {WAIT:?}"
        );
        audio
    })
}

/// The rate tests' marker: input frame `f` of an `input`-frame clip is
/// `min(f, input - 1 - f) / MARK`, a tent that starts and ends at 0 and
/// steps by `1 / MARK` per frame. A sample's value says which input frame
/// it came from, on either slope, and with no step at either end the
/// resampler doesn't ring there, so the very first and last samples can be
/// checked too.
const MARK: f32 = 10_000_000.0;

fn tent(f: usize, input: usize) -> f32 {
    #[allow(clippy::cast_precision_loss, reason = "a few hundred thousand frames")]
    let value = f.min(input - 1 - f) as f32 / MARK;
    value
}

/// Input frame `f` as the device delivers it: mono, or in stereo `3x` on
/// the left and `-x` on the right, so only their average (not either
/// channel, nor their sum) gives the tent.
fn device_frame(f: usize, input: usize, channels: u16) -> Vec<f32> {
    let x = tent(f, input);
    match channels {
        1 => vec![x],
        2 => vec![3.0 * x, -x],
        _ => unreachable!("mono or stereo"),
    }
}

/// How far a 16 kHz sample may sit from where its input frame belongs, in
/// 16 kHz frames. Measured: at most 0.55 (96 kHz, whose resampler delay
/// rounds by half a frame); a lost or extra frame at either end is 1.
const ALIGNMENT_TOLERANCE: f64 = 0.75;

/// #125: through the real adapter and capture engine (ring, mono mix,
/// resampler, the adapter's delay skip), a finite clip at each supported
/// rate comes out exactly `floor(input * 16 kHz / rate)` samples long (no
/// tolerance on the count), and every sample, the first and the last
/// included, holds the input frame it should: nothing lost at the start
/// or the tail, no padding, nothing doubled. Clips: empty, one frame, less
/// than one resampler chunk (1024 frames), a 30 ms word plus an odd tail,
/// and a second plus an odd tail, all pushed in 10 ms device blocks with a
/// final partial block. The longest is under the 2 s ring, so no frame can
/// be dropped however slowly the delivery thread runs.
#[test]
fn exact_clip_length_and_alignment_across_supported_rates() {
    for rate in [8_000, 16_000, 22_050, 24_000, 44_100, 48_000, 96_000] {
        let frames_per_block = rate as usize / 100;
        let inputs = [0, 1, 1_000, frames_per_block * 3 + 7, rate as usize + 333];
        for channels in [1, 2] {
            let device = device(rate, channels);
            // Warm, so the device staying open shows each stop was clean.
            let mut mic = device.microphone(Permission::Granted, WAIT * 10);
            for input in inputs {
                let case = format!("{rate} Hz x{channels}, {input} frames");
                mic.start().unwrap();
                let mut frame = 0;
                while frame < input {
                    let end = (frame + frames_per_block).min(input);
                    let block: Vec<f32> = (frame..end)
                        .flat_map(|f| device_frame(f, input, channels))
                        .collect();
                    assert!(device.fake.push(&block), "{case}: the stream took it");
                    frame = end;
                }
                let audio = stop_acknowledged(&device.fake, &mut mic).unwrap();
                assert!(mic.is_open(), "{case}: the stop stalled or failed");

                assert_eq!(audio.sample_rate, 16_000, "{case}");
                let expected = input * 16_000 / rate as usize;
                assert_eq!(audio.samples.len(), expected, "{case}: length");
                let ratio = f64::from(rate) / 16_000.0;
                #[allow(clippy::cast_precision_loss, reason = "a few hundred thousand frames")]
                let last_input = input.saturating_sub(1) as f64;
                for (k, &sample) in audio.samples.iter().enumerate() {
                    #[allow(clippy::cast_precision_loss, reason = "under 20 000 samples")]
                    let at = k as f64 * ratio;
                    // Around the peak the two slopes meet and the value is
                    // ambiguous; everywhere else it says the frame.
                    if (at - last_input / 2.0).abs() < 8.0 * ratio {
                        continue;
                    }
                    // Past the last input frame (upsampling) the input is
                    // silence, as it is at the tent's foot.
                    let want = at.min(last_input - at).max(0.0);
                    let off = (f64::from(sample) * f64::from(MARK) - want).abs() / ratio;
                    assert!(
                        off <= ALIGNMENT_TOLERANCE,
                        "{case}: sample {k} of {expected} is {off:.2} frames from input frame {at:.1}"
                    );
                }
            }
        }
    }
}

#[test]
fn m2_2_criterion_2_44_1_khz_stereo_is_resampled_and_mixed_to_mono() {
    let device = device(44_100, 2);
    let mut mic = device.microphone(Permission::Granted, Duration::ZERO);
    // The ramp on the left channel only, doubled: the mono mix is the ramp.
    let _pump = device.pump(44_100, 2, true);

    let audio = record(&mut mic, 16_000);

    assert_eq!(audio.sample_rate, 16_000);
    let ratio = input_frames_per_output_sample(&audio.samples) / (44_100.0 / 16_000.0);
    assert!((ratio - 1.0).abs() < 0.01, "rate ratio {ratio}");
}

#[test]
fn a_16_khz_device_needs_no_resampling_and_skips_nothing() {
    assert_eq!(resampler_delay(16_000), 0);
    assert!(resampler_delay(48_000) > 0);
    let device = device(16_000, 1);
    let mut mic = device.microphone(Permission::Granted, Duration::ZERO);
    let audio = record_exact(&device, &mut mic, 16_000, 1, 3_200);
    assert_eq!(audio.samples.len(), 3_200);
    assert!(audio.samples[0].abs() < f32::EPSILON);
}

#[test]
fn the_time_to_first_audio_is_measured() {
    let device = device(16_000, 1);
    let mut mic = device.microphone(Permission::Granted, Duration::ZERO);
    assert_eq!(mic.last_first_audio(), None);
    let _pump = device.pump(16_000, 1, false);
    record(&mut mic, 160);
    let latency = mic.last_first_audio().unwrap();
    assert!(latency < WAIT, "{latency:?}");
}

#[test]
fn criterion_4_a_device_lost_mid_recording_keeps_the_audio_so_far() {
    let device = device(48_000, 1);
    let mut mic = device.microphone(Permission::Granted, Duration::ZERO);
    let pump = device.pump(48_000, 1, false);

    mic.start().unwrap();
    wait_until("audio", || mic.frames_so_far() >= 1_600);
    // AirPods disconnect, or the default input changes.
    assert!(device.fake.report_error(FakeError::DeviceNotAvailable));
    let audio = mic.stop().unwrap();
    assert!(audio.samples.len() >= 1_600);
    assert!(!mic.is_open(), "a failed recorder isn't kept");

    // The next recording opens the device again.
    let audio = record(&mut mic, 1_600);
    assert!(audio.samples.len() >= 1_600);
    assert_eq!(device.opens(), 2);
    drop(pump);
}

#[test]
fn criterion_4_a_device_lost_before_any_audio_is_a_device_error() {
    let device = device(48_000, 1);
    let mut mic = device.microphone(Permission::Granted, Duration::ZERO);
    mic.start().unwrap();
    assert!(device.fake.report_error(FakeError::DeviceNotAvailable));
    wait_until("the failure", || !device.fake.is_streaming());
    assert!(matches!(mic.stop(), Err(AudioError::Device(_))));
    assert!(!mic.is_recording());
}

#[test]
fn criterion_4_a_device_change_while_warm_reopens_at_the_next_start() {
    let device = device(48_000, 1);
    let mut mic = device.microphone(Permission::Granted, WAIT * 10);
    let _pump = device.pump(48_000, 1, false);

    record(&mut mic, 1_600);
    assert!(mic.is_open(), "warm");
    assert!(device.fake.report_error(FakeError::DeviceNotAvailable));
    wait_until("the failure", || !device.fake.is_streaming());

    let audio = record(&mut mic, 1_600);
    assert!(audio.samples.len() >= 1_600);
    assert_eq!(device.opens(), 2);
}

/// M2.3 criterion 1 (#77 B1): the default input moves to another device
/// while the old one stays connected and its warm recorder hasn't failed.
#[test]
fn m2_3_criterion_1_a_default_only_switch_while_warm_reopens_on_the_new_default() {
    let route = Route::new();
    let mut mic = route.microphone();
    assert_eq!(source_of(&record(&mut mic, 1_600)), "A");
    assert!(mic.is_open(), "warm");

    route.set_default(Some("B"));
    assert!(route.a.is_streaming(), "A is still alive and open");
    assert_eq!(source_of(&record(&mut mic, 1_600)), "B");
    assert_eq!(route.opened(), ["A", "B"]);
    wait_until("A to close", || !route.a.is_streaming());
    // And B stays warm for the next one.
    assert_eq!(source_of(&record(&mut mic, 1_600)), "B");
    assert_eq!(route.opened(), ["A", "B"]);
}

/// The policy for a default change mid-recording: the recording stays on
/// its device; the next one switches.
#[test]
fn a_default_switch_mid_recording_keeps_the_recording_on_its_device() {
    let route = Route::new();
    let mut mic = route.microphone();
    mic.start().unwrap();
    wait_until("audio", || mic.frames_so_far() >= 800);
    route.set_default(Some("B"));
    let before = mic.frames_so_far();
    wait_until("more audio", || mic.frames_so_far() >= before + 800);
    assert_eq!(source_of(&mic.stop().unwrap()), "A");

    assert_eq!(source_of(&record(&mut mic, 1_600)), "B");
    assert_eq!(route.opened(), ["A", "B"]);
}

#[test]
fn an_unreadable_default_keeps_the_warm_device() {
    let route = Route::new();
    let mut mic = route.microphone();
    record(&mut mic, 160);
    route.set_default(None);
    assert_eq!(source_of(&record(&mut mic, 160)), "A");
    assert_eq!(route.opened(), ["A"]);
}

#[test]
fn the_current_default_input_reads_without_opening() {
    // Whatever this machine has; on a Mac with an input it's an id in
    // handy-recorder's form.
    if let Some(id) = current_default_input() {
        assert!(id.contains(':'), "{id}");
    }
}

#[test]
fn a_warm_device_is_reused_by_the_next_recording() {
    let device = device(16_000, 1);
    let mut mic = device.microphone(Permission::Granted, WAIT * 10);
    let _pump = device.pump(16_000, 1, false);
    record(&mut mic, 160);
    record(&mut mic, 160);
    assert_eq!(device.opens(), 1);
    assert!(mic.is_open());
    assert!(mic.device_name().is_some());
}

#[test]
fn without_warm_time_the_device_closes_after_each_recording() {
    let device = device(16_000, 1);
    let mut mic = device.microphone(Permission::Granted, Duration::ZERO);
    let _pump = device.pump(16_000, 1, false);
    record(&mut mic, 160);
    assert!(!mic.is_open());
    assert_eq!(mic.device_name(), None);
    record(&mut mic, 160);
    assert_eq!(device.opens(), 2);
}

#[test]
fn the_warm_device_closes_once_idle_long_enough() {
    let device = device(16_000, 1);
    let mut mic = device.microphone(Permission::Granted, Duration::from_millis(20));
    let _pump = device.pump(16_000, 1, false);
    record(&mut mic, 160);
    wait_until("the warm close", || !mic.is_open());
    // The timer closes the stream just after taking it out.
    wait_until("the stream to stop", || !device.fake.is_streaming());
}

#[test]
fn a_recording_started_while_warm_is_not_closed_by_the_old_timer() {
    let device = device(16_000, 1);
    let mut mic = device.microphone(Permission::Granted, Duration::from_millis(20));
    let _pump = device.pump(16_000, 1, false);
    record(&mut mic, 160);
    mic.start().unwrap();
    thread::sleep(Duration::from_millis(60));
    assert!(mic.is_open(), "closed mid-recording");
    wait_until("audio", || mic.frames_so_far() >= 160);
    mic.stop().unwrap();
    assert_eq!(device.opens(), 1);
}

#[test]
fn criterion_6_without_permission_nothing_opens() {
    for permission in [Permission::Denied, Permission::NotDetermined] {
        let device = device(16_000, 1);
        let mut mic = device.microphone(permission, Duration::ZERO);
        assert_eq!(mic.start(), Err(AudioError::PermissionDenied));
        assert!(!mic.is_recording());
        assert_eq!(device.opens(), 0);
    }
}

#[test]
fn an_unknown_permission_lets_the_device_decide() {
    let device = device(16_000, 1);
    let mut mic = device.microphone(Permission::Unknown, Duration::ZERO);
    let _pump = device.pump(16_000, 1, false);
    record(&mut mic, 160);
}

#[test]
fn open_failures_map_to_audio_errors() {
    let cases = [
        (FakeError::PermissionDenied, AudioError::PermissionDenied),
        (FakeError::DeviceNotAvailable, AudioError::NoDevice),
    ];
    for (fake_error, expected) in cases {
        let device = device(16_000, 1);
        let mut mic = device.microphone(Permission::Granted, Duration::ZERO);
        device.fake.fail_next_open(fake_error);
        assert_eq!(mic.start(), Err(expected));
        assert!(!mic.is_recording());
        assert!(!mic.is_open());
    }
    let device = device(16_000, 1);
    let mut mic = device.microphone(Permission::Granted, Duration::ZERO);
    device.fake.fail_next_open(FakeError::DeviceBusy);
    assert!(matches!(mic.start(), Err(AudioError::Device(_))));
}

#[test]
fn misuse_is_reported_not_panicked() {
    let device = device(16_000, 1);
    let mut mic = device.microphone(Permission::Granted, Duration::ZERO);
    let _pump = device.pump(16_000, 1, false);
    assert_eq!(mic.stop(), Err(AudioError::NotRecording));
    mic.discard();

    mic.start().unwrap();
    assert!(mic.is_recording());
    assert_eq!(mic.start(), Err(AudioError::AlreadyRecording));
    mic.discard();
    assert!(!mic.is_recording());
    assert_eq!(mic.stop(), Err(AudioError::NotRecording));
    // The device still works after a discard.
    record(&mut mic, 160);
}

#[test]
fn errors_map_by_kind() {
    assert!(!is_recorder_failure(ErrorKind::PermissionDenied));
    assert!(!is_recorder_failure(ErrorKind::NotRecording));
    assert!(is_recorder_failure(ErrorKind::DeviceLost));
    assert!(is_recorder_failure(ErrorKind::StreamInvalidated));
    assert!(is_recorder_failure(ErrorKind::NoAudio));
}

#[test]
fn the_real_microphone_opens_nothing_until_started() {
    let mic = MacMicrophone::new();
    assert!(!mic.is_open());
    assert!(!mic.is_recording());
}
