//! Quitting while launches are being forwarded (M1.7.1): quitting waits for
//! at most `2 * io_timeout` for a stuck callback, and a forwarded launch's
//! arguments are acted on at most once.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail the test on purpose"
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use dictation_platform::single_instance::{Config, Launch, Message, acquire};

mod common;
use common::Scratch;

/// The scratch directory goes when the first value is dropped, so bind it.
fn config(tag: &str) -> (Scratch, Config) {
    let scratch = Scratch::new(tag);
    let mut config = Config::in_dir(scratch.path(), "t");
    config.io_timeout = Duration::from_millis(500);
    (scratch, config)
}

fn message(arg: &str) -> Message {
    Message {
        cwd: "/".into(),
        args: vec!["app".into(), arg.into()],
    }
}

#[test]
fn quitting_with_a_stuck_callback_is_bounded() {
    let (_scratch, config) = config("stuck");
    let Launch::Primary(mut instance) = acquire(&config, &message("primary")).unwrap() else {
        panic!("not the primary");
    };
    let (entered_tx, entered) = mpsc::channel();
    instance
        .listen(move |_| {
            let _ = entered_tx.send(());
            std::thread::sleep(Duration::from_secs(30));
        })
        .unwrap();
    let forwarder = {
        let config = config.clone();
        std::thread::spawn(move || {
            let _ = acquire(&config, &message("stuck"));
        })
    };
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let started = Instant::now();
    drop(instance);
    let took = started.elapsed();
    assert!(
        took < config.io_timeout * 2 + Duration::from_millis(1500),
        "quit took {took:?}"
    );
    drop(forwarder);
}

#[test]
fn quitting_during_a_burst_acts_on_each_forward_at_most_once() {
    for round in 0..20 {
        let (_scratch, config) = config(&format!("burst{round}"));
        let Launch::Primary(mut instance) = acquire(&config, &message("primary")).unwrap() else {
            panic!("not the primary");
        };
        let seen: Arc<Mutex<HashMap<String, usize>>> = Arc::default();
        let log = Arc::clone(&seen);
        instance
            .listen(move |m| {
                *log.lock().unwrap().entry(m.args[1].clone()).or_default() += 1;
                std::thread::sleep(Duration::from_millis(5));
            })
            .unwrap();
        let launches: Vec<_> = (0..8)
            .map(|i| {
                let config = config.clone();
                std::thread::spawn(move || {
                    // Forwarded, or (after the quit) a new primary: both are fine.
                    matches!(
                        acquire(&config, &message(&i.to_string())),
                        Ok(Launch::Forwarded)
                    )
                })
            })
            .collect();
        std::thread::sleep(Duration::from_millis(3));
        drop(instance);
        let mut forwarded = 0;
        for launch in launches {
            if launch.join().unwrap() {
                forwarded += 1;
            }
        }
        let seen = seen.lock().unwrap();
        assert!(seen.values().all(|n| *n == 1), "round {round}: {seen:?}");
        // A launch told it was forwarded had its message acted on.
        assert!(
            seen.len() >= forwarded,
            "round {round}: {seen:?} {forwarded}"
        );
    }
}
