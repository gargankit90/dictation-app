//! T2 (P1-03): the saved hotkey is the one listening, at startup and right
//! after Settings is saved; a hotkey that can't start leaves the old one
//! listening, the file holding the old one, and an error saying why.
//!
//! The real pipeline runs against a [`ConfigurableHotkey`] whose factory makes
//! fakes and records which spec each was built for, so the tests can press
//! exactly the key a spec stands for (its fake) and see a dictation, or not.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test on purpose"
)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use dictation_core::coordinator::Phase::{Idle, Inserting, Recording, Transcribing};
use dictation_core::hotkey_spec::HotkeySpec;
use dictation_core::settings::Settings;
use dictation_pipeline::config::Config as SettingsFile;
use dictation_pipeline::hotkey::PermissionTolerantHotkey;
use dictation_pipeline::hotkey_setting::{
    self, ConfigurableHotkey, HotkeyChange, HotkeyControl, Switched,
};
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{FIXED_TRANSCRIPT, FixedTranscriber, ToneAudio};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{
    FakeClock, FakeHotkey, FakeInserter, FakePermissions, MemSecretStore,
};
use dictation_ports::{HotkeyError, HotkeyEvent, HotkeySink, HotkeySource};

const WAIT: Duration = Duration::from_secs(5);
const QUIET: Duration = Duration::from_millis(200);

fn spec(text: &str) -> HotkeySpec {
    text.parse().unwrap()
}

/// A fresh settings directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dictation-hotkey-settings-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }

    fn settings_file(&self) -> SettingsFile<MemSecretStore> {
        SettingsFile::new(&self.0, MemSecretStore::new())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

type StopHook = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;
type StartHook = Box<dyn FnOnce(&FakeHotkey) + Send>;

/// A source the factory built: a fake, plus fences a test can put into its
/// start (after it listens) and its stop (before it stops listening), like
/// the real listener's thread start and join.
struct Key {
    fake: FakeHotkey,
    after_start: Option<StartHook>,
    on_stop: StopHook,
}

impl HotkeySource for Key {
    fn start(&mut self, sink: HotkeySink) -> Result<(), HotkeyError> {
        self.fake.start(sink)?;
        if let Some(hook) = self.after_start.take() {
            hook(&self.fake);
        }
        Ok(())
    }

    fn stop(&mut self) {
        let hook = self.on_stop.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
        self.fake.stop();
    }
}

/// The hotkey factory: one fake per spec asked for, in order. A start
/// failure or a start fence can be queued for the next source it makes.
#[derive(Clone, Default)]
struct Keyboard {
    built: Arc<Mutex<Vec<(HotkeySpec, FakeHotkey, StopHook)>>>,
    fail_next: Arc<Mutex<Option<HotkeyError>>>,
    after_next_start: Arc<Mutex<Option<StartHook>>>,
}

impl Keyboard {
    fn factory(&self) -> impl Fn(HotkeySpec) -> Key + Send + Sync + 'static {
        let keyboard = self.clone();
        move |spec| {
            let fake = FakeHotkey::new();
            if let Some(error) = keyboard.fail_next.lock().unwrap().take() {
                fake.fail_next_start(error);
            }
            let on_stop = StopHook::default();
            keyboard
                .built
                .lock()
                .unwrap()
                .push((spec, fake.clone(), Arc::clone(&on_stop)));
            Key {
                fake,
                after_start: keyboard.after_next_start.lock().unwrap().take(),
                on_stop,
            }
        }
    }

    fn fail_next_start(&self, error: HotkeyError) {
        *self.fail_next.lock().unwrap() = Some(error);
    }

    /// Runs `hook` on the next source built, right after it starts
    /// listening and before `start` returns.
    fn after_next_start(&self, hook: impl FnOnce(&FakeHotkey) + Send + 'static) {
        *self.after_next_start.lock().unwrap() = Some(Box::new(hook));
    }

    /// Runs `hook` when the source for `spec` is stopped, before it stops
    /// listening: its stop blocks until `hook` returns.
    fn on_stop(&self, spec: HotkeySpec, hook: impl FnOnce() + Send + 'static) {
        let built = self.built.lock().unwrap();
        let (_, _, slot) = built
            .iter()
            .rev()
            .find(|(s, _, _)| *s == spec)
            .unwrap_or_else(|| panic!("no source was built for {spec}"));
        *slot.lock().unwrap() = Some(Box::new(hook));
    }

    fn specs(&self) -> Vec<HotkeySpec> {
        self.built
            .lock()
            .unwrap()
            .iter()
            .map(|(s, _, _)| *s)
            .collect()
    }

    /// The most recent source built for `spec`: the key it stands for.
    fn key(&self, spec: HotkeySpec) -> FakeHotkey {
        self.built
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(s, _, _)| *s == spec)
            .map(|(_, fake, _)| fake.clone())
            .unwrap_or_else(|| panic!("no source was built for {spec}"))
    }

    /// Blocks the stop of the source for `spec` until the returned gate is
    /// opened; `stopping` receives once that stop has begun.
    fn hold_stop(&self, spec: HotkeySpec) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (began, stopping) = mpsc::channel();
        let (gate, gate_rx) = mpsc::channel::<()>();
        self.on_stop(spec, move || {
            began.send(()).unwrap();
            gate_rx
                .recv_timeout(WAIT)
                .expect("the stop was never let go");
        });
        (stopping, gate)
    }
}

