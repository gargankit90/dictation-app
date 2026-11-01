//! Tester probe (M1.7.2): many launches create a not-yet-existing,
//! nested config directory at the same moment. None may fail with
//! `AlreadyExists` ("File exists") or any other error.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail the test on purpose"
)]

use std::sync::{Arc, Barrier};
use std::time::Duration;

use dictation_platform::single_instance::{Config, Launch, Message, acquire};

mod common;
use common::Scratch;

const ROUNDS: usize = 300;
const LAUNCHES: usize = 8;

#[test]
fn simultaneous_first_launches_never_fail_creating_the_directory() {
    let scratch = Scratch::new("mk");
    let base = scratch.path();
    let mut errors = Vec::new();
    for round in 0..ROUNDS {
        // Nested and missing, like a first launch ever.
        let dir = base.join(format!("r{round}")).join("app").join("single");
        let mut config = Config::in_dir(dir, "t");
        config.forward_timeout = Duration::from_secs(5);
        config.io_timeout = Duration::from_millis(500);
        let barrier = Arc::new(Barrier::new(LAUNCHES));
        let handles: Vec<_> = (0..LAUNCHES)
            .map(|i| {
                let (config, barrier) = (config.clone(), Arc::clone(&barrier));
                std::thread::spawn(move || {
                    barrier.wait();
                    let message = Message {
                        cwd: "/".into(),
                        args: vec!["app".into(), i.to_string()],
                    };
                    acquire(&config, &message).map(|launch| match launch {
                        Launch::Primary(mut instance) => {
                            instance.listen(|_| {}).unwrap();
                            Some(instance)
                        }
                        _ => None,
                    })
                })
            })
            .collect();
        let mut primaries = Vec::new();
        for handle in handles {
            match handle.join().unwrap() {
                Ok(Some(instance)) => primaries.push(instance),
                Ok(None) => {}
                Err(error) => errors.push(format!("round {round}: {:?}: {error}", error.kind())),
            }
        }
        drop(primaries);
    }
    assert!(
        errors.is_empty(),
        "{} failures, first: {:?}",
        errors.len(),
        &errors[..errors.len().min(5)]
    );
}
