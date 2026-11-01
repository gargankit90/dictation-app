//! Single-instance launches as real processes (T2 P1-01, #82, #103): cold
//! starts that race, contenders that exit or are killed, hostile local
//! peers, malformed frames and missing acknowledgements.
//!
//! Each launch is this test binary re-run as [`launches_child`] with `SIL_*`
//! environment variables. A child reports what happened as `SIL <event>`
//! lines on stdout, which the test reads with deadlines, and takes orders
//! (`GO`, `RELEASE`, end of input to quit) on stdin. A running instance
//! records every message its callback gets as a receipt file (`r-<key>`)
//! before the callback returns, so a launch can check that a message
//! reported as forwarded had really reached the callback first.
//!
//! The wire format is written out again here, from the description in
//! `single_instance/frame.rs`, rather than borrowed from the crate, so the
//! frames these tests send don't change if the encoder does.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test helpers fail the test on purpose"
)]

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

mod common;

use dictation_platform::single_instance::{Config, Launch, Message, Rejected, acquire};

// --- The wire format (see the module docs) -------------------------------

/// The largest body the running instance accepts.
const MAX_FRAME: usize = 64 * 1024;
/// "Message handled."
const ACK: u8 = 0x06;
/// "Invalid frame; not handled."
const NAK: u8 = 0x15;

fn be(n: usize) -> [u8; 4] {
    u32::try_from(n).unwrap().to_be_bytes()
}

/// A body: the string count, then each string as its length and bytes.
fn body(strings: &[&[u8]]) -> Vec<u8> {
    let mut body = be(strings.len()).to_vec();
    for string in strings {
        body.extend_from_slice(&be(string.len()));
        body.extend_from_slice(string);
    }
    body
}

/// A frame: the body's length, then the body.
fn framed(body: &[u8]) -> Vec<u8> {
    [&be(body.len())[..], body].concat()
}

// --- The child side ------------------------------------------------------

/// What [`launches_child`] does: `launch` (an ordinary launch) or `mute` (a
/// fake running instance that holds the lock and never acknowledges).
const ROLE: &str = "SIL_ROLE";
/// The directory for the lock, socket and receipts.
const DIR: &str = "SIL_DIR";
/// The launch's argument, which is also its receipt key.
const ARG: &str = "SIL_ARG";
/// If set, the launch says `READY` and waits for a line on stdin first.
const GATE: &str = "SIL_GATE";
/// [`Config::io_timeout`] in milliseconds.
const IO_MS: &str = "SIL_IO_MS";
/// [`Config::forward_timeout`] in milliseconds.
const FORWARD_MS: &str = "SIL_FORWARD_MS";
/// How a `mute` instance treats a connection: `silent` (reads, keeps it
/// open, never answers), `close` (reads, closes unanswered) or `garbage`
/// (answers a byte that is neither ACK nor NAK).
const MUTE_MODE: &str = "SIL_MUTE_MODE";

/// The stem of the lock and socket names.
const NAME: &str = "p";

/// Receipt keys the running instance's callback treats specially.
const REJECT: &str = "reject"; // returns `Rejected`
const HOLD: &str = "hold"; // blocks until the instance reads `RELEASE`
const VANISH: &str = "vanish"; // the process exits before answering

fn message(arg: &str) -> Message {
    Message {
        cwd: "/".into(),
        args: vec!["app".into(), arg.into()],
    }
}

/// One event line for the parent test.
fn say(event: &str) {
    let mut out = std::io::stdout().lock();
    writeln!(out, "SIL {event}").unwrap();
    out.flush().unwrap();
}

fn env_ms(name: &str, default: u64) -> Duration {
    Duration::from_millis(
        std::env::var(name)
            .ok()
            .map_or(default, |ms| ms.parse().unwrap()),
    )
}

/// A short, printable name for a message: its last argument, unless that is
/// long or unusual.
fn receipt_key(message: &Message) -> String {
    match message.args.last() {
        None => "noargs".into(),
        Some(arg)
            if !arg.is_empty()
                && arg.len() <= 32
                && arg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') =>
        {
            arg.clone()
        }
        Some(arg) => format!("long{}", arg.len()),
    }
}

