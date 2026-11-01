//! One running instance per user: a second launch hands its working
//! directory and arguments to the running instance and exits.
//!
//! Replaces `tauri-plugin-single-instance` 2.5.5 (M1.7, #82), which on macOS
//! let two near-simultaneous launches both run, let the losing one unlink
//! the winner's socket, and let one silent connection block every later
//! launch's message.
//!
//! How it works:
//!
//! - A lock file ([`Config::lock_path`]) decides who runs: the process that
//!   holds an exclusive lock on it (`File::try_lock`, `flock` on Unix) is
//!   the running instance. The OS releases the lock when the process dies,
//!   so a crash leaves nothing stale to clean up.
//! - Only the lock holder removes a leftover socket and binds a new one, and
//!   it does both before [`acquire`] returns. A bind failure is an error, so
//!   no process runs without a working socket.
//! - Everyone else connects to the socket and sends a [`Message`]. If the
//!   holder hasn't bound yet (or has just quit), they retry until
//!   [`Config::forward_timeout`]; if the lock frees up they take it.
//! - The running instance reads each connection in its own thread with a
//!   deadline and a size limit (`frame::MAX_FRAME`), at most
//!   `MAX_READERS` at a time, and acknowledges a message once its callback
//!   has run. A launch that gets no acknowledgement reports an error
//!   instead of exiting as if its arguments had arrived.
//!
//! The socket and lock live in a per-user directory ([`default_dir`]), not
//! the shared `/tmp` the plugin used.
//!
//! Unix only for now. Elsewhere the lock still keeps a second instance from
//! running, but it can't forward its arguments.
// TODO(M-W.1): forward arguments on Windows (a named pipe).

mod frame;
#[cfg(unix)]
mod unix;

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What a launch hands the running instance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Message {
    /// The launch's working directory (lossy UTF-8).
    pub cwd: String,
    /// Its arguments, the executable first (lossy UTF-8), like
    /// [`std::env::args_os`].
    pub args: Vec<String>,
}

impl Message {
    /// This process's working directory and arguments.
    #[must_use]
    pub fn current() -> Self {
        Self {
            cwd: std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            args: std::env::args_os()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
        }
    }
}

/// Where the lock and socket live, and how long to wait.
#[derive(Debug, Clone)]
pub struct Config {
    /// The directory for the lock and socket; created if missing.
    pub dir: PathBuf,
    /// The stem of the lock and socket file names.
    pub name: String,
    /// How long a second launch keeps trying to reach the running instance
    /// (it may still be starting, or just quitting).
    pub forward_timeout: Duration,
    /// How long the running instance waits for one launch's message, and a
    /// launch for the acknowledgement.
    pub io_timeout: Duration,
}

impl Config {
    /// The defaults for the app with bundle identifier `identifier`: files in
    /// [`default_dir`], a 5 s forward timeout and a 2 s I/O timeout.
    #[must_use]
    pub fn new(identifier: &str) -> Self {
        Self::in_dir(default_dir(identifier), "instance")
    }

    /// Files named `name.lock` and `name.sock` in `dir`, with the default
    /// timeouts.
    #[must_use]
    pub fn in_dir(dir: impl Into<PathBuf>, name: &str) -> Self {
        Self {
            dir: dir.into(),
            name: name.to_owned(),
            forward_timeout: Duration::from_secs(5),
            io_timeout: Duration::from_secs(2),
        }
    }

    /// The lock file.
    #[must_use]
    pub fn lock_path(&self) -> PathBuf {
        self.dir.join(format!("{}.lock", self.name))
    }

    /// The socket.
    #[must_use]
    pub fn socket_path(&self) -> PathBuf {
        self.dir.join(format!("{}.sock", self.name))
    }
}

/// The longest socket path that fits everywhere (`sun_path` is 104 bytes
/// on macOS, 108 on Linux, including the terminating NUL).
const MAX_SOCKET_PATH: usize = 100;

/// A per-user directory for the app's lock and socket.
///
/// macOS: `~/Library/Caches/<identifier>`, which the system doesn't clean
/// up while the app runs (unlike `/tmp` and `$TMPDIR`). Linux:
/// `$XDG_RUNTIME_DIR/<identifier>`. Elsewhere, or if that path would make
/// the socket path too long: the per-user temporary directory.
#[must_use]
pub fn default_dir(identifier: &str) -> PathBuf {
    let preferred = preferred_dir().map(|dir| dir.join(identifier));
    match preferred {
        Some(dir) if dir.as_os_str().len() + "/instance.sock".len() <= MAX_SOCKET_PATH => dir,
        _ => std::env::temp_dir().join(identifier),
    }
}

