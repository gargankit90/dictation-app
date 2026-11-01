//! The Unix-socket transport: the running instance's server and a second
//! launch's client.

use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::frame::{self, ACK, HEADER, NAK};
use super::{Config, Forward, Message, Rejected};

/// The most connections read at once. More are closed unread; their
/// launches retry. Keeps idle or slow peers from tying up unbounded threads,
/// while a few of them can't keep a real launch out.
pub const MAX_READERS: usize = 8;

type Callback = Box<dyn FnMut(Message) -> Result<(), Rejected> + Send>;

/// The running instance's socket, bound while the lock is held.
#[derive(Debug)]
pub struct Server {
    socket: PathBuf,
    io_timeout: Duration,
    /// Taken by [`Server::listen`].
    listener: Option<UnixListener>,
    stopping: Arc<AtomicBool>,
}

impl Server {
    /// Removes a socket a previous instance left behind and binds a new one.
    /// Call only while holding the lock: that's what makes the leftover
    /// safe to remove.
    pub fn bind(config: &Config) -> io::Result<Self> {
        let socket = config.socket_path();
        match std::fs::remove_file(&socket) {
            Ok(()) => tracing::debug!(path = %socket.display(), "removed a stale socket"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let listener = UnixListener::bind(&socket)?;
        Ok(Self {
            socket,
            io_timeout: config.io_timeout,
            listener: Some(listener),
            stopping: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Accepts connections in a background thread, each read in its own
    /// thread, and runs `on_message` for each valid message, one at a time.
    /// Connections that arrive before this wait in the socket's backlog.
    pub fn listen(&mut self, on_message: Callback) -> io::Result<()> {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "already listening"))?;
        let accepting = Accepting {
            on_message: Arc::new(Mutex::new(on_message)),
            readers: Arc::new(AtomicUsize::new(0)),
            io_timeout: self.io_timeout,
            stopping: Arc::clone(&self.stopping),
        };
        std::thread::Builder::new()
            .name("single-instance".into())
            .spawn(move || accepting.run(&listener))?;
        Ok(())
    }

    /// Stops accepting and removes the socket. The lock must still be held,
    /// so no other instance has bound a socket at this path.
    pub fn stop(&mut self) {
        if self.stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        if self.listener.take().is_none() {
            // Wake the accept loop so it sees `stopping` and ends.
            let _ = UnixStream::connect(&self.socket);
        }
        if let Err(error) = std::fs::remove_file(&self.socket) {
            tracing::debug!(%error, "could not remove the socket");
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The accept loop's state.
struct Accepting {
    on_message: Arc<Mutex<Callback>>,
    readers: Arc<AtomicUsize>,
    io_timeout: Duration,
    stopping: Arc<AtomicBool>,
}

impl Accepting {
    fn run(&self, listener: &UnixListener) {
        for stream in listener.incoming() {
            if self.stopping.load(Ordering::SeqCst) {
                return;
            }
            match stream {
                Ok(stream) => self.read_in_thread(stream),
                Err(error) => {
                    tracing::debug!(%error, "single-instance accept failed");
                    // Out of file descriptors, say: don't spin.
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    fn read_in_thread(&self, stream: UnixStream) {
        let Some(slot) = Slot::take(&self.readers) else {
            tracing::debug!("single-instance: too many connections; closing one unread");
            return;
        };
        let on_message = Arc::clone(&self.on_message);
        let io_timeout = self.io_timeout;
        let spawned = std::thread::Builder::new()
            .name("single-instance-reader".into())
            .spawn(move || {
                let _slot = slot;
                serve(stream, io_timeout, &on_message);
            });
        if let Err(error) = spawned {
            tracing::debug!(%error, "single-instance: could not start a reader");
        }
    }
}

/// One of the [`MAX_READERS`] reader slots, given back when dropped (even
/// if the callback panics).
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(readers: &Arc<AtomicUsize>) -> Option<Self> {
        readers
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < MAX_READERS).then_some(n + 1)
            })
            .ok()
            .map(|_| Self(Arc::clone(readers)))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Reads one message within `io_timeout`, runs the callback and answers
/// [`ACK`] if it took the message, or [`NAK`] if it rejected it or the frame
/// is invalid. A peer that's too slow gets no answer.
fn serve(mut stream: UnixStream, io_timeout: Duration, on_message: &Mutex<Callback>) {
    let deadline = Instant::now() + io_timeout;
    let reply = match read_message(&mut stream, deadline) {
        Ok(message) => {
            let mut on_message = on_message.lock().unwrap_or_else(PoisonError::into_inner);
            match on_message(message) {
                Ok(()) => ACK,
                Err(Rejected) => NAK,
            }
        }
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            tracing::debug!(%error, "single-instance: rejected a message");
            NAK
        }
        Err(error) => {
            tracing::debug!(%error, "single-instance: no message read");
            return;
        }
    };
    let answered = stream
        .set_write_timeout(Some(io_timeout))
        .and_then(|()| stream.write_all(&[reply]));
    if let Err(error) = answered {
        tracing::debug!(%error, "single-instance: could not answer");
    }
}

fn read_message(stream: &mut UnixStream, deadline: Instant) -> io::Result<Message> {
    let mut header = [0; HEADER];
    read_by(stream, &mut header, deadline)?;
    let mut body = vec![0; frame::body_len(header)?];
    read_by(stream, &mut body, deadline)?;
    frame::decode(&body)
}

/// Fills `buf` from `stream`, or fails with [`io::ErrorKind::TimedOut`] at
/// `deadline`: a total deadline, so a peer can't stretch it by trickling
/// bytes.
fn read_by(stream: &mut UnixStream, buf: &mut [u8], deadline: Instant) -> io::Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        stream.set_read_timeout(Some(left))?;
        match stream.read(&mut buf[filled..]) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if is_timeout(&error) => return Err(io::ErrorKind::TimedOut.into()),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

/// Sends `frame` to the running instance and waits for its answer.
pub(super) fn forward(config: &Config, frame: &[u8]) -> Result<(), Forward> {
    let mut stream = UnixStream::connect(config.socket_path()).map_err(Forward::Retry)?;
    stream
        .set_write_timeout(Some(config.io_timeout))
        .and_then(|()| stream.set_read_timeout(Some(config.io_timeout)))
        // macOS fails these with EINVAL once the instance has closed the
        // connection unread (it was busy), so they're worth a retry too.
        .map_err(Forward::Retry)?;
    // A write error means the instance closed the connection unread.
    stream.write_all(frame).map_err(Forward::Retry)?;
    let mut reply = [0];
    match stream.read(&mut reply) {
        Ok(1) if reply[0] == ACK => Ok(()),
        Ok(1) => Err(Forward::Fail(io::Error::new(
            io::ErrorKind::InvalidData,
            "the running instance couldn't act on this launch's arguments (it may still be starting up or shutting down)",
        ))),
        // Closed unread: busy, or stopping.
        Ok(_) => Err(Forward::Retry(io::ErrorKind::UnexpectedEof.into())),
        Err(error) if is_timeout(&error) => Err(Forward::Fail(io::Error::new(
            io::ErrorKind::TimedOut,
            "the running instance didn't answer",
        ))),
        Err(error) => Err(Forward::Retry(error)),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::single_instance::{Instance, Launch, acquire};

    /// A fresh, short directory (socket paths are limited to ~100 bytes).
    fn temp_config() -> Config {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("si-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut config = Config::in_dir(dir, "t");
        config.forward_timeout = Duration::from_secs(3);
        config.io_timeout = Duration::from_millis(300);
        config
    }

    fn message(arg: &str) -> Message {
        Message {
            cwd: "/".into(),
            args: vec!["app".into(), arg.into()],
        }
    }

    fn primary(config: &Config) -> (Instance, mpsc::Receiver<Message>) {
        let Launch::Primary(mut instance) = acquire(config, &message("first")).unwrap() else {
            panic!("not the primary");
        };
        let (tx, rx) = mpsc::channel();
        instance
            .listen(move |message| tx.send(message).unwrap())
            .unwrap();
        (instance, rx)
    }

    fn send_raw(config: &Config, bytes: &[u8]) -> io::Result<u8> {
        let mut stream = UnixStream::connect(config.socket_path())?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.write_all(bytes)?;
        let mut reply = [0];
        stream.read_exact(&mut reply)?;
        Ok(reply[0])
    }

    #[test]
    fn a_second_launch_forwards_and_is_acknowledged() {
        let config = temp_config();
        let (_instance, rx) = primary(&config);
        assert!(matches!(
            acquire(&config, &message("--release")).unwrap(),
            Launch::Forwarded
        ));
        // Acknowledged means already handled.
        assert_eq!(rx.try_recv().unwrap(), message("--release"));
    }

    #[test]
    fn the_directory_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let config = temp_config();
        let (_instance, _rx) = primary(&config);
        let mode = std::fs::metadata(&config.dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn a_stale_socket_is_replaced() {
        let config = temp_config();
        std::fs::create_dir_all(&config.dir).unwrap();
        // A socket file nobody listens on, as a crash leaves behind.
        drop(UnixListener::bind(config.socket_path()).unwrap());
        assert!(config.socket_path().exists());
        let (_instance, rx) = primary(&config);
        assert!(matches!(
            acquire(&config, &message("x")).unwrap(),
            Launch::Forwarded
        ));
        assert_eq!(rx.try_recv().unwrap(), message("x"));
    }

    #[test]
    fn dropping_the_instance_removes_the_socket_and_frees_the_lock() {
        let config = temp_config();
        let (instance, _rx) = primary(&config);
        assert!(config.socket_path().exists());
        drop(instance);
        assert!(!config.socket_path().exists());
        let (_next, rx) = primary(&config);
        assert!(matches!(
            acquire(&config, &message("y")).unwrap(),
            Launch::Forwarded
        ));
        assert_eq!(rx.try_recv().unwrap(), message("y"));
    }

    #[test]
    fn dropping_an_instance_that_never_listened_removes_the_socket() {
        let config = temp_config();
        let Launch::Primary(instance) = acquire(&config, &message("a")).unwrap() else {
            panic!("not the primary");
        };
        drop(instance);
        assert!(!config.socket_path().exists());
    }

    #[test]
    fn listening_twice_fails() {
        let config = temp_config();
        let (mut instance, _rx) = primary(&config);
        let error = instance.listen(|_| {}).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn a_silent_peer_does_not_hold_up_a_launch() {
        let mut config = temp_config();
        // Long enough that waiting out the silent peer would be obvious.
        config.io_timeout = Duration::from_secs(10);
        let (_instance, rx) = primary(&config);
        // Connected, never writes, never closes.
        let _silent = UnixStream::connect(config.socket_path()).unwrap();
        let started = Instant::now();
        assert!(matches!(
            acquire(&config, &message("--release")).unwrap(),
            Launch::Forwarded
        ));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(rx.try_recv().unwrap(), message("--release"));
    }

    #[test]
    fn more_silent_peers_than_readers_only_delay_a_launch_to_the_io_timeout() {
        let config = temp_config();
        let (_instance, rx) = primary(&config);
        let silent: Vec<_> = (0..MAX_READERS * 2)
            .map(|_| UnixStream::connect(config.socket_path()).unwrap())
            .collect();
        std::thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        assert!(matches!(
            acquire(&config, &message("late")).unwrap(),
            Launch::Forwarded
        ));
        // The readers time out after io_timeout (300 ms); then a slot frees
        // and the launch's retry gets in, well before forward_timeout (3 s).
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(rx.try_recv().unwrap(), message("late"));
        drop(silent);
    }

    #[test]
    fn a_trickling_peer_is_cut_off_at_the_deadline() {
        let config = temp_config();
        let (_instance, _rx) = primary(&config);
        let mut slow = UnixStream::connect(config.socket_path()).unwrap();
        slow.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let started = Instant::now();
        // A header announcing 100 bytes, then one byte at a time.
        slow.write_all(&100u32.to_be_bytes()).unwrap();
        let mut reply = [0];
        let read = loop {
            if slow.write_all(&[0]).is_err() {
                break slow.read(&mut reply);
            }
            std::thread::sleep(Duration::from_millis(50));
            if started.elapsed() > Duration::from_secs(2) {
                panic!("still reading after the deadline");
            }
        };
        // Closed without an answer.
        assert!(matches!(read, Ok(0) | Err(_)));
        assert!(started.elapsed() >= config.io_timeout);
    }

    #[test]
    fn an_oversized_frame_is_rejected_without_reading_it() {
        let config = temp_config();
        let (_instance, rx) = primary(&config);
        let header = u32::try_from(frame::MAX_FRAME + 1).unwrap().to_be_bytes();
        assert_eq!(send_raw(&config, &header).unwrap(), NAK);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_garbled_frame_is_rejected_and_the_next_launch_still_forwards() {
        let config = temp_config();
        let (_instance, rx) = primary(&config);
        let garbled = [&3u32.to_be_bytes()[..], &[0xff, 0xff, 0xff]].concat();
        assert_eq!(send_raw(&config, &garbled).unwrap(), NAK);
        let mut bad_utf8 = frame::encode(&message("é")).unwrap();
        *bad_utf8.last_mut().unwrap() = 0xff;
        assert_eq!(send_raw(&config, &bad_utf8).unwrap(), NAK);
        assert!(rx.try_recv().is_err());
        assert!(matches!(
            acquire(&config, &message("ok")).unwrap(),
            Launch::Forwarded
        ));
        assert_eq!(rx.try_recv().unwrap(), message("ok"));
    }

    #[test]
    fn a_rejected_message_fails_the_launch() {
        let config = temp_config();
        std::fs::create_dir_all(&config.dir).unwrap();
        // A fake instance: holds the lock, answers NAK to everything.
        let lock = std::fs::File::create(config.lock_path()).unwrap();
        lock.lock().unwrap();
        let listener = UnixListener::bind(config.socket_path()).unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 64];
            let _ = stream.read(&mut buf);
            stream.write_all(&[NAK]).unwrap();
        });
        let error = acquire(&config, &message("x")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn an_unanswered_message_fails_the_launch_after_the_io_timeout() {
        let config = temp_config();
        std::fs::create_dir_all(&config.dir).unwrap();
        // A fake instance that reads but never answers.
        let lock = std::fs::File::create(config.lock_path()).unwrap();
        lock.lock().unwrap();
        let listener = UnixListener::bind(config.socket_path()).unwrap();
        let (done, wait) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            let _ = wait.recv();
        });
        let started = Instant::now();
        let error = acquire(&config, &message("x")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < config.forward_timeout);
        drop(done);
    }

    #[test]
    fn a_launch_gives_up_when_the_lock_holder_never_binds() {
        let mut config = temp_config();
        config.forward_timeout = Duration::from_millis(200);
        std::fs::create_dir_all(&config.dir).unwrap();
        let lock = std::fs::File::create(config.lock_path()).unwrap();
        lock.lock().unwrap();
        let started = Instant::now();
        let error = acquire(&config, &message("x")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() >= config.forward_timeout);
    }

    #[test]
    fn a_launch_waits_for_the_lock_holder_to_bind() {
        let config = temp_config();
        std::fs::create_dir_all(&config.dir).unwrap();
        // Holds the lock, binds 100 ms later: an instance still starting.
        let lock = std::fs::File::create(config.lock_path()).unwrap();
        lock.lock().unwrap();
        let (tx, rx) = mpsc::channel();
        let starting = config.clone();
        let late = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            let mut server = Server::bind(&starting).unwrap();
            server
                .listen(Box::new(move |message| {
                    tx.send(message).unwrap();
                    Ok(())
                }))
                .unwrap();
            (server, lock)
        });
        assert!(matches!(
            acquire(&config, &message("early")).unwrap(),
            Launch::Forwarded
        ));
        assert_eq!(rx.recv().unwrap(), message("early"));
        drop(late.join().unwrap());
    }

    #[test]
    fn a_launch_takes_over_when_the_instance_quits_meanwhile() {
        let config = temp_config();
        let (instance, _rx) = primary(&config);
        let quitting = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(instance);
        });
        // Until the drop, this forwards; after it, it becomes the instance.
        // Either way it doesn't fail.
        let outcome = acquire(&config, &message("x")).unwrap();
        quitting.join().unwrap();
        if let Launch::Primary(_) = outcome {
            assert!(config.socket_path().exists());
        }
    }

    #[test]
    fn a_callback_panic_frees_its_reader_slot() {
        let config = temp_config();
        let Launch::Primary(mut instance) = acquire(&config, &message("a")).unwrap() else {
            panic!("not the primary");
        };
        let (tx, rx) = mpsc::channel();
        instance
            .listen(move |message: Message| {
                assert_ne!(message.args[1], "boom", "test panic");
                tx.send(message).unwrap();
            })
            .unwrap();
        for _ in 0..MAX_READERS + 1 {
            let frame = frame::encode(&message("boom")).unwrap();
            // No answer: the callback panicked.
            assert!(send_raw(&config, &frame).is_err());
        }
        assert!(matches!(
            acquire(&config, &message("after")).unwrap(),
            Launch::Forwarded
        ));
        assert_eq!(rx.try_recv().unwrap(), message("after"));
    }

    #[test]
    fn a_rejected_callback_nacks_and_fails_the_launch() {
        let config = temp_config();
        let Launch::Primary(mut instance) = acquire(&config, &message("a")).unwrap() else {
            panic!("not the primary");
        };
        instance
            .listen_checked(|message| {
                if message.args[1] == "no" {
                    Err(Rejected)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        let error = acquire(&config, &message("no")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(matches!(
            acquire(&config, &message("yes")).unwrap(),
            Launch::Forwarded
        ));
    }

    #[test]
    fn a_launch_before_listening_waits_and_is_answered_after() {
        let config = temp_config();
        let Launch::Primary(mut instance) = acquire(&config, &message("a")).unwrap() else {
            panic!("not the primary");
        };
        let second = {
            let config = config.clone();
            std::thread::spawn(move || acquire(&config, &message("early")))
        };
        std::thread::sleep(Duration::from_millis(100));
        assert!(!second.is_finished(), "answered before anyone listened");
        let (tx, rx) = mpsc::channel();
        instance
            .listen(move |message| tx.send(message).unwrap())
            .unwrap();
        assert!(matches!(second.join().unwrap().unwrap(), Launch::Forwarded));
        assert_eq!(rx.try_recv().unwrap(), message("early"));
    }

    #[test]
    fn an_existing_open_directory_is_made_private() {
        use std::os::unix::fs::PermissionsExt;
        let config = temp_config();
        std::fs::create_dir_all(&config.dir).unwrap();
        std::fs::set_permissions(&config.dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        let Launch::Primary(_instance) = acquire(&config, &message("a")).unwrap() else {
            panic!("not the primary");
        };
        let mode = std::fs::metadata(&config.dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn a_symlinked_directory_is_refused() {
        let config = temp_config();
        let target = config.dir.with_extension("target");
        std::fs::create_dir_all(&target).unwrap();
        std::os::unix::fs::symlink(&target, &config.dir).unwrap();
        let error = acquire(&config, &message("a")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(!target.join("t.lock").exists(), "followed the symlink");
    }

    #[test]
    fn a_directory_that_is_a_file_is_refused() {
        let config = temp_config();
        std::fs::write(&config.dir, b"").unwrap();
        assert!(acquire(&config, &message("a")).is_err());
    }

    #[test]
    fn a_symlinked_lock_file_is_refused_and_its_target_untouched() {
        let config = temp_config();
        std::fs::create_dir_all(&config.dir).unwrap();
        let target = config.dir.join("elsewhere");
        std::fs::write(&target, b"keep").unwrap();
        std::os::unix::fs::symlink(&target, config.lock_path()).unwrap();
        let error = acquire(&config, &message("a")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");
    }
}
