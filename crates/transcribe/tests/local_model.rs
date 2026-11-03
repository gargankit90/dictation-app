//! L1 criterion 1 against a real model, skipped unless one is present.
//!
//! Set `DICTATION_LOCAL_MODEL` to a GGUF file (for example the catalogue's
//! Parakeet download) to run it. With `DICTATION_LOCAL_SPEECH` (a 16 kHz mono
//! 16-bit WAV of speech) and `DICTATION_LOCAL_EXPECT` (a word it says), the
//! text is checked too; otherwise one second of silence must transcribe
//! without an error.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::path::PathBuf;

use dictation_ports::{Audio, Transcriber};
use dictation_transcribe::local::{CppModel, LocalTranscriber};

fn read_wav(path: &str) -> Audio {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    assert_eq!((spec.channels, spec.sample_rate), (1, 16_000), "{spec:?}");
    let samples = reader
        .samples::<i16>()
        .map(|s| f32::from(s.unwrap()) / 32_768.0)
        .collect();
    Audio {
        sample_rate: 16_000,
        samples,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_real_gguf_model_transcribes_16_khz_audio() {
    let Some(model) = std::env::var_os("DICTATION_LOCAL_MODEL").map(PathBuf::from) else {
        eprintln!("skipped: DICTATION_LOCAL_MODEL is not set");
        return;
    };
    let loaded = tokio::task::spawn_blocking(move || CppModel::load(&model))
        .await
        .unwrap()
        .unwrap();
    let engine = LocalTranscriber::new(Box::new(loaded), None);
    match (
        std::env::var("DICTATION_LOCAL_SPEECH"),
        std::env::var("DICTATION_LOCAL_EXPECT"),
    ) {
        (Ok(wav), Ok(expect)) => {
            let started = std::time::Instant::now();
            let text = engine.transcribe(&read_wav(&wav)).await.unwrap();
            eprintln!("transcribed in {:?}", started.elapsed());
            assert!(
                text.to_lowercase().contains(&expect.to_lowercase()),
                "{text:?} lacks {expect:?}"
            );
        }
        _ => {
            let silence = Audio {
                sample_rate: 16_000,
                samples: vec![0.0; 16_000],
            };
            engine.transcribe(&silence).await.unwrap();
        }
    }
}