#[cfg(target_os = "macos")]
fn preferred_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| Path::new(&home).join("Library/Caches"))
}

#[cfg(target_os = "linux")]
fn preferred_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn preferred_dir() -> Option<PathBuf> {
    None
}

/// A message the running instance couldn't act on: the second launch is
/// told, and fails, instead of being reported as handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rejected;

/// What this launch turned out to be.
#[derive(Debug)]
pub enum Launch {
    /// The running instance. Keep it alive for as long as the app runs and
    /// call [`Instance::listen`] to receive later launches' messages.
    Primary(Instance),
    /// Another instance is running and has handled this launch's message;
    /// this process should exit.
    Forwarded,
}

/// Becomes the running instance, or hands `message` to the one that is.
///
/// # Errors
///
/// - The directory or lock file can't be created, or the socket can't be
///   bound (no process should run without one).
/// - The directory exists but isn't a real directory this user owns, or the
///   lock file is a symlink ([`io::ErrorKind::PermissionDenied`]).
/// - `message` is too long to forward ([`io::ErrorKind::InvalidInput`]).
/// - Another instance holds the lock but didn't acknowledge the message
///   within [`Config::forward_timeout`] ([`io::ErrorKind::TimedOut`]), or
///   rejected it.
/// - Another instance runs and this OS can't forward to it
///   ([`io::ErrorKind::Unsupported`]).
pub fn acquire(config: &Config, message: &Message) -> io::Result<Launch> {
    create_private_dir(&config.dir)?;
    let frame = frame::encode(message)?;
    refuse_symlink(&config.lock_path())?;
    let lock = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(config.lock_path())?;
    let deadline = Instant::now() + config.forward_timeout;
    loop {
        match lock.try_lock() {
            Ok(()) => return Instance::start(config, lock).map(Launch::Primary),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => return Err(error),
        }
        let last_error = match forward(config, &frame) {
            Ok(()) => return Ok(Launch::Forwarded),
            Err(Forward::Retry(error)) => error,
            Err(Forward::Fail(error)) => return Err(error),
        };
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("the running instance didn't take this launch's arguments: {last_error}"),
            ));
        }
        std::thread::sleep(RETRY_INTERVAL);
    }
}

/// Creates `dir` (and its parents) if missing; on Unix, one only this user
/// can enter, so other users can't reach the socket.
///
/// On Unix an existing `dir` is held to the same standard, because the
/// directory is the access boundary: it must be a real directory (not a
/// symlink), and it is made 0700 if it was looser, which also proves this
/// user owns it (chmod fails otherwise). Anything else is refused, so a
/// directory someone else prepared is never trusted.
fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    #[cfg(unix)]
    ensure_private(dir)?;
    Ok(())
}

#[cfg(unix)]
fn ensure_private(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let refuse = |why: &str| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} {why}; refusing to use it", dir.display()),
        )
    };
    // Not `metadata`: a symlink is someone else's choice of where we go.
    let meta = std::fs::symlink_metadata(dir)?;
    if !meta.is_dir() {
        return Err(refuse("is not a plain directory (a symlink, say)"));
    }
    if meta.permissions().mode() & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| refuse(&format!("is open to other users and not ours ({error})")))?;
        tracing::debug!(path = %dir.display(), "made the single-instance directory private");
    }
    Ok(())
}

