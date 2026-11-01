//! Races between launches (M1.7 #1, issue #82 B1): however many start at
//! once, exactly one runs, the rest hand it their arguments, and none of
//! them breaks the running instance's socket.
#![cfg(unix)]

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dictation_platform::single_instance::{Config, Launch, Message, acquire};

const LAUNCHES: usize = 6;

/// Environment variables that make [`child_launch`] act as one launch.
const CHILD_DIR: &str = "SINGLE_INSTANCE_CHILD_DIR";
const CHILD_START_MS: &str = "SINGLE_INSTANCE_CHILD_START_MS";
const CHILD_ARG: &str = "SINGLE_INSTANCE_CHILD_ARG";

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("si-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn message(arg: &str) -> Message {
    Message {
        cwd: "/".into(),
        args: vec!["app".into(), arg.into()],
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[test]
fn concurrent_launches_in_threads_start_one_instance() {
    let config = Config::in_dir(temp_dir("threads"), "t");
    let barrier = Arc::new(Barrier::new(LAUNCHES));
    let (tx, rx) = mpsc::channel();
    let launches: Vec<_> = (0..LAUNCHES)
        .map(|i| {
            let config = config.clone();
            let barrier = Arc::clone(&barrier);
            let tx = tx.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let launch = acquire(&config, &message(&i.to_string())).unwrap();
                if let Launch::Primary(mut instance) = launch {
                    instance
                        .listen(move |message| tx.send(message).unwrap())
                        .unwrap();
                    Some(instance)
                } else {
                    None
                }
            })
        })
        .collect();
    let instances: Vec<_> = launches
        .into_iter()
        .filter_map(|launch| launch.join().unwrap())
        .collect();
    assert_eq!(instances.len(), 1, "exactly one launch runs");

    // Every other launch was acknowledged, so its message has arrived.
    let received: Vec<_> = rx.try_iter().collect();
    assert_eq!(received.len(), LAUNCHES - 1);

    // The losers are gone and the socket still works.
    assert!(config.socket_path().exists());
    assert!(matches!(
        acquire(&config, &message("after")).unwrap(),
        Launch::Forwarded
    ));
    assert_eq!(rx.try_recv().unwrap(), message("after"));
}

/// One launch in its own process, run by
/// [`concurrent_launches_in_processes_start_one_instance`]; does nothing
/// when run by the test runner itself.
#[test]
fn child_launch() {
    let (Some(dir), Some(start), Some(arg)) = (
        std::env::var_os(CHILD_DIR),
        std::env::var(CHILD_START_MS).ok(),
        std::env::var(CHILD_ARG).ok(),
    ) else {
        return;
    };
    // Start together: wait for the agreed moment.
    let start: u128 = start.parse().unwrap();
    while now_ms() < start {
        std::hint::spin_loop();
    }
    let config = Config::in_dir(dir, "p");
    match acquire(&config, &message(&arg)).unwrap() {
        Launch::Forwarded => println!("RESULT forwarded"),
        Launch::Primary(mut instance) => {
            let (tx, rx) = mpsc::channel();
            instance
                .listen(move |message| tx.send(message).unwrap())
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut received = 0;
            while received < LAUNCHES - 1 && Instant::now() < deadline {
                if rx.recv_timeout(Duration::from_millis(100)).is_ok() {
                    received += 1;
                }
            }
            // The losers have exited by now; none of them removed the socket.
            let intact = config.socket_path().exists();
            println!("RESULT primary received={received} socket_intact={intact}");
        }
    }
}

#[test]
fn concurrent_launches_in_processes_start_one_instance() {
    let dir = temp_dir("procs");
    let exe = std::env::current_exe().unwrap();
    let start = now_ms() + 500;
    let children: Vec<_> = (0..LAUNCHES)
        .map(|i| {
            Command::new(&exe)
                .args(["--exact", "child_launch", "--nocapture", "--test-threads=1"])
                .env(CHILD_DIR, &dir)
                .env(CHILD_START_MS, start.to_string())
                .env(CHILD_ARG, i.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    let results: Vec<String> = children
        .into_iter()
        .map(|child| {
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .find_map(|line| {
                    line.split_once("RESULT ")
                        .map(|(_, result)| result.to_owned())
                })
                .unwrap()
        })
        .collect();

    let primaries: Vec<_> = results
        .iter()
        .filter(|r| r.starts_with("primary"))
        .collect();
    let forwarded = results.iter().filter(|r| *r == "forwarded").count();
    assert_eq!(primaries.len(), 1, "{results:?}");
    assert_eq!(forwarded, LAUNCHES - 1, "{results:?}");
    assert_eq!(
        primaries[0],
        &format!("primary received={} socket_intact=true", LAUNCHES - 1)
    );

    // The instance has exited: its socket is gone and the next launch runs.
    let config = Config::in_dir(&dir, "p");
    assert!(!config.socket_path().exists());
    assert!(matches!(
        acquire(&config, &message("next")).unwrap(),
        Launch::Primary(_)
    ));
}
