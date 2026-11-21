//! Q4: the real cleanup helper with the real model, skipped unless both
//! are present.
//!
//! Set `DICTATION_LLAMA_SERVER` to a `llama-server` binary (what
//! `scripts/build-llama-server.sh` builds) and `DICTATION_CLEANUP_MODEL` to
//! the Gemma 4 E2B GGUF to run it. It starts the helper, cleans one
//! invented transcript, checks the fillers are gone and the words kept, and
//! that stopping it ends the process.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dictation_ports::Cleaner;
use dictation_transcribe::llama::{LlamaServer, LocalCleaner};

#[tokio::test(flavor = "multi_thread")]
async fn the_real_helper_cleans_a_transcript() {
    let (Some(helper), Some(model)) = (
        std::env::var_os("DICTATION_LLAMA_SERVER").map(PathBuf::from),
        std::env::var_os("DICTATION_CLEANUP_MODEL").map(PathBuf::from),
    ) else {
        eprintln!("skipped: DICTATION_LLAMA_SERVER or DICTATION_CLEANUP_MODEL is not set");
        return;
    };
    let started = Instant::now();
    let server = Arc::new(LlamaServer::start(&helper, &model).await.unwrap());
    eprintln!("ready in {:?}", started.elapsed());
    let pid = server.server_pid().unwrap();
    let cleaner = LocalCleaner::new(Arc::clone(&server));
    let raw = "um so can you uh send me the the budget by friday";
    let started = Instant::now();
    let cleaned = cleaner.clean(raw).await.unwrap();
    eprintln!("cleaned in {:?}", started.elapsed());
    let lower = cleaned.to_lowercase();
    assert!(
        lower.contains("budget") && lower.contains("friday"),
        "{cleaned}"
    );
    assert!(
        !lower.contains(" uh ") && !lower.contains("the the"),
        "{cleaned}"
    );
    // A second request is faster: the prompt's fixed part is cached.
    let started = Instant::now();
    cleaner
        .clean("okay and the the meeting is at noon")
        .await
        .unwrap();
    eprintln!("second in {:?}", started.elapsed());

    server.stop();
    for _ in 0..250 {
        let alive = std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success();
        if !alive {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the helper is still running after stop");
}