/// The running instance's callback: records the receipt (and any duplicate)
/// before it answers, then reports it.
fn receive(dir: &Path, message: &Message, release: &Receiver<()>) -> Result<(), Rejected> {
    let key = receipt_key(message);
    let first = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(format!("r-{key}")))
        .is_ok();
    if !first {
        File::create(dir.join(format!("dup-{key}"))).unwrap();
    }
    say(&format!("RECEIVED {key}"));
    match key.as_str() {
        REJECT => Err(Rejected),
        HOLD => {
            let _ = release.recv();
            Ok(())
        }
        VANISH => std::process::exit(0),
        _ => Ok(()),
    }
}

/// One process of a test below; does nothing when the test runner itself
/// runs it.
#[test]
fn launches_child() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    let mut config = Config::in_dir(std::env::var_os(DIR).unwrap(), NAME);
    config.io_timeout = env_ms(IO_MS, 2000);
    config.forward_timeout = env_ms(FORWARD_MS, 5000);
    match role.as_str() {
        "launch" => child_launch(&config),
        "mute" => child_mute(&config),
        other => panic!("unknown role {other}"),
    }
}

fn child_launch(config: &Config) {
    let arg = std::env::var(ARG).unwrap();
    let mut stdin = std::io::stdin().lines();
    if std::env::var_os(GATE).is_some() {
        say("READY");
        // The start barrier: the parent releases every contender at once.
        assert_eq!(stdin.next().unwrap().unwrap(), "GO");
    }
    let started = Instant::now();
    let launch = acquire(config, &message(&arg));
    let ms = started.elapsed().as_millis();
    match launch {
        Ok(Launch::Forwarded) => {
            let receipt = config.dir.join(format!("r-{arg}")).exists();
            say(&format!("FORWARDED receipt={receipt} ms={ms}"));
        }
        Ok(Launch::Primary(mut instance)) => {
            let (release_tx, release_rx) = mpsc::channel();
            let dir = config.dir.clone();
            instance
                .listen_checked(move |message| receive(&dir, &message, &release_rx))
                .unwrap();
            say("PRIMARY");
            for line in stdin {
                if line.unwrap() == "RELEASE" {
                    release_tx.send(()).unwrap();
                }
            }
            drop(instance);
            say("STOPPED");
        }
        Err(error) => say(&format!("ERROR {:?} ms={ms}", error.kind())),
    }
}

/// Holds the lock and the socket like a running instance, but never
/// acknowledges a message.
fn child_mute(config: &Config) {
    std::fs::create_dir_all(&config.dir).unwrap();
    std::fs::set_permissions(&config.dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let lock = File::create(config.lock_path()).unwrap();
    lock.lock().unwrap();
    let listener = UnixListener::bind(config.socket_path()).unwrap();
    let mode = std::env::var(MUTE_MODE).unwrap();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut buf = [0; 4096];
            let _ = stream.read(&mut buf);
            say("HEARD");
            match mode.as_str() {
                "silent" => held.push(stream),
                "close" => drop(stream),
                "garbage" => {
                    let _ = stream.write_all(&[0]);
                    held.push(stream);
                }
                other => panic!("unknown mute mode {other}"),
            }
        }
    });
    say("READY");
    // Until the parent closes stdin.
    for _ in std::io::stdin().lines() {}
    drop(lock);
}

// --- The parent side -----------------------------------------------------

/// How long a child may take to start and report (generous: the binary is
/// re-run under CPU load). A deadline, not a delay: a passing run never
/// waits this long.
const STARTUP: Duration = Duration::from_secs(30);

/// A fresh short directory (the shared [`common::Scratch`]: unique, not
/// created, removed afterwards) and the receipts the launches leave in it.
struct Scratch(common::Scratch);

