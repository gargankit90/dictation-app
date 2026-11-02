//! Test hooks: the `--press` and `--release` command-line flags, which feed
//! synthetic hotkey events into the pipeline so e2e scripts can drive a
//! running app without a keyboard, and `--debug-audio-file <wav>`, which
//! records that file instead of the microphone ([`debug_audio_file`]).
//!
//! [`SyntheticHotkey`] wraps the real [`HotkeySource`] and adds an
//! [`Injector`]; the shell parses each launch's flags with [`hook_events`]
//! (a second launch's arrive through the single-instance socket) and injects
//! them. These are test hooks, not a feature: nothing else sends synthetic
//! keys.
//!
//! `--debug-set-api-key-stdin` and `--debug-delete-api-key` store and remove
//! the Groq API key through the same [`Config`] the Settings window uses
//! ([`KeyCommand`], [`run_key_command`]), so a test makes the app itself the
//! keychain item's owner. An item made by another tool (`security`) isn't
//! trusted by the app, and macOS asks the user before the app may read it.

use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dictation_core::error::DictationError;
use dictation_core::settings::Provider;
use dictation_ports::{
    HotkeyError, HotkeyEvent, HotkeySink, HotkeySource, Permission, SecretStore,
};

use crate::config::{Config, MAX_KEY_LEN};

/// The flag that injects [`HotkeyEvent::Pressed`].
pub const PRESS_FLAG: &str = "--press";
/// The flag that injects [`HotkeyEvent::Released`].
pub const RELEASE_FLAG: &str = "--release";
/// `--debug-audio-file <wav>` (or `=<wav>`): record that WAV instead of the
/// microphone.
pub const DEBUG_AUDIO_FILE_FLAG: &str = "--debug-audio-file";
/// `--debug-save-audio`: save every recording as a WAV (debug mode).
pub const DEBUG_SAVE_AUDIO_FLAG: &str = "--debug-save-audio";

/// `--debug-set-api-key-stdin`: store the Groq API key read from stdin (never
/// an argument, which any local process can read) and exit.
pub const DEBUG_SET_API_KEY_FLAG: &str = "--debug-set-api-key-stdin";
/// `--debug-delete-api-key`: remove the Groq API key and exit.
pub const DEBUG_DELETE_API_KEY_FLAG: &str = "--debug-delete-api-key";

/// `--debug-launch-at-login-on`, `-off` and `-status`: set or print the login
/// item through the same code as the Settings window, then exit (M5.3).
pub const DEBUG_LAUNCH_ON_FLAG: &str = "--debug-launch-at-login-on";
/// See [`DEBUG_LAUNCH_ON_FLAG`].
pub const DEBUG_LAUNCH_OFF_FLAG: &str = "--debug-launch-at-login-off";
/// See [`DEBUG_LAUNCH_ON_FLAG`].
pub const DEBUG_LAUNCH_STATUS_FLAG: &str = "--debug-launch-at-login-status";

/// What a launch-at-login hook asks for. The process does it and exits
/// instead of starting the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchCommand {
    /// Turn it on.
    On,
    /// Turn it off.
    Off,
    /// Print what the OS says.
    Status,
}

impl LaunchCommand {
    /// The hook `flag` is, if it is one.
    #[must_use]
    pub fn from_flag(flag: &str) -> Option<Self> {
        match flag {
            DEBUG_LAUNCH_ON_FLAG => Some(Self::On),
            DEBUG_LAUNCH_OFF_FLAG => Some(Self::Off),
            DEBUG_LAUNCH_STATUS_FLAG => Some(Self::Status),
            _ => None,
        }
    }
}

