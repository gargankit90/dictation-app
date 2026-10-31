//! M2.2 Tester (run 1): resampling to 16 kHz mono from input rates the
//! adapter's own tests don't cover (8 kHz, 22.05 kHz, 24 kHz Bluetooth
//! headset profile, 96 kHz). Runs handy-recorder's real engine over its fake
//! device with the adapter's settings (`sample_rate: 16 kHz`, `MixToMono`,
//! default chunks). macOS only, like the adapter.

#![cfg(target_os = "macos")]
#![allow(
    clippy::unwrap_used,
    clippy::cast_precision_loss,
    reason = "test code: failing is the point, ramps are small"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use handy_recorder::testing::FakeMic;
use handy_recorder::{Channels, CollectingSink, Recorder, RecorderConfig};

const OPEN_TIMEOUT: Duration = Duration::from_secs(5);
/// A sample's value says which input frame it came from.
const RAMP_FRAMES: f32 = 10_000_000.0;

fn adapter_config() -> RecorderConfig {
    RecorderConfig {
        device: None,
        sample_rate: Some(16_000),
        channels: Channels::MixToMono,
        frames_per_chunk: None,
        take_headset: false,
    }
}

/// Records at least `seconds` of a ramp played by a fake device at `rate`
/// Hz with `channels` identical channels and returns the 16 kHz output. The
/// device keeps playing while the recorder stops, as a real one does (the
/// engine flags a stall otherwise).
fn record(rate: u32, channels: u16, seconds: f32) -> Vec<f32> {
    let fake = FakeMic::new(rate, channels);
    let recorder: Recorder<CollectingSink> =
        fake.open(adapter_config(), None, OPEN_TIMEOUT).unwrap();
    recorder.start(CollectingSink::new()).unwrap();

    let running = Arc::new(AtomicBool::new(true));
    let pushed = Arc::new(AtomicU64::new(0));
    let pump = {
        let fake = fake.clone();
        let running = Arc::clone(&running);
        let pushed = Arc::clone(&pushed);
        thread::spawn(move || {
            let frames_per_block = (rate / 100) as usize;
            let mut frame = 0u64;
            let mut block = vec![0.0f32; frames_per_block * usize::from(channels)];
            while running.load(Ordering::Relaxed) {
                for samples in block.chunks_mut(usize::from(channels)) {
                    samples.fill(frame as f32 / RAMP_FRAMES);
                    frame += 1;
                }
                if fake.push(&block) {
                    pushed.store(frame, Ordering::Relaxed);
                } else {
                    frame -= frames_per_block as u64;
                }
                thread::sleep(Duration::from_millis(2));
            }
        })
    };
    let target = (rate as f32 * seconds) as u64;
    let deadline = Instant::now() + Duration::from_secs(10);
    while pushed.load(Ordering::Relaxed) < target {
        assert!(Instant::now() < deadline, "the device never played enough");
        thread::sleep(Duration::from_millis(5));
    }
    let stopped = recorder.stop().unwrap();
    running.store(false, Ordering::Relaxed);
    pump.join().unwrap();
    assert!(
        stopped.is_complete(),
        "end {:?}, dropped {}",
        stopped.end_reason,
        stopped.dropped_frames
    );
    stopped.sink.into_samples()
}

/// Input frames per output sample, from the ramp's slope away from the
/// edges, where the resampler's filter fills and empties.
fn input_frames_per_output_sample(samples: &[f32]) -> f64 {
    // 100 ms: wider than the sinc filter even at 8 kHz, where its taps span
    // twice as many output samples as at 16 kHz.
    let edge = 1_600;
    assert!(samples.len() > 4 * edge, "{} samples", samples.len());
    let first = f64::from(samples[edge]);
    let last = f64::from(samples[samples.len() - 1 - edge]);
    let span = (samples.len() - 1 - 2 * edge) as f64;
    (last - first) * f64::from(RAMP_FRAMES) / span
}

fn assert_keeps_duration(rate: u32, channels: u16) {
    let seconds = 1.5;
    let out = record(rate, channels, seconds);
    let ratio = input_frames_per_output_sample(&out) / (f64::from(rate) / 16_000.0);
    assert!(
        (ratio - 1.0).abs() < 0.01,
        "{rate} Hz x{channels}: duration ratio {ratio}"
    );
    // At least the duration played, at 16 kHz (the device keeps playing
    // until stop returns, so a little more is fine).
    let expected = f64::from(16_000.0 * seconds);
    let len = out.len() as f64;
    assert!(
        len >= expected * 0.99,
        "{rate} Hz x{channels}: {len} samples for {expected}"
    );
}

#[test]
fn eight_khz_is_upsampled_keeping_the_duration() {
    assert_keeps_duration(8_000, 1);
}

#[test]
fn sixteen_khz_passes_through_keeping_the_duration() {
    assert_keeps_duration(16_000, 1);
}

#[test]
fn twenty_two_05_khz_keeps_the_duration() {
    assert_keeps_duration(22_050, 1);
}

#[test]
fn twenty_four_khz_bluetooth_headset_rate_keeps_the_duration() {
    assert_keeps_duration(24_000, 1);
}

#[test]
fn ninety_six_khz_stereo_keeps_the_duration() {
    assert_keeps_duration(96_000, 2);
}

#[test]
fn a_recording_with_no_audio_is_empty_not_an_error() {
    let fake = FakeMic::new(48_000, 1);
    let recorder: Recorder<CollectingSink> =
        fake.open(adapter_config(), None, OPEN_TIMEOUT).unwrap();
    recorder.start(CollectingSink::new()).unwrap();
    let stopped = recorder.stop().unwrap();
    assert!(stopped.sink.into_samples().is_empty());
}