impl Scratch {
    fn new() -> Self {
        Self(common::Scratch::new("sil"))
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn config(&self) -> Config {
        Config::in_dir(self.path(), NAME)
    }

    fn has_receipt(&self, key: &str) -> bool {
        self.path().join(format!("r-{key}")).exists()
    }

    /// Receipts recorded twice, i.e. messages acted on more than once.
    fn duplicates(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.path()) else {
            return Vec::new();
        };
        entries
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("dup-"))
            .collect()
    }
}

/// Options for one child.
#[derive(Default)]
struct Opts {
    gate: bool,
    io_ms: Option<u64>,
    forward_ms: Option<u64>,
    mute_mode: Option<&'static str>,
}

/// A child process; killed if the test ends (or panics) while it runs.
struct Proc {
    name: String,
    child: Child,
    stdin: Option<ChildStdin>,
    events: Receiver<String>,
}

impl Proc {
    fn launch(dir: &Scratch, arg: &str, opts: &Opts) -> Self {
        Self::spawn(dir, "launch", arg, opts)
    }

    fn mute(dir: &Scratch, mode: &'static str) -> Self {
        let opts = Opts {
            mute_mode: Some(mode),
            ..Opts::default()
        };
        Self::spawn(dir, "mute", "mute", &opts)
    }