/// Does `command` through `login` the way saving the Settings window would
/// ([`crate::launch::apply`]) and says what the OS reports afterwards:
/// `launch at login: enabled`, `disabled` or `requires approval`.
///
/// # Errors
///
/// The reason (for the user) if the OS can't be read or refuses.
pub fn run_launch_command(
    command: LaunchCommand,
    login: &dyn dictation_ports::LaunchAtLogin,
) -> Result<String, String> {
    use dictation_ports::LaunchAtLoginState;

    let wanted = match command {
        LaunchCommand::On => Some(true),
        LaunchCommand::Off => Some(false),
        LaunchCommand::Status => None,
    };
    if let Some(wanted) = wanted {
        let mut settings = dictation_core::settings::Settings {
            launch_at_login: wanted,
            ..dictation_core::settings::Settings::default()
        };
        crate::launch::apply(login, &mut settings).map_err(|problem| problem.to_string())?;
    }
    let state = login.state().map_err(|error| error.to_string())?;
    Ok(format!(
        "launch at login: {}",
        match state {
            LaunchAtLoginState::Enabled => "enabled",
            LaunchAtLoginState::Disabled => "disabled",
            LaunchAtLoginState::RequiresApproval => "requires approval",
        }
    ))
}

/// What a key hook asks for. The process does it and exits instead of
/// starting the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCommand {
    /// `--debug-set-api-key-stdin`.
    Set,
    /// `--debug-delete-api-key`.
    Delete,
}

impl KeyCommand {
    /// The key hook `flag` is, if it is one.
    #[must_use]
    pub fn from_flag(flag: &str) -> Option<Self> {
        match flag {
            DEBUG_SET_API_KEY_FLAG => Some(Self::Set),
            DEBUG_DELETE_API_KEY_FLAG => Some(Self::Delete),
            _ => None,
        }
    }
}

/// Does `command` for [`Provider::Groq`] through `config`, the way the
/// Settings window would: `Set` reads the key from `stdin` and stores it
/// ([`Config::set_api_key`], which trims and validates it), `Delete` removes
/// it. Returns what to tell the user, which never holds the key.
///
/// # Errors
///
/// A message (without the key) if stdin can't be read or isn't a valid key,
/// or the keychain refuses.
pub fn run_key_command<S: SecretStore>(
    command: KeyCommand,
    config: &Config<S>,
    mut stdin: impl Read,
) -> Result<&'static str, String> {
    match command {
        KeyCommand::Set => {
            // One byte over the longest key, so too long is reported as such.
            let mut input = Vec::new();
            stdin
                .by_ref()
                .take(MAX_KEY_LEN as u64 * 4 + 1)
                .read_to_end(&mut input)
                .map_err(|error| format!("could not read the key from stdin: {}", error.kind()))?;
            let key = String::from_utf8(input)
                .map_err(|_| "the key on stdin is not valid UTF-8".to_owned())?;
            config
                .set_api_key(Provider::Groq, &key)
                .map(|()| "API key stored")
                .map_err(|error| error.to_string())
        }
        KeyCommand::Delete => config
            .delete_api_key(Provider::Groq)
            .map(|()| "API key deleted")
            .map_err(|error| error.to_string()),
    }
}

/// The WAV `--debug-audio-file` names in `args`, if any; the last one wins.
/// A flag without a value names nothing.
#[must_use]
pub fn debug_audio_file<I, S>(args: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut path = None;
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        if arg == DEBUG_AUDIO_FILE_FLAG {
            // A flag after it is not its path (#73).
            if let Some(value) = args.next_if(|next| !next.as_ref().starts_with("--")) {
                path = Some(PathBuf::from(value.as_ref()));
            }
        } else if let Some(value) = arg
            .strip_prefix(DEBUG_AUDIO_FILE_FLAG)
            .and_then(|rest| rest.strip_prefix('='))
            .filter(|value| !value.is_empty())
        {
            path = Some(PathBuf::from(value));
        }
    }
    path
}

/// The hotkey events `args` ask for, in order. Every other argument (the
/// program path, macOS's `-psn_…`, anything unknown) is ignored, so a launch
/// without hooks yields nothing.
#[must_use]
pub fn hook_events<I, S>(args: I) -> Vec<HotkeyEvent>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .filter_map(|arg| match arg.as_ref() {
            PRESS_FLAG => Some(HotkeyEvent::Pressed),
            RELEASE_FLAG => Some(HotkeyEvent::Released),
            _ => None,
        })
        .collect()
}

type SharedSink = Arc<Mutex<Option<HotkeySink>>>;

fn lock(sink: &SharedSink) -> MutexGuard<'_, Option<HotkeySink>> {
    sink.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A [`HotkeySource`] that sends both the real source's events and the ones