/// Refuses a lock file that is a symlink: where it points is not ours to
/// open. (The directory is private by now, so nobody else can swap it in
/// between this check and the open.)
fn refuse_symlink(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is a symlink; refusing to use it", path.display()),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// How often a second launch retries while the running instance starts up.
const RETRY_INTERVAL: Duration = Duration::from_millis(20);

/// Why a forward attempt failed.
enum Forward {
    /// Worth retrying: no socket yet, or the running instance dropped the
    /// connection unread (it was busy, starting or quitting).
    Retry(io::Error),
    /// Not worth retrying: the message was rejected, or may have been
    /// handled without an acknowledgement.
    Fail(io::Error),
}

/// The running instance: holds the lock and the socket until dropped.
#[derive(Debug)]
pub struct Instance {
    #[cfg(unix)]
    server: unix::Server,
    /// Released last, after the socket is gone, so no one else can bind
    /// while this instance's socket still exists.
    lock: File,
}

impl Drop for Instance {
    fn drop(&mut self) {
        #[cfg(unix)]
        self.server.stop();
        // The lock goes with the file; unlocking first just makes it
        // explicit. Any error leaves it to the close.
        let _ = self.lock.unlock();
    }
}

#[cfg(unix)]
impl Instance {
    fn start(config: &Config, lock: File) -> io::Result<Self> {
        Ok(Self {
            server: unix::Server::bind(config)?,
            lock,
        })
    }

    /// Calls `on_message` with each later launch's message, one at a time,
    /// in a background thread, until this instance is dropped. Every message
    /// is acknowledged once `on_message` returns; use
    /// [`Instance::listen_checked`] when it may not be able to act on one.
    ///
    /// # Errors
    ///
    /// If the thread can't be started, or this instance already listens.
    pub fn listen(
        &mut self,
        mut on_message: impl FnMut(Message) + Send + 'static,
    ) -> io::Result<()> {
        self.listen_checked(move |message| {
            on_message(message);
            Ok(())
        })
    }

    /// Like [`Instance::listen`], but acknowledges a message only if
    /// `on_message` returns `Ok`. On [`Rejected`] the second launch gets a
    /// negative answer and fails with [`io::ErrorKind::InvalidData`], so a
    /// message that wasn't acted on is never reported as handled.
    ///
    /// Launches that arrive before this call wait for the answer (up to
    /// [`Config::io_timeout`]), so start listening only once the app can
    /// act on messages.
    ///
    /// # Errors
    ///
    /// If the thread can't be started, or this instance already listens.
    pub fn listen_checked(
        &mut self,
        on_message: impl FnMut(Message) -> Result<(), Rejected> + Send + 'static,
    ) -> io::Result<()> {
        self.server.listen(Box::new(on_message))
    }
}

#[cfg(unix)]
fn forward(config: &Config, frame: &[u8]) -> Result<(), Forward> {
    unix::forward(config, frame)
}

#[cfg(not(unix))]
impl Instance {
    fn start(_config: &Config, lock: File) -> io::Result<Self> {
        Ok(Self { lock })
    }

    /// Does nothing: this OS can't receive messages yet.
    ///
    /// # Errors
    ///
    /// Never.
    pub fn listen(&mut self, _on_message: impl FnMut(Message) + Send + 'static) -> io::Result<()> {
        Ok(())
    }

    /// Does nothing: this OS can't receive messages yet.
    ///
    /// # Errors
    ///
    /// Never.
    pub fn listen_checked(
        &mut self,
        _on_message: impl FnMut(Message) -> Result<(), Rejected> + Send + 'static,
    ) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(not(unix))]
fn forward(_config: &Config, _frame: &[u8]) -> Result<(), Forward> {
    Err(Forward::Fail(io::Error::new(
        io::ErrorKind::Unsupported,
        "another instance is running, and this OS can't forward arguments to it yet",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_message_has_this_executable() {
        let message = Message::current();
        assert!(!message.args.is_empty());
        assert!(!message.cwd.is_empty());
    }

    #[test]
    fn paths_are_in_the_configured_dir() {
        let config = Config::in_dir("/x", "app");
        assert_eq!(config.lock_path(), Path::new("/x/app.lock"));
        assert_eq!(config.socket_path(), Path::new("/x/app.sock"));
    }

    #[test]
    fn default_dir_ends_in_the_identifier_and_fits_a_socket() {
        let config = Config::new("com.example.app");
        assert!(config.dir.ends_with("com.example.app"));
        let socket = config.socket_path();
        let fits = socket.as_os_str().len() <= MAX_SOCKET_PATH;
        // Only a very long temporary directory could break this.
        assert!(fits || config.dir.starts_with(std::env::temp_dir()));
    }

    #[test]
    fn default_dir_falls_back_for_a_long_identifier() {
        let long = "x".repeat(MAX_SOCKET_PATH);
        assert_eq!(default_dir(&long), std::env::temp_dir().join(&long));
    }
}
