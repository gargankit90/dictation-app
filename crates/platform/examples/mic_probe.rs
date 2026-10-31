//! Manual check for M2.2: records from the default microphone and prints the
//! time from `start` to the first audio (criterion 5), cold and then warm.
//!
//! ```sh
//! cargo run -p dictation-platform --example mic_probe                  # 3 x 2 s
//! cargo run -p dictation-platform --example mic_probe -- 5 1           # 1 x 5 s
//! cargo run -p dictation-platform --example mic_probe -- 2 1 out.wav   # and save it
//! ```
//!
//! Microphone access is granted to the app the probe runs in (Terminal,
//! iTerm, …). Prints only timings, length and loudness, never the audio;
//! nothing is saved unless a path is given.

#[cfg(target_os = "macos")]
fn main() -> std::process::ExitCode {
    use std::process::ExitCode;
    use std::time::{Duration, Instant};

    use dictation_core::{audio_math, wav};
    use dictation_platform::audio::MacMicrophone;
    use dictation_platform::permissions::macos::MacPermissions;
    use dictation_ports::{AudioSource, Permission, PermissionStatus, Permissions};

    let mut args = std::env::args().skip(1);
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(2);
    let count: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(3);
    let save = args.next();

    let permissions = MacPermissions::new();
    let status = permissions.check(Permission::Microphone);
    println!("microphone permission: {status:?}");
    if status != PermissionStatus::Granted {
        if let Err(e) = permissions.request(Permission::Microphone) {
            println!("could not show the prompt: {e}");
        }
        println!(
            "Grant it to this terminal (System Settings > Privacy & Security > Microphone), then run again"
        );
        return ExitCode::FAILURE;
    }

    let mut mic = MacMicrophone::new();
    for take in 1..=count {
        let started = Instant::now();
        if let Err(e) = mic.start() {
            println!("take {take}: start failed: {e}");
            return ExitCode::FAILURE;
        }
        let start_ms = started.elapsed().as_secs_f64() * 1_000.0;
        std::thread::sleep(Duration::from_secs(seconds));
        let audio = match mic.stop() {
            Ok(audio) => audio,
            Err(e) => {
                println!("take {take}: stop failed: {e}");
                return ExitCode::FAILURE;
            }
        };
        let first_audio = mic.last_first_audio().map_or_else(
            || "none".to_owned(),
            |d| format!("{:.1} ms", d.as_secs_f64() * 1_000.0),
        );
        let rms = audio_math::rms(&audio.samples);
        println!(
            "take {take} ({}): start() {start_ms:.1} ms, first audio {first_audio}, {:?} at {} Hz, RMS {rms:.4}{}",
            if take == 1 { "cold" } else { "warm" },
            audio.duration(),
            audio.sample_rate,
            if audio_math::is_silent(&audio.samples, audio_math::DEFAULT_SILENCE_THRESHOLD) {
                " (silent)"
            } else {
                ""
            },
        );
        if let Some(path) = save.as_deref().filter(|_| take == count) {
            let bytes = match wav::encode_f32(&audio.samples, audio.sample_rate) {
                Ok(bytes) => bytes,
                Err(e) => {
                    println!("could not encode: {e}");
                    return ExitCode::FAILURE;
                }
            };
            if let Err(e) = std::fs::write(path, bytes) {
                println!("could not save {path}: {e}");
                return ExitCode::FAILURE;
            }
            println!("saved {path}");
        }
    }
    ExitCode::SUCCESS
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("mic_probe runs on macOS only");
}