/// an [`Injector`] injects to the same sink, one at a time.
///
/// If the real source can't start (no Accessibility, no adapter for this
/// OS), `start` still succeeds so the hooks keep working; the error is
/// logged and kept in [`real_error`](Self::real_error).
pub struct SyntheticHotkey<H> {
    real: H,
    sink: SharedSink,
    real_error: Option<HotkeyError>,
}

/// Injects synthetic events into a [`SyntheticHotkey`]. Cheap to clone;
/// callable from any thread.
#[derive(Clone)]
pub struct Injector {
    sink: SharedSink,
    outage: Option<HotkeyOutage>,
}

impl<H: HotkeySource> SyntheticHotkey<H> {
    /// Wraps `real`; returns the source and its injector.
    pub fn new(real: H) -> (Self, Injector) {
        let sink = SharedSink::default();
        let injector = Injector {
            sink: Arc::clone(&sink),
            outage: None,
        };
        (
            Self {
                real,
                sink,
                real_error: None,
            },
            injector,
        )
    }

    /// Why the real source didn't start at the last
    /// [`start`](HotkeySource::start), if it didn't.
    #[must_use]
    pub fn real_error(&self) -> Option<&HotkeyError> {
        self.real_error.as_ref()
    }
}

impl<H: HotkeySource> HotkeySource for SyntheticHotkey<H> {
    fn start(&mut self, sink: HotkeySink) -> Result<(), HotkeyError> {
        {
            let mut shared = lock(&self.sink);
            if shared.is_some() {
                return Err(HotkeyError::AlreadyStarted);
            }
            *shared = Some(sink);
        }
        let forward = Arc::clone(&self.sink);
        self.real_error = self
            .real
            .start(Box::new(move |event| {
                if let Some(sink) = lock(&forward).as_mut() {
                    sink(event);
                }
            }))
            .err();
        match &self.real_error {
            None => {}
            Some(HotkeyError::PermissionDenied) => tracing::warn!(
                error = %DictationError::PermissionMissing(Permission::Accessibility),
                "PermissionMissing: the hotkey is off; only the --press/--release test hooks work"
            ),
            Some(error) => tracing::warn!(
                %error,
                "the hotkey is off; only the --press/--release test hooks work"
            ),
        }
        Ok(())
    }

    fn stop(&mut self) {
        // The real source first, without holding the lock: its stop may wait
        // for a sink call that needs it.
        self.real.stop();
        *lock(&self.sink) = None;
    }
}

impl Injector {
    /// This injector, also able to start and end `outage`.
    #[must_use]
    pub fn with_outage(mut self, outage: HotkeyOutage) -> Self {
        self.outage = Some(outage);
        self
    }

    /// Starts or ends the hotkey outage. Returns whether this injector has
    /// one ([`with_outage`](Self::with_outage)).
    pub fn outage(&self, command: OutageCommand) -> bool {
        match &self.outage {
            Some(outage) => {
                outage.run(command);
                true
            }
            None => false,
        }
    }

    /// Sends `event` to the pipeline. Returns whether it was listening; an
    /// event while stopped is lost, like a real key press.
    pub fn inject(&self, event: HotkeyEvent) -> bool {
        match lock(&self.sink).as_mut() {
            Some(sink) => {
                sink(event);
                true
            }
            None => false,
        }
    }
}

/// `--debug-hotkey-outage`: the real hotkey dies, as when Accessibility is
/// turned off, and can't start again until `--debug-hotkey-recover` (debug
/// builds; for the e2e scenario `e2e_hotkey_listener_recovers`, #88).
pub const DEBUG_HOTKEY_OUTAGE_FLAG: &str = "--debug-hotkey-outage";
/// `--debug-hotkey-recover`: the real hotkey may start again; the app's
/// supervisor brings it back by itself.
pub const DEBUG_HOTKEY_RECOVER_FLAG: &str = "--debug-hotkey-recover";

/// What an outage hook asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutageCommand {
    /// [`DEBUG_HOTKEY_OUTAGE_FLAG`].
    Begin,
    /// [`DEBUG_HOTKEY_RECOVER_FLAG`].
    End,
}

