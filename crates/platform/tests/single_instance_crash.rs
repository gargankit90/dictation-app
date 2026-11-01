//! An instance killed without cleanup (SIGKILL, power loss) leaves its socket
//! file behind; the OS drops its lock. The next launch must take over, and the
//! launch after that must forward to it (M1.7 #1, stale socket).
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use dictation_platform::single_instance::{Config, Launch, Message, acquire};

mod common;
use common::Scratch;

const CHILD_DIR: &str = "SI_CRASH_CHILD_DIR";

fn message(arg: &str) -> Message {
    Message {
        cwd: "/".into(),
        args: vec!["app".into(), arg.into()],
    }
}

/// The primary that gets killed; does nothing under the test runner itself.
#[test]
fn crash_child() {
    let Some(dir) = std::env::var_os(CHILD_DIR) else {
        return;
    };
    let config = Config::in_dir(dir, "c");
    let Launch::Primary(mut instance) = acquire(&config, &message("child")).unwrap() else {
        panic!("the child must be the primary");
    };
    instance.listen(|_| {}).unwrap();
    println!("READY");
    std::thread::sleep(Duration::from_secs(60));
}

#[test]
fn a_killed_instance_leaves_a_stale_socket_and_the_next_launch_recovers() {
    let scratch = Scratch::new("crash");
    let dir = scratch.path();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_child", "--nocapture", "--test-threads=1"])
        .env(CHILD_DIR, dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    assert!(
        lines.any(|l| l.unwrap().ends_with("READY")),
        "child never listened"
    );

    let config = Config::in_dir(dir, "c");
    assert!(config.socket_path().exists());
    child.kill().unwrap(); // SIGKILL: no Drop, no cleanup
    child.wait().unwrap();
    assert!(
        config.socket_path().exists(),
        "the kill left a stale socket"
    );

    // The next launch takes over (does not fail, does not forward to nobody)...
    let Launch::Primary(mut instance) = acquire(&config, &message("after-crash")).unwrap() else {
        panic!("a launch after a crash must become the primary");
    };
    let (tx, rx) = mpsc::channel();
    instance.listen(move |m| tx.send(m).unwrap()).unwrap();

    // ...and the one after that forwards to it.
    assert!(matches!(
        acquire(&config, &message("forwarded")).unwrap(),
        Launch::Forwarded
    ));
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        message("forwarded")
    );
}