    fn spawn(dir: &Scratch, role: &str, arg: &str, opts: &Opts) -> Self {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "launches_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(ROLE, role)
            .env(DIR, dir.path())
            .env(ARG, arg)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if opts.gate {
            command.env(GATE, "1");
        }
        if let Some(ms) = opts.io_ms {
            command.env(IO_MS, ms.to_string());
        }
        if let Some(ms) = opts.forward_ms {
            command.env(FORWARD_MS, ms.to_string());
        }
        if let Some(mode) = opts.mute_mode {
            command.env(MUTE_MODE, mode);
        }
        let mut child = command.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, events) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                // libtest may print its own text in front on the same line.
                if let Some((_, event)) = line.split_once("SIL ")
                    && tx.send(event.to_owned()).is_err()
                {
                    break;
                }
            }
        });
        Self {
            name: arg.to_owned(),
            stdin: child.stdin.take(),
            child,
            events,
        }
    }

    /// The child's next event.
    fn next(&self, within: Duration) -> String {
        match self.events.recv_timeout(within) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => {
                panic!("{}: no event within {within:?}", self.name)
            }
            Err(RecvTimeoutError::Disconnected) => {
                panic!("{}: exited without another event", self.name)
            }
        }
    }

    fn expect(&self, event: &str) {
        assert_eq!(self.next(STARTUP), event, "{}", self.name);
    }

    /// Sends one line to the child's stdin.
    fn tell(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    }

    /// Waits (up to [`STARTUP`]) for the child to exit after its last
    /// event, and returns how it exited.
    fn exit(mut self) -> ExitStatus {
        self.stdin = None;
        match self.events.recv_timeout(STARTUP) {
            Err(RecvTimeoutError::Disconnected) => {}
            Ok(event) => panic!("{}: unexpected event {event}", self.name),
            Err(RecvTimeoutError::Timeout) => panic!("{}: still running", self.name),
        }
        // Its stdout is closed, so it has exited or is about to.
        self.child.wait().unwrap()
    }

    /// Closes the running instance's stdin, which makes it quit, and checks
    /// that it stops cleanly.
    fn stop(mut self) {
        self.stdin = None;
        self.expect("STOPPED");
        let name = self.name.clone();
        assert!(self.exit().success(), "{name}");
    }

    /// Kills the child (SIGKILL: no cleanup runs).
    fn kill(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts a running instance in its own process.
fn owner(dir: &Scratch, opts: &Opts) -> Proc {
    let owner = Proc::launch(dir, "owner", opts);
    owner.expect("PRIMARY");
    owner
}

/// A launch from this process, which must be forwarded and acknowledged
/// only after the running instance's callback has the message.
fn forward_from_here(dir: &Scratch, owner: &Proc, key: &str) {
    let launch = acquire(&dir.config(), &message(key)).unwrap();
    assert!(matches!(launch, Launch::Forwarded), "{key}: {launch:?}");
    assert!(dir.has_receipt(key), "{key}: forwarded before the callback");
    assert_eq!(owner.next(STARTUP), format!("RECEIVED {key}"));
}

/// The socket's identity: if a contender unlinked it and someone bound a new
/// one, this changes.
fn socket_id(config: &Config) -> (u64, u64) {
    let meta = std::fs::metadata(config.socket_path()).unwrap();
    (meta.dev(), meta.ino())
}

/// What a raw peer got back.
#[derive(Debug, PartialEq, Eq)]
enum Reply {
    Byte(u8),
    /// Closed without an answer.
    Closed,
}

/// Connects as a raw peer, sends `bytes` (half-closing afterwards if
/// `then_eof`) and reads the answer, with a deadline.
fn raw(config: &Config, bytes: &[u8], then_eof: bool) -> Reply {
    let mut stream = UnixStream::connect(config.socket_path()).unwrap();
    stream.set_read_timeout(Some(STARTUP)).unwrap();
    stream.set_write_timeout(Some(STARTUP)).unwrap();
    // A write error means the instance already answered and closed.
    if stream.write_all(bytes).is_ok() && then_eof {
        let _ = stream.shutdown(Shutdown::Write);
    }
    let mut reply = [0];
    match stream.read(&mut reply) {
        Ok(0) => Reply::Closed,
        Ok(_) => Reply::Byte(reply[0]),
        Err(error) if error.kind() == ErrorKind::ConnectionReset => Reply::Closed,
        Err(error) => panic!("no answer and not closed: {error}"),
    }
}

// --- The tests -----------------------------------------------------------

/// Contenders per cold race.
const CONTENDERS: usize = 8;
/// Races per run: one race is too few to hit a narrow window.
const ROUNDS: usize = 3;

/// However many launches start at once, from cold (no directory, no lock,
/// no socket) and without waiting for each other to be ready, exactly one
/// runs and every other one's message reaches its callback exactly once,
/// before that launch is told it was forwarded.
#[test]
fn concurrent_cold_starts_have_one_owner() {
    for round in 0..ROUNDS {
        cold_race(round);
    }
}

fn cold_race(round: usize) {
    let dir = Scratch::new();
    let gated = Opts {
        gate: true,
        ..Opts::default()
    };
    let mut contenders: Vec<Proc> = (0..CONTENDERS)
        .map(|i| Proc::launch(&dir, &format!("c{i}"), &gated))
        .collect();
    // The start barrier: every contender is loaded and blocked on stdin...
    for contender in &contenders {
        contender.expect("READY");
    }
    assert!(!dir.path().exists(), "round {round}: not a cold start");
    // ...and all are released together.
    for contender in &mut contenders {
        contender.tell("GO");
    }

    let mut owner = None;
    let mut forwarded = BTreeSet::new();
    for contender in contenders {
        let event = contender.next(STARTUP);
        if event == "PRIMARY" {
            assert!(owner.is_none(), "round {round}: a second owner");
            owner = Some(contender);
        } else {
            assert!(
                event.starts_with("FORWARDED receipt=true "),
                "round {round}: {} {event}",
                contender.name
            );
            forwarded.insert(contender.name.clone());
            let name = contender.name.clone();
            assert!(contender.exit().success(), "round {round}: {name}");
        }
    }
    let owner = owner.unwrap_or_else(|| panic!("round {round}: no owner"));
    assert_eq!(forwarded.len(), CONTENDERS - 1, "round {round}");

    // Every forwarded launch's message reached the callback.
    let received: BTreeSet<String> = (1..CONTENDERS)
        .map(|_| {
            let event = owner.next(STARTUP);
            event.strip_prefix("RECEIVED ").unwrap().to_owned()
        })
        .collect();
    assert_eq!(received, forwarded, "round {round}");

    // The losers have exited and left the owner's socket working.
    forward_from_here(&dir, &owner, "late");
    assert_eq!(dir.duplicates(), Vec::<String>::new(), "round {round}");
    owner.stop();
    assert!(!dir.config().socket_path().exists(), "round {round}");
}

/// A losing launch can't remove or replace the running instance's socket,
/// whether it is forwarded and exits, is rejected and exits with an error,
/// or is killed while it waits for its answer.
#[test]
fn loser_exit_cannot_unlink_owner_socket() {
    let dir = Scratch::new();
    let config = dir.config();
    let mut owner = owner(&dir, &Opts::default());
    let socket = socket_id(&config);

    // Forwarded, then exits normally.
    let loser = Proc::launch(&dir, "fwd", &Opts::default());
    assert!(loser.next(STARTUP).starts_with("FORWARDED receipt=true "));
    assert!(loser.exit().success());
    owner.expect("RECEIVED fwd");
    assert_eq!(socket_id(&config), socket);
    forward_from_here(&dir, &owner, "after-fwd");

    // Rejected, then exits after its error.
    let loser = Proc::launch(&dir, REJECT, &Opts::default());
    assert!(loser.next(STARTUP).starts_with("ERROR InvalidData "));
    assert!(loser.exit().success());
    owner.expect(&format!("RECEIVED {REJECT}"));
    assert_eq!(socket_id(&config), socket);
    forward_from_here(&dir, &owner, "after-reject");

    // Killed while the callback holds its message, before the answer.
    let loser = Proc::launch(&dir, HOLD, &Opts::default());
    owner.expect(&format!("RECEIVED {HOLD}"));
    loser.kill();
    owner.tell("RELEASE");
    assert_eq!(socket_id(&config), socket);
    forward_from_here(&dir, &owner, "after-kill");

    assert_eq!(dir.duplicates(), Vec::<String>::new());
    owner.stop();
}

/// Peers that connect and send nothing, or send more than the limit, can't
/// keep a real launch (the `--release` of a hotkey, say) from being
/// forwarded within its timeouts.
#[test]
fn idle_or_oversized_peer_does_not_starve_release() {
    // A few hostile peers, and an instance that would wait 10 s for each:
    // the launch gets in at once, long before that.
    {
        let dir = Scratch::new();
        let config = dir.config();
        let owner = owner(
            &dir,
            &Opts {
                io_ms: Some(10_000),
                ..Opts::default()
            },
        );
        // The peers write from their own threads, so an instance that
        // doesn't read them shows up below as a launch that isn't
        // forwarded, not as this test hanging here.
        let connect = || {
            let stream = UnixStream::connect(config.socket_path()).unwrap();
            // Timeouts go on now: macOS refuses them once the peer closes.
            stream.set_write_timeout(Some(STARTUP)).unwrap();
            stream.set_read_timeout(Some(STARTUP)).unwrap();
            stream
        };
        // Connect and send nothing.
        let idle: Vec<_> = (0..3).map(|_| connect()).collect();
        // Announces a full-size body, sends half of it, then stalls.
        let stalled = {
            let mut stream = connect();
            std::thread::spawn(move || {
                let _ = stream.write_all(&be(MAX_FRAME));
                let _ = stream.write_all(&vec![0; MAX_FRAME / 2]);
                stream
            })
        };
        // Announces more than the limit, and stops.
        let mut oversized = connect();
        oversized.write_all(&be(MAX_FRAME + 1)).unwrap();
        // Announces more than the limit and keeps sending, never reading.
        let flood = {
            let mut stream = connect();
            std::thread::spawn(move || {
                let _ = stream.write_all(&be(MAX_FRAME + 1));
                // Fails once the instance has answered and closed.
                let _ = stream.write_all(&vec![0x55; 16 * MAX_FRAME]);
            })
        };

        let release = Proc::launch(&dir, "release", &Opts::default());
        let event = release.next(STARTUP);
        assert!(event.starts_with("FORWARDED receipt=true "), "{event}");
        assert!(release.exit().success());
        owner.expect("RECEIVED release");

        // The oversized peer was refused without its body being read.
        let mut reply = [0];
        oversized.read_exact(&mut reply).unwrap();
        assert_eq!(reply[0], NAK);
        flood.join().unwrap();
        drop((idle, stalled.join().unwrap(), oversized));
        owner.stop();
    }

    // More idle peers than the instance reads at once: the launch's
    // connections are closed unread until the readers time out (1 s), and
    // it gets in by retrying, within its 5 s forward timeout.
    {
        let dir = Scratch::new();
        let config = dir.config();
        let owner = owner(
            &dir,
            &Opts {
                io_ms: Some(1000),
                ..Opts::default()
            },
        );
        let idle: Vec<_> = (0..32)
            .map(|_| UnixStream::connect(config.socket_path()).unwrap())
            .collect();
        let release = Proc::launch(
            &dir,
            "release",
            &Opts {
                forward_ms: Some(5000),
                ..Opts::default()
            },
        );
        let event = release.next(STARTUP);
        assert!(event.starts_with("FORWARDED receipt=true "), "{event}");
        assert!(release.exit().success());
        owner.expect("RECEIVED release");
        drop(idle);
        owner.stop();
    }
}

/// Every kind of malformed frame is refused (or, if cut short, dropped
/// unanswered) without reaching the callback, and the next real launch is
/// still delivered. Frames at the format's edges that are valid are
/// delivered.
#[test]
fn malformed_frame_then_valid_frame_recovers() {
    let dir = Scratch::new();
    let config = dir.config();
    let owner = owner(&dir, &Opts::default());

    let valid = body(&[b"/", b"app", b"--release"]);
    let malformed: Vec<(&str, Vec<u8>, bool, Reply)> = vec![
        ("truncated header", vec![0, 0], true, Reply::Closed),
        (
            "truncated body",
            [&be(valid.len())[..], &valid[..5]].concat(),
            true,
            Reply::Closed,
        ),
        ("empty body", framed(&[]), false, Reply::Byte(NAK)),
        (
            "string longer than the body",
            framed(&[&be(1)[..], &be(100), b"abc"].concat()),
            false,
            Reply::Byte(NAK),
        ),
        (
            "working directory not UTF-8",
            framed(&body(&[b"\xff\xfe", b"app"])),
            false,
            Reply::Byte(NAK),
        ),
        (
            "argument not UTF-8",
            framed(&body(&[b"/", b"app", b"--re\xc3"])),
            false,
            Reply::Byte(NAK),
        ),
        (
            "missing string (count too high)",
            framed(&[&be(3)[..], &body(&[b"/", b"app"])[4..]].concat()),
            false,
            Reply::Byte(NAK),
        ),
        (
            "missing length of the last string",
            framed(&[&be(2)[..], &be(1), b"/"].concat()),
            false,
            Reply::Byte(NAK),
        ),
        (
            "extra length after the last string",
            framed(&[&valid[..], &be(0)].concat()),
            false,
            Reply::Byte(NAK),
        ),
        (
            "extra string beyond the count",
            framed(&[&be(1)[..], &body(&[b"/", b"app"])[4..]].concat()),
            false,
            Reply::Byte(NAK),
        ),
        (
            "count far beyond the body",
            framed(&[&u32::MAX.to_be_bytes()[..], &be(0)].concat()),
            false,
            Reply::Byte(NAK),
        ),
        (
            "empty argv, not even a working directory",
            framed(&be(0)),
            false,
            Reply::Byte(NAK),
        ),
        (
            "body one byte over the limit",
            be(MAX_FRAME + 1).to_vec(),
            false,
            Reply::Byte(NAK),
        ),
        (
            "over the limit, body following",
            [&be(MAX_FRAME + 1)[..], &[0x41; 1000]].concat(),
            false,
            Reply::Byte(NAK),
        ),
        (
            "largest length a header holds",
            u32::MAX.to_be_bytes().to_vec(),
            false,
            Reply::Byte(NAK),
        ),
    ];
    for (i, (case, bytes, then_eof, expected)) in malformed.into_iter().enumerate() {
        assert_eq!(raw(&config, &bytes, then_eof), expected, "{case}");
        // The next event is this launch's: the bad frame reached no callback.
        forward_from_here(&dir, &owner, &format!("ok{i}"));
    }

    // Valid frames at the edges of the format are delivered.
    let long = "x".repeat(MAX_FRAME - 20);
    let long_key = format!("long{}", long.len());
    let edges: Vec<(&str, Vec<u8>, &str)> = vec![
        ("a hand-built frame", framed(&valid), "--release"),
        (
            "a working directory and no argv",
            framed(&body(&[b"/"])),
            "noargs",
        ),
        (
            "a body exactly at the limit",
            framed(&body(&[b"/", b"app", long.as_bytes()])),
            &long_key,
        ),
    ];
    for (case, bytes, key) in edges {
        assert_eq!(raw(&config, &bytes, false), Reply::Byte(ACK), "{case}");
        assert_eq!(owner.next(STARTUP), format!("RECEIVED {key}"), "{case}");
    }

    assert_eq!(dir.duplicates(), Vec::<String>::new());
    owner.stop();
}

/// A launch counts as forwarded only once the running instance has
/// acknowledged that its callback took the message: a rejection, an
/// instance that doesn't answer in time, one that dies before answering,
/// one that closes the connection unanswered and one that answers anything
/// but ACK all leave the launch not forwarded.
#[test]
fn forwarded_launch_requires_delivery_ack() {
    // A real instance whose callback rejects the message.
    {
        let dir = Scratch::new();
        let owner = owner(&dir, &Opts::default());
        let launch = Proc::launch(&dir, REJECT, &Opts::default());
        let event = launch.next(STARTUP);
        assert!(event.starts_with("ERROR InvalidData "), "{event}");
        owner.expect(&format!("RECEIVED {REJECT}"));
        drop(launch);
        // The next launch is acknowledged as usual.
        let launch = Proc::launch(&dir, "accepted", &Opts::default());
        assert!(launch.next(STARTUP).starts_with("FORWARDED receipt=true "));
        owner.expect("RECEIVED accepted");
        drop(launch);
        owner.stop();
    }

    // A real instance whose callback takes longer than the launch waits.
    {
        let dir = Scratch::new();
        let mut owner = owner(&dir, &Opts::default());
        let launch = Proc::launch(
            &dir,
            HOLD,
            &Opts {
                io_ms: Some(500),
                ..Opts::default()
            },
        );
        owner.expect(&format!("RECEIVED {HOLD}"));
        let event = launch.next(STARTUP);
        assert!(event.starts_with("ERROR TimedOut "), "{event}");
        owner.tell("RELEASE");
        owner.stop();
    }

    // A real instance that exits after its callback took the message but
    // before answering: the launch isn't forwarded; it runs itself.
    {
        let dir = Scratch::new();
        let owner = owner(&dir, &Opts::default());
        let launch = Proc::launch(&dir, VANISH, &Opts::default());
        owner.expect(&format!("RECEIVED {VANISH}"));
        assert!(owner.exit().success());
        launch.expect("PRIMARY");
        // And it works as the running instance.
        forward_from_here(&dir, &launch, "after");
        launch.stop();
    }

    // Fake instances holding the lock: one that reads and never answers,
    // one that closes unanswered, and one that answers garbage.
    for (mode, expected) in [
        ("silent", "ERROR TimedOut "),
        ("close", "ERROR TimedOut "),
        ("garbage", "ERROR InvalidData "),
    ] {
        let dir = Scratch::new();
        let fake = Proc::mute(&dir, mode);
        fake.expect("READY");
        let launch = Proc::launch(
            &dir,
            "unanswered",
            &Opts {
                io_ms: Some(500),
                forward_ms: Some(600),
                ..Opts::default()
            },
        );
        fake.expect("HEARD");
        let event = launch.next(STARTUP);
        assert!(event.starts_with(expected), "{mode}: {event}");
        assert!(launch.exit().success(), "{mode}");
    }
}