impl OutageCommand {
    /// The outage hook `flag` is, if it is one.
    #[must_use]
    pub fn from_flag(flag: &str) -> Option<Self> {
        match flag {
            DEBUG_HOTKEY_OUTAGE_FLAG => Some(Self::Begin),
            DEBUG_HOTKEY_RECOVER_FLAG => Some(Self::End),
            _ => None,
        }
    }
}

/// Makes real hotkey sources fail on demand, the way revoking Accessibility
/// does, without touching the OS (a VM can't re-grant it without a person
/// typing a password). Wrap each source the hotkey factory builds with
/// [`wrap`](Self::wrap); [`begin`](Self::begin) then ends every wrapped
/// source that listens with [`HotkeyEvent::Failed`] and makes every start
/// fail with [`HotkeyError::PermissionDenied`] until [`end`](Self::end).
/// Cheap to clone; every clone is the same outage.
#[derive(Clone, Default)]
pub struct HotkeyOutage {
    shared: Arc<Mutex<Outage>>,
}

#[derive(Default)]
struct Outage {
    on: bool,
    /// The sinks of the wrapped sources started so far; those still alive
    /// listen.
    sinks: Vec<std::sync::Weak<Mutex<Option<HotkeySink>>>>,
}

impl HotkeyOutage {
    fn lock(&self) -> MutexGuard<'_, Outage> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `inner`, failing while this outage lasts.
    pub fn wrap<H: HotkeySource>(&self, inner: H) -> Breakable<H> {
        Breakable {
            inner,
            outage: self.clone(),
            sink: SharedSink::default(),
        }
    }

    /// Starts the outage: every wrapped source that listens is cut off from
    /// its sink, which gets [`HotkeyEvent::Failed`] (its real listener keeps
    /// running, unheard, until it is stopped), and new ones can't start.
    /// Returns how many were listening.
    pub fn begin(&self) -> usize {
        let live: Vec<_> = {
            let mut outage = self.lock();
            outage.on = true;
            std::mem::take(&mut outage.sinks)
                .iter()
                .filter_map(std::sync::Weak::upgrade)
                .collect()
        };
        let mut failed = 0;
        for slot in live {
            // Taken out, then called and dropped without the lock.
            let sink = lock(&slot).take();
            if let Some(mut sink) = sink {
                sink(HotkeyEvent::Failed);
                failed += 1;
            }
        }
        tracing::info!(failed, "test hook: hotkey outage begins");
        failed
    }

    /// Ends the outage: wrapped sources start again.
    pub fn end(&self) {
        self.lock().on = false;
        tracing::info!("test hook: hotkey outage ends");
    }

    /// Does `command`.
    pub fn run(&self, command: OutageCommand) {
        match command {
            OutageCommand::Begin => {
                self.begin();
            }
            OutageCommand::End => self.end(),
        }
    }
}

/// A [`HotkeySource`] that a [`HotkeyOutage`] can break.
pub struct Breakable<H> {
    inner: H,
    outage: HotkeyOutage,
    sink: SharedSink,
}

impl<H: HotkeySource> HotkeySource for Breakable<H> {
    fn start(&mut self, sink: HotkeySink) -> Result<(), HotkeyError> {
        {
            let mut outage = self.outage.lock();
            if outage.on {
                return Err(HotkeyError::PermissionDenied);
            }
            *lock(&self.sink) = Some(sink);
            outage.sinks.retain(|sink| sink.strong_count() > 0);
            outage.sinks.push(Arc::downgrade(&self.sink));
        }
        let forward = Arc::clone(&self.sink);
        let started = self.inner.start(Box::new(move |event| {
            if let Some(sink) = lock(&forward).as_mut() {
                sink(event);
            }
        }));
        if started.is_err() {
            *lock(&self.sink) = None;
        }
        started
    }

    fn stop(&mut self) {
        // The real source first, without the lock: its stop may wait for a
        // sink call that needs it.
        self.inner.stop();
        let sink = lock(&self.sink).take();
        drop(sink);
    }
}

#[cfg(test)]
mod outage_tests {
    use std::sync::mpsc;

    use HotkeyEvent::{Failed, Pressed};
    use dictation_platform::fakes::FakeHotkey;

    use super::*;