struct App {
    keyboard: Keyboard,
    control: HotkeyControl,
    clock: FakeClock,
    inserter: FakeInserter,
    pipeline: Pipeline,
    ui: UiReceiver,
}

impl App {
    /// Starts like the shell: the hotkey from the saved settings.
    fn start(file: &SettingsFile<MemSecretStore>) -> Self {
        Self::start_with(file, Keyboard::default(), |hotkey| hotkey)
    }

    fn start_with<H: HotkeySource + 'static>(
        file: &SettingsFile<MemSecretStore>,
        keyboard: Keyboard,
        wrap: impl FnOnce(ConfigurableHotkey<Key>) -> H,
    ) -> Self {
        let (hotkey, control) = ConfigurableHotkey::new(
            hotkey_setting::startup_spec(file.settings()),
            keyboard.factory(),
        );
        let clock = FakeClock::at(Duration::from_secs(100));
        let inserter = FakeInserter::new();
        let ports = Ports {
            hotkey: wrap(hotkey),
            audio: ToneAudio::default(),
            transcriber: FixedTranscriber,
            inserter: inserter.clone(),
            permissions: FakePermissions::all_granted(),
            clock: clock.clone(),
        };
        let (pipeline, ui) = Pipeline::spawn(ports, Config::default()).unwrap();
        Self {
            keyboard,
            control,
            clock,
            inserter,
            pipeline,
            ui,
        }
    }

    async fn next(&mut self) -> UiEvent {
        tokio::time::timeout(WAIT, self.ui.recv())
            .await
            .expect("no UI event in time")
            .expect("pipeline stopped")
    }

    async fn assert_quiet(&mut self) {
        let next = tokio::time::timeout(QUIET, self.ui.recv()).await;
        assert!(next.is_err(), "unexpected UI event: {next:?}");
    }

    /// Holds the key `spec` stands for and lets go: one whole dictation.
    async fn dictate_with(&mut self, spec: HotkeySpec) {
        let key = self.keyboard.key(spec);
        let before = self.inserter.inserted().len();
        assert!(key.emit(HotkeyEvent::Pressed), "{spec} isn't listening");
        assert_eq!(self.next().await, UiEvent::StateChanged(Recording));
        self.clock.advance(Duration::from_millis(500));
        assert!(key.emit(HotkeyEvent::Released));
        assert_eq!(self.next().await, UiEvent::StateChanged(Transcribing));
        assert_eq!(self.next().await, UiEvent::StateChanged(Inserting));
        assert_eq!(self.next().await, UiEvent::StateChanged(Idle));
        assert_eq!(self.inserter.inserted().len(), before + 1);
        assert_eq!(self.inserter.inserted().last().unwrap(), FIXED_TRANSCRIPT);
    }

    /// The key `spec` stands for is dead: pressing it starts nothing.
    async fn assert_deaf_to(&mut self, spec: HotkeySpec) {
        let key = self.keyboard.key(spec);
        assert!(!key.is_listening(), "{spec} still listens");
        assert!(!key.emit(HotkeyEvent::Pressed));
        self.assert_quiet().await;
    }

    /// What saving Settings does: the login item aside, the shell's
    /// `save_settings` applies the hotkey, then writes the file.
    fn save(&self, file: &SettingsFile<MemSecretStore>, hotkey: &str) -> HotkeyChange {
        let mut settings = file.settings().unwrap();
        settings.hotkey = hotkey.to_owned();
        let change = hotkey_setting::apply(&self.control, &mut settings);
        file.save_settings(&settings).unwrap();
        change
    }
}

fn saved_with_hotkey(dir: &TempDir, hotkey: &str) -> SettingsFile<MemSecretStore> {
    let file = dir.settings_file();
    file.save_settings(&Settings {
        hotkey: hotkey.to_owned(),
        ..Settings::default()
    })
    .unwrap();
    file
}

#[tokio::test(flavor = "multi_thread")]
async fn saved_hotkey_is_used_at_startup() {
    let dir = TempDir::new("startup");
    let file = saved_with_hotkey(&dir, "Ctrl+Shift+Space");
    let mut app = App::start(&file);

    // Only the saved hotkey was ever registered: not the default first.
    assert_eq!(app.keyboard.specs(), [spec("Ctrl+Shift+Space")]);
    assert_eq!(app.control.spec(), spec("Ctrl+Shift+Space"));
    app.dictate_with(spec("Ctrl+Shift+Space")).await;
    app.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_saved_settings_the_default_hotkey_listens() {
    let dir = TempDir::new("no-file");
    let mut app = App::start(&dir.settings_file());
    assert_eq!(app.keyboard.specs(), [HotkeySpec::default()]);
    app.dictate_with(HotkeySpec::default()).await;
    app.pipeline.shutdown().await;
}

#[test]
fn unreadable_settings_start_with_the_default_hotkey() {
    let dir = TempDir::new("malformed");
    let file = dir.settings_file();
    std::fs::create_dir_all(&dir.0).unwrap();
    std::fs::write(file.path(), "{ not json").unwrap();
    assert!(file.settings().is_err());
    assert_eq!(
        hotkey_setting::startup_spec(file.settings()),
        HotkeySpec::default()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn saving_hotkey_reconfigures_listener_right_away() {
    let dir = TempDir::new("switch");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    let old = HotkeySpec::default();

    let change = app.save(&file, "F6");
    assert_eq!(change, HotkeyChange::Switched { previous: old });
    assert_eq!(change.problem(), None, "a working switch needs no warning");
    assert_eq!(app.control.spec(), spec("F6"));
    assert_eq!(file.settings().unwrap().hotkey, "F6");

    // The new key dictates without a restart; the old one is dead.
    app.dictate_with(spec("F6")).await;
    app.assert_deaf_to(old).await;

    // And back again.
    assert_eq!(
        app.save(&file, &old.to_string()),
        HotkeyChange::Switched {
            previous: spec("F6")
        }
    );
    app.dictate_with(old).await;
    app.assert_deaf_to(spec("F6")).await;
    app.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_reregistration_keeps_the_old_hotkey_and_says_why() {
    let dir = TempDir::new("rollback");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    let old = HotkeySpec::default();

    app.keyboard
        .fail_next_start(HotkeyError::Unavailable("tap refused".into()));
    let change = app.save(&file, "Ctrl+Shift+Space");
    assert_eq!(
        change,
        HotkeyChange::Kept {
            kept: old,
            wanted: spec("Ctrl+Shift+Space"),
            error: HotkeyError::Unavailable("tap refused".into()),
        }
    );
    let problem = change.problem().expect("the user is told");
    assert!(problem.contains("Ctrl+Shift+Space"), "{problem}");
    assert!(problem.contains("tap refused"), "{problem}");
    assert!(problem.contains(&format!("{old} still works")), "{problem}");

    // The file, the control and the keyboard all still say the old hotkey.
    assert_eq!(file.settings().unwrap().hotkey, old.to_string());
    assert_eq!(app.control.spec(), old);
    app.dictate_with(old).await;
    let refused = app.keyboard.key(spec("Ctrl+Shift+Space"));
    assert!(!refused.is_listening());

    // Retrying once the problem is gone works.
    assert_eq!(
        app.save(&file, "Ctrl+Shift+Space"),
        HotkeyChange::Switched { previous: old }
    );
    app.dictate_with(spec("Ctrl+Shift+Space")).await;
    app.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_hotkey_is_saved_in_canonical_form() {
    let dir = TempDir::new("canonical");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    assert_eq!(
        app.save(&file, " ctrl+SHIFT+space "),
        HotkeyChange::Switched {
            previous: HotkeySpec::default()
        }
    );
    assert_eq!(file.settings().unwrap().hotkey, "Ctrl+Shift+Space");
    assert!(matches!(
        app.save(&file, "leftalt"),
        HotkeyChange::Switched { .. }
    ));
    assert_eq!(file.settings().unwrap().hotkey, "LeftAlt");
    // The same key written differently is the same hotkey: nothing changes.
    assert_eq!(app.save(&file, "LEFTALT"), HotkeyChange::Unchanged);
    assert_eq!(file.settings().unwrap().hotkey, "LeftAlt");
    app.dictate_with(spec("LeftAlt")).await;
    app.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn saving_the_same_hotkey_reregisters_nothing() {
    let dir = TempDir::new("same");
    let file = saved_with_hotkey(&dir, "F6");
    let mut app = App::start(&file);
    assert_eq!(app.save(&file, "F6"), HotkeyChange::Unchanged);
    assert_eq!(app.keyboard.specs(), [spec("F6")]);
    app.dictate_with(spec("F6")).await;
    app.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_switch_during_a_hold_ends_it_instead_of_leaving_it_stuck() {
    let dir = TempDir::new("mid-hold");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    let old = HotkeySpec::default();
    assert!(app.keyboard.key(old).emit(HotkeyEvent::Pressed));
    assert_eq!(app.next().await, UiEvent::StateChanged(Recording));

    // The old key's release can no longer arrive: the hold is dropped.
    app.save(&file, "F6");
    assert_eq!(app.next().await, UiEvent::StateChanged(Idle));
    app.assert_quiet().await;
    assert!(app.inserter.inserted().is_empty());
    app.dictate_with(spec("F6")).await;
    app.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_keyboard_access_the_hotkey_is_saved_for_the_next_start() {
    let dir = TempDir::new("no-access");
    let file = dir.settings_file();
    let keyboard = Keyboard::default();
    keyboard.fail_next_start(HotkeyError::PermissionDenied);
    // The release build's wrapper: no Accessibility doesn't stop the app.
    let app = App::start_with(&file, keyboard, PermissionTolerantHotkey::new);

    let change = app.save(&file, "F6");
    assert_eq!(
        change,
        HotkeyChange::NotListening {
            wanted: spec("F6"),
            reason: Some(HotkeyError::PermissionDenied),
        }
    );
    let problem = change.problem().expect("not a plain success");
    assert!(problem.contains("Accessibility"), "{problem}");
    assert!(problem.contains("restart"), "{problem}");
    // Saved, so the next start uses it; nothing new was registered now.
    assert_eq!(file.settings().unwrap().hotkey, "F6");
    assert_eq!(app.keyboard.specs(), [HotkeySpec::default()]);
    app.pipeline.shutdown().await;

    // Restarted with access granted: the saved hotkey listens.
    let mut app = App::start(&file);
    assert_eq!(app.keyboard.specs(), [spec("F6")]);
    app.dictate_with(spec("F6")).await;
    app.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_file_write_puts_the_old_hotkey_back() {
    let dir = TempDir::new("undo");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    let old = HotkeySpec::default();
    let mut settings = file.settings().unwrap();
    settings.hotkey = "F6".into();
    let change = hotkey_setting::apply(&app.control, &mut settings);
    assert_eq!(change, HotkeyChange::Switched { previous: old });
    // The shell's save_settings: the write failed, so undo.
    change.undo(&app.control);
    assert_eq!(app.control.spec(), old);
    app.dictate_with(old).await;
    app.assert_deaf_to(spec("F6")).await;
    app.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hotkey_that_died_listens_again_after_a_switch() {
    let dir = TempDir::new("revive");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    let old = HotkeySpec::default();
    assert!(app.keyboard.key(old).emit(HotkeyEvent::Failed));
    assert!(matches!(app.next().await, UiEvent::Error(_)));

    assert_eq!(
        app.control.switch(spec("F6")),
        Ok(Switched::Now),
        "a new hotkey starts even though the old one died"
    );
    app.dictate_with(spec("F6")).await;
    app.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_hotkey_field_touches_nothing() {
    let dir = TempDir::new("invalid");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    let mut settings = file.settings().unwrap();
    settings.hotkey = "Ctrl+Banana".into();
    assert_eq!(
        hotkey_setting::apply(&app.control, &mut settings),
        HotkeyChange::Invalid
    );
    // Saving reports it; the listener is untouched.
    assert!(file.save_settings(&settings).is_err());
    assert_eq!(app.keyboard.specs(), [HotkeySpec::default()]);
    app.dictate_with(HotkeySpec::default()).await;
    app.pipeline.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_the_pipeline_stops_a_switch_only_records_the_spec() {
    let dir = TempDir::new("stopped");
    let file = dir.settings_file();
    let app = App::start(&file);
    let control = app.control.clone();
    let keyboard = app.keyboard.clone();
    app.pipeline.shutdown().await;
    assert!(!keyboard.key(HotkeySpec::default()).is_listening());
    assert_eq!(control.switch(spec("F6")), Ok(Switched::NotListening(None)));
    assert_eq!(control.spec(), spec("F6"));
    assert_eq!(keyboard.specs(), [HotkeySpec::default()]);
}

/// R1 (T2 review round 1): the new key is heard while the old listener is
/// still being stopped and joined. A press then is one dictation, not a lost
/// press followed by a stray release.
#[tokio::test(flavor = "multi_thread")]
async fn a_press_of_the_new_key_while_the_old_one_stops_is_heard_once() {
    let dir = TempDir::new("handoff-press");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    let old = HotkeySpec::default();
    let (stopping, gate) = app.keyboard.hold_stop(old);

    let control = app.control.clone();
    let switching = std::thread::spawn(move || control.switch(spec("F6")));
    stopping
        .recv_timeout(WAIT)
        .expect("the old source was never stopped");

    // The old listener is mid-stop; the user already holds the new key.
    assert!(app.keyboard.key(spec("F6")).emit(HotkeyEvent::Pressed));
    assert_eq!(app.next().await, UiEvent::StateChanged(Recording));

    gate.send(()).unwrap();
    assert_eq!(switching.join().unwrap(), Ok(Switched::Now));
    app.clock.advance(Duration::from_millis(500));
    assert!(app.keyboard.key(spec("F6")).emit(HotkeyEvent::Released));
    assert_eq!(app.next().await, UiEvent::StateChanged(Transcribing));
    assert_eq!(app.next().await, UiEvent::StateChanged(Inserting));
    assert_eq!(app.next().await, UiEvent::StateChanged(Idle));
    app.assert_quiet().await;
    assert_eq!(app.inserter.inserted(), [FIXED_TRANSCRIPT]);
    app.pipeline.shutdown().await;
}

/// The other side of the handoff: once the new key has taken over, an event
/// the old listener sends while it is being stopped never arrives, so the
/// two keys are never heard at once.
#[tokio::test(flavor = "multi_thread")]
async fn the_old_key_is_not_heard_once_the_new_one_has_taken_over() {
    let dir = TempDir::new("handoff-old");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    let old = HotkeySpec::default();
    let (stopping, gate) = app.keyboard.hold_stop(old);

    let control = app.control.clone();
    let switching = std::thread::spawn(move || control.switch(spec("F6")));
    stopping
        .recv_timeout(WAIT)
        .expect("the old source was never stopped");

    // Still listening (its stop hasn't got that far), but no longer current.
    assert!(app.keyboard.key(old).emit(HotkeyEvent::Pressed));
    app.assert_quiet().await;

    gate.send(()).unwrap();
    assert_eq!(switching.join().unwrap(), Ok(Switched::Now));
    assert!(!app.keyboard.key(old).emit(HotkeyEvent::Released));
    app.assert_quiet().await;
    app.dictate_with(spec("F6")).await;
    assert_eq!(app.inserter.inserted(), [FIXED_TRANSCRIPT]);
    app.pipeline.shutdown().await;
}

/// A press of the new key after it starts listening but before it takes
/// over isn't lost either: it starts the dictation at the handoff, so the
/// release that follows ends it.
#[tokio::test(flavor = "multi_thread")]
async fn a_press_of_the_new_key_as_it_starts_is_heard_at_the_handoff() {
    let dir = TempDir::new("handoff-start");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    app.keyboard.after_next_start(|key| {
        assert!(key.emit(HotkeyEvent::Pressed));
    });

    assert_eq!(app.control.switch(spec("F6")), Ok(Switched::Now));
    assert_eq!(app.next().await, UiEvent::StateChanged(Recording));
    app.clock.advance(Duration::from_millis(500));
    assert!(app.keyboard.key(spec("F6")).emit(HotkeyEvent::Released));
    assert_eq!(app.next().await, UiEvent::StateChanged(Transcribing));
    assert_eq!(app.next().await, UiEvent::StateChanged(Inserting));
    assert_eq!(app.next().await, UiEvent::StateChanged(Idle));
    app.assert_quiet().await;
    assert_eq!(app.inserter.inserted(), [FIXED_TRANSCRIPT]);
    app.pipeline.shutdown().await;
}

/// A new key that starts but fails before it takes over: the failure is
/// passed on at the handoff instead of being swallowed, and a press of the
/// old key while the new one was starting still reached the pipeline.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_key_that_fails_as_it_starts_reports_it_at_the_handoff() {
    let dir = TempDir::new("handoff-failed");
    let file = dir.settings_file();
    let mut app = App::start(&file);
    let old = app.keyboard.key(HotkeySpec::default());
    app.keyboard.after_next_start(move |key| {
        assert!(old.emit(HotkeyEvent::Pressed), "the old key still listens");
        assert!(key.emit(HotkeyEvent::Failed));
    });

    assert_eq!(app.control.switch(spec("F6")), Ok(Switched::Now));
    assert_eq!(app.next().await, UiEvent::StateChanged(Recording));
    // The old hold ends at the handoff, then the new key's failure.
    assert_eq!(app.next().await, UiEvent::StateChanged(Idle));
    assert!(matches!(app.next().await, UiEvent::Error(_)));
    assert!(app.inserter.inserted().is_empty());
    app.pipeline.shutdown().await;
}