    fn channel_sink() -> (HotkeySink, mpsc::Receiver<HotkeyEvent>) {
        let (tx, rx) = mpsc::channel();
        (
            Box::new(move |event| {
                let _ = tx.send(event);
            }),
            rx,
        )
    }

    #[test]
    fn the_flags_name_the_commands() {
        assert_eq!(
            OutageCommand::from_flag(DEBUG_HOTKEY_OUTAGE_FLAG),
            Some(OutageCommand::Begin)
        );
        assert_eq!(
            OutageCommand::from_flag(DEBUG_HOTKEY_RECOVER_FLAG),
            Some(OutageCommand::End)
        );
        assert_eq!(OutageCommand::from_flag("--press"), None);
    }

    #[test]
    fn an_outage_kills_the_listening_source_and_refuses_new_ones() {
        let outage = HotkeyOutage::default();
        let real = FakeHotkey::new();
        let mut source = outage.wrap(real.clone());
        let (sink, events) = channel_sink();
        source.start(sink).unwrap();
        assert!(real.emit(Pressed));

        outage.run(OutageCommand::Begin);
        // The real source still runs but is no longer heard.
        assert!(real.emit(Pressed));
        assert_eq!(events.try_iter().collect::<Vec<_>>(), [Pressed, Failed]);
        assert_eq!(outage.begin(), 0, "nothing listens any more");

        let mut next = outage.wrap(FakeHotkey::new());
        let (sink, _) = channel_sink();
        assert_eq!(next.start(sink), Err(HotkeyError::PermissionDenied));
        source.stop();
        assert!(!real.is_listening());

        outage.run(OutageCommand::End);
        let again = FakeHotkey::new();
        let mut next = outage.wrap(again.clone());
        let (sink, events) = channel_sink();
        next.start(sink).unwrap();
        assert!(again.emit(Pressed));
        assert_eq!(events.try_recv(), Ok(Pressed));
        next.stop();
    }

    #[test]
    fn a_source_that_cannot_start_is_not_broken_later() {
        let outage = HotkeyOutage::default();
        let real = FakeHotkey::new();
        real.fail_next_start(HotkeyError::Unavailable("busy".into()));
        let mut source = outage.wrap(real);
        let (sink, events) = channel_sink();
        assert!(source.start(sink).is_err());
        assert_eq!(outage.begin(), 0);
        assert!(events.try_recv().is_err());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use HotkeyEvent::{CancelPressed, Pressed, Released};
    use dictation_platform::fakes::{FakeHotkey, MemSecretStore};

    use super::*;

    fn channel_sink() -> (HotkeySink, mpsc::Receiver<HotkeyEvent>) {
        let (tx, rx) = mpsc::channel();
        (
            Box::new(move |event| {
                let _ = tx.send(event);
            }),
            rx,
        )
    }

    #[test]
    fn the_launch_at_login_hooks_set_and_report_the_os_state() {
        use dictation_platform::fakes::FakeLaunchAtLogin;
        use dictation_ports::{LaunchAtLoginError, LaunchAtLoginState};

        let login = FakeLaunchAtLogin::new();
        let run = |command| run_launch_command(command, &login);
        assert_eq!(
            run(LaunchCommand::Status).unwrap(),
            "launch at login: disabled"
        );
        assert_eq!(run(LaunchCommand::On).unwrap(), "launch at login: enabled");
        assert_eq!(
            run(LaunchCommand::Status).unwrap(),
            "launch at login: enabled"
        );
        assert_eq!(
            run(LaunchCommand::Off).unwrap(),
            "launch at login: disabled"
        );

        login.enabling_gives(LaunchAtLoginState::RequiresApproval);
        assert!(run(LaunchCommand::On).unwrap_err().contains("approval"));
        assert_eq!(
            run(LaunchCommand::Status).unwrap(),
            "launch at login: requires approval"
        );
        login.fail_with(LaunchAtLoginError::Unsupported);
        assert!(run(LaunchCommand::Status).is_err());
        for (flag, command) in [
            (DEBUG_LAUNCH_ON_FLAG, LaunchCommand::On),
            (DEBUG_LAUNCH_OFF_FLAG, LaunchCommand::Off),
            (DEBUG_LAUNCH_STATUS_FLAG, LaunchCommand::Status),
        ] {
            assert_eq!(LaunchCommand::from_flag(flag), Some(command));
        }
        assert_eq!(LaunchCommand::from_flag("--press"), None);
    }

    #[test]
    fn parses_hook_flags_in_order_and_ignores_the_rest() {
        assert_eq!(
            hook_events([
                "/Applications/dictation-app.app/Contents/MacOS/dictation-app",
                "--press",
                "-psn_0_12345",
                "--verbose",
                "--release",
                "--press",
            ]),
            [Pressed, Released, Pressed]
        );
        assert!(hook_events(["dictation-app"]).is_empty());
        assert!(hook_events(Vec::<String>::new()).is_empty());
        // Exact match only.
        assert!(hook_events(["--PRESS", "--press=1", "press", "-press"]).is_empty());
    }

    #[test]
    fn finds_the_debug_audio_file_in_either_form() {
        assert_eq!(
            debug_audio_file(["app", "--debug-audio-file", "/tmp/a.wav", "--press"]),
            Some(PathBuf::from("/tmp/a.wav"))
        );
        assert_eq!(
            debug_audio_file(["app", "--debug-audio-file=/tmp/b c.wav"]),
            Some(PathBuf::from("/tmp/b c.wav"))
        );
        // The last one wins.
        assert_eq!(
            debug_audio_file([
                "--debug-audio-file=one.wav",
                "--debug-audio-file",
                "two.wav"
            ]),
            Some(PathBuf::from("two.wav"))
        );
        assert_eq!(debug_audio_file(["app", "--press"]), None);
        assert_eq!(debug_audio_file(["app", "--debug-audio-file"]), None);
        assert_eq!(
            debug_audio_file(["app", "--debug-audio-file", "--press"]),
            None,
            "a flag is not a path (#73)"
        );
        assert_eq!(debug_audio_file(["app", "--debug-audio-file="]), None);
        assert_eq!(debug_audio_file(["--debug-audio-files=x.wav"]), None);
    }

    #[test]
    fn real_and_injected_events_reach_the_same_sink() {
        let real = FakeHotkey::new();
        let (mut hotkey, injector) = SyntheticHotkey::new(real.clone());
        assert!(!injector.inject(Pressed), "lost before start");

        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert!(hotkey.real_error().is_none());
        assert!(injector.inject(Pressed));
        assert!(real.emit(Released));
        assert!(real.emit(CancelPressed));
        assert!(injector.inject(Released));

        hotkey.stop();
        assert!(!real.is_listening());
        assert!(!injector.inject(Pressed), "lost after stop");
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            [Pressed, Released, CancelPressed, Released]
        );
    }

    #[test]
    fn hooks_work_when_the_real_source_cannot_start() {
        let real = FakeHotkey::new();
        real.fail_next_start(HotkeyError::PermissionDenied);
        let (mut hotkey, injector) = SyntheticHotkey::new(real);
        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert_eq!(hotkey.real_error(), Some(&HotkeyError::PermissionDenied));
        assert!(injector.inject(Pressed));
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Pressed]);
    }

    #[test]
    fn real_scripted_events_on_start_are_forwarded() {
        let (mut hotkey, _injector) = SyntheticHotkey::new(FakeHotkey::scripted([Pressed]));
        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Pressed]);
    }

    #[test]
    fn start_twice_is_already_started_and_restart_works() {
        let (mut hotkey, injector) = SyntheticHotkey::new(FakeHotkey::new());
        hotkey.start(channel_sink().0).unwrap();
        assert_eq!(
            hotkey.start(channel_sink().0),
            Err(HotkeyError::AlreadyStarted)
        );
        hotkey.stop();
        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        assert!(injector.inject(Released));
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Released]);
    }

    #[test]
    fn injector_works_from_another_thread() {
        let (mut hotkey, injector) = SyntheticHotkey::new(FakeHotkey::new());
        let (sink, rx) = channel_sink();
        hotkey.start(sink).unwrap();
        std::thread::spawn(move || injector.inject(Pressed))
            .join()
            .unwrap();
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [Pressed]);
    }

    const DUMMY_KEY: &str = "gsk_dummy_not_a_real_key_0123456789";

    fn key_config() -> (Config<MemSecretStore>, MemSecretStore) {
        let secrets = MemSecretStore::new();
        (Config::new(std::env::temp_dir(), secrets.clone()), secrets)
    }

    fn stored(secrets: &MemSecretStore) -> Option<String> {
        secrets
            .get("groq-api-key")
            .unwrap()
            .map(|secret| secret.expose().to_owned())
    }

    #[test]
    fn key_flags_map_to_their_commands() {
        assert_eq!(
            KeyCommand::from_flag(DEBUG_SET_API_KEY_FLAG),
            Some(KeyCommand::Set)
        );
        assert_eq!(
            KeyCommand::from_flag(DEBUG_DELETE_API_KEY_FLAG),
            Some(KeyCommand::Delete)
        );
        assert_eq!(KeyCommand::from_flag("--debug-set-api-key"), None);
        assert_eq!(KeyCommand::from_flag(PRESS_FLAG), None);
    }

    #[test]
    fn set_stores_the_key_from_stdin_trimmed() {
        let (config, secrets) = key_config();
        let message = run_key_command(
            KeyCommand::Set,
            &config,
            format!("{DUMMY_KEY}\n").as_bytes(),
        );
        assert_eq!(message, Ok("API key stored"));
        assert_eq!(stored(&secrets).as_deref(), Some(DUMMY_KEY));
    }

    #[test]
    fn set_replaces_an_earlier_key() {
        let (config, secrets) = key_config();
        run_key_command(KeyCommand::Set, &config, "gsk_first".as_bytes()).unwrap();
        run_key_command(KeyCommand::Set, &config, DUMMY_KEY.as_bytes()).unwrap();
        assert_eq!(stored(&secrets).as_deref(), Some(DUMMY_KEY));
    }

    #[test]
    fn delete_removes_the_key_and_is_fine_without_one() {
        let (config, secrets) = key_config();
        run_key_command(KeyCommand::Set, &config, DUMMY_KEY.as_bytes()).unwrap();
        assert_eq!(
            run_key_command(KeyCommand::Delete, &config, std::io::empty()),
            Ok("API key deleted")
        );
        assert_eq!(stored(&secrets), None);
        assert_eq!(
            run_key_command(KeyCommand::Delete, &config, std::io::empty()),
            Ok("API key deleted")
        );
    }

    #[test]
    fn set_rejects_a_bad_key_and_stores_nothing() {
        let (config, secrets) = key_config();
        let long = "k".repeat(MAX_KEY_LEN + 1);
        let cases: [&[u8]; 5] = [
            b"",
            b"  \n",
            b"two words",
            &[0xff, 0xfe, b'a'],
            long.as_bytes(),
        ];
        for input in cases {
            let error = run_key_command(KeyCommand::Set, &config, input).unwrap_err();
            assert!(!error.is_empty());
            assert_eq!(stored(&secrets), None, "{error}");
        }
    }

    #[test]
    fn a_failing_keychain_is_reported_without_the_key() {
        let (config, secrets) = key_config();
        secrets.set_access_denied(true);
        let error = run_key_command(KeyCommand::Set, &config, DUMMY_KEY.as_bytes()).unwrap_err();
        assert!(error.contains("denied"), "{error}");
        assert!(!error.contains(DUMMY_KEY), "{error}");
        let error = run_key_command(KeyCommand::Delete, &config, std::io::empty()).unwrap_err();
        assert!(error.contains("denied"), "{error}");
    }

    #[test]
    fn an_unreadable_stdin_is_reported_without_the_input() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("secret details"))
            }
        }
        let (config, _) = key_config();
        let error = run_key_command(KeyCommand::Set, &config, Broken).unwrap_err();
        assert!(error.contains("could not read"), "{error}");
        assert!(!error.contains("secret details"), "{error}");
    }

    #[test]
    fn only_as_much_stdin_as_a_key_could_need_is_read() {
        let (config, secrets) = key_config();
        let huge = "k".repeat(100_000);
        let error = run_key_command(KeyCommand::Set, &config, huge.as_bytes()).unwrap_err();
        assert!(error.contains("too long"), "{error}");
        assert_eq!(stored(&secrets), None);
    }
}
