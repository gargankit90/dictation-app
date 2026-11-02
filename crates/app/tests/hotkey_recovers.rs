//! M5.5b (#88): the hotkey comes back by itself. The real pipeline runs
//! against a [`ConfigurableHotkey`] whose factory makes fakes and records
//! when each was built and for which spec; a switch turns "keyboard access"
//! off and on, as Accessibility would.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test on purpose"
)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dictation_core::coordinator::Phase::{Idle, Inserting, Recording, Transcribing};
use dictation_core::error::DictationError;
use dictation_core::hotkey_spec::HotkeySpec;
use dictation_core::retry::RetryPolicy;
use dictation_core::settings::Settings;
use dictation_pipeline::hotkey_setting::{
    self, ConfigurableHotkey, HotkeyChange, HotkeyControl, RESTART_POLICY,
};
use dictation_pipeline::pipeline::{Config, Pipeline, Ports, UiEvent};
use dictation_pipeline::stand_ins::{FIXED_TRANSCRIPT, FixedTranscriber, ToneAudio};
use dictation_pipeline::ui_events::UiReceiver;
use dictation_platform::fakes::{FakeClock, FakeHotkey, FakeInserter, FakePermissions};
use dictation_ports::{HotkeyError, HotkeyEvent};

const WAIT: Duration = Duration::from_secs(5);
const QUIET: Duration = Duration::from_millis(200);

/// Short waits, so the tests can watch several tries: 40, 80, then 160 ms.
const FAST: RetryPolicy = RetryPolicy {
    base_delay: Duration::from_millis(40),
    max_delay: Duration::from_millis(160),
    ..RESTART_POLICY
};

fn spec(text: &str) -> HotkeySpec {
    text.parse().unwrap()
}

/// The hotkey factory: one fake per source asked for, with when and for
/// which spec it was built. While access is denied, each fails to start.
#[derive(Clone, Default)]
struct Keyboard {
    built: Arc<Mutex<Vec<(HotkeySpec, FakeHotkey, Instant)>>>,
    denied: Arc<AtomicBool>,
}

impl Keyboard {
    fn factory(&self) -> impl Fn(HotkeySpec) -> FakeHotkey + Send + Sync + 'static {
        let keyboard = self.clone();
        move |spec| {
            let fake = FakeHotkey::new();
            if keyboard.denied.load(Ordering::SeqCst) {
                fake.fail_next_start(HotkeyError::PermissionDenied);
            }
            keyboard
                .built
                .lock()
                .unwrap()
                .push((spec, fake.clone(), Instant::now()));
            fake
        }
    }

    fn deny_access(&self, denied: bool) {
        self.denied.store(denied, Ordering::SeqCst);
    }

    fn built(&self) -> usize {
        self.built.lock().unwrap().len()
    }

    /// When each source was built, in order.
    fn times(&self) -> Vec<Instant> {
        self.built.lock().unwrap().iter().map(|b| b.2).collect()
    }

    /// How many sources listen now.
    fn listening(&self) -> usize {
        self.built
            .lock()
            .unwrap()
            .iter()
            .filter(|b| b.1.is_listening())
            .count()
    }

    /// The source that listens now, and its spec; there must be one.
    fn current(&self) -> (HotkeySpec, FakeHotkey) {
        let built = self.built.lock().unwrap();
        let mut live = built.iter().filter(|b| b.1.is_listening());
        let (spec, fake, _) = live.next().expect("no source listens");
        assert!(live.next().is_none(), "a second source listens");
        (*spec, fake.clone())
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
    fn start(keyboard: Keyboard, policy: RetryPolicy) -> Self {
        let (hotkey, control) = ConfigurableHotkey::with_restart_policy(
            HotkeySpec::default(),
            keyboard.factory(),
            policy,
        );
        let clock = FakeClock::at(Duration::from_secs(100));
        let inserter = FakeInserter::new();
        let ports = Ports {
            hotkey,
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

    /// The hotkey's listener dies, as when Accessibility is turned off.
    async fn kill_hotkey(&mut self) {
        let (_, fake) = self.keyboard.current();
        assert!(fake.emit(HotkeyEvent::Failed));
        assert_eq!(
            self.next().await,
            UiEvent::Error(DictationError::HotkeyFailed)
        );
    }

    /// Holds the hotkey that listens now and lets go: one whole dictation.
    async fn dictate(&mut self) -> HotkeySpec {
        let (spec, key) = self.keyboard.current();
        let before = self.inserter.inserted().len();
        assert!(key.emit(HotkeyEvent::Pressed));
        assert_eq!(self.next().await, UiEvent::StateChanged(Recording));
        self.clock.advance(Duration::from_millis(500));
        assert!(key.emit(HotkeyEvent::Released));
        assert_eq!(self.next().await, UiEvent::StateChanged(Transcribing));
        assert_eq!(self.next().await, UiEvent::StateChanged(Inserting));
        assert_eq!(self.next().await, UiEvent::StateChanged(Idle));
        assert_eq!(self.inserter.inserted().len(), before + 1);
        assert_eq!(self.inserter.inserted().last().unwrap(), FIXED_TRANSCRIPT);
        spec
    }
}

/// Waits (polling, without blocking the runtime) until `done`.
async fn wait_for(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Criterion 1: a dead listener shows "hotkey unavailable" (the
/// `HotkeyFailed` error), the supervisor tries again and again, each wait
/// longer, until access is back; then the hotkey listens, the UI hears it is
/// restored, and the next press dictates.
#[tokio::test(flavor = "multi_thread")]
async fn criterion_1_a_dead_hotkey_backs_off_then_comes_back_and_dictates() {
    let mut app = App::start(Keyboard::default(), FAST);
    app.dictate().await;
    app.keyboard.deny_access(true);
    app.kill_hotkey().await;

    // Four failed tries: 1 built at start, then 4 more.
    wait_for("four tries", || app.keyboard.built() >= 5).await;
    let times = app.keyboard.times();
    // The waits between tries grow: at least 40, 80, 160, 160 ms.
    for (n, pair) in times[1..5].windows(2).enumerate() {
        let gap = pair[1] - pair[0];
        let want = FAST.backoff(u32::try_from(n).unwrap() + 2);
        assert!(
            gap >= want,
            "try {}: waited {gap:?}, less than {want:?}",
            n + 2
        );
    }
    // The dead source was stopped; nothing listens while access is off.
    assert_eq!(app.keyboard.listening(), 0);
    // Nothing reaches the UI while it is down: no error per try.
    app.assert_quiet().await;

    app.keyboard.deny_access(false);
    assert_eq!(app.next().await, UiEvent::HotkeyRestored);
    assert_eq!(app.keyboard.listening(), 1);
    assert_eq!(app.dictate().await, HotkeySpec::default());
    app.pipeline.shutdown().await;
}

/// Criterion 1: it doesn't spin. Tries are spaced by at least the capped
/// wait, so a hotkey that stays down for a second is tried a handful of
/// times, not thousands.
#[tokio::test(flavor = "multi_thread")]
async fn criterion_1_a_hotkey_that_stays_down_is_tried_a_few_times_a_second() {
    let mut app = App::start(Keyboard::default(), FAST);
    app.keyboard.deny_access(true);
    app.kill_hotkey().await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    // 40 + 80 + 160 + 160 + ... ms: at most 8 tries in a second.
    let tries = app.keyboard.built() - 1;
    assert!((3..=8).contains(&tries), "{tries} tries in a second");
    app.pipeline.shutdown().await;
}

/// Criterion 1: Quit (the pipeline's shutdown) stops the retries at once,
/// even in the middle of a long wait.
#[tokio::test(flavor = "multi_thread")]
async fn criterion_1_quit_stops_the_retries() {
    let slow = RetryPolicy {
        base_delay: Duration::from_secs(30),
        max_delay: Duration::from_secs(30),
        ..RESTART_POLICY
    };
    let mut app = App::start(Keyboard::default(), slow);
    app.keyboard.deny_access(true);
    app.kill_hotkey().await;
    let keyboard = app.keyboard.clone();
    let before = keyboard.built();
    let stopping = Instant::now();
    tokio::time::timeout(WAIT, app.pipeline.shutdown())
        .await
        .expect("shutdown waited for the supervisor's 30 s wait");
    assert!(stopping.elapsed() < Duration::from_secs(2));

    // And with short waits, no try comes after the shutdown.
    let mut app = App::start(Keyboard::default(), FAST);
    app.keyboard.deny_access(true);
    app.kill_hotkey().await;
    wait_for("a retry", || app.keyboard.built() >= 2).await;
    let keyboard_fast = app.keyboard.clone();
    app.pipeline.shutdown().await;
    let after = keyboard_fast.built();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(keyboard_fast.built(), after, "tried after Quit");
    assert_eq!(keyboard_fast.listening(), 0);
    assert_eq!(keyboard.built(), before);
}

/// Criterion 1: no second listener. While the hotkey works the supervisor
/// starts nothing, and once it is back exactly one source listens, through
/// another failure and recovery too.
#[tokio::test(flavor = "multi_thread")]
async fn criterion_1_there_is_never_a_second_listener() {
    let mut app = App::start(Keyboard::default(), FAST);
    app.assert_quiet().await;
    assert_eq!(app.keyboard.built(), 1, "a working hotkey was rebuilt");

    for _ in 0..2 {
        app.keyboard.deny_access(true);
        app.kill_hotkey().await;
        wait_for("a retry", || app.keyboard.listening() == 0).await;
        app.keyboard.deny_access(false);
        assert_eq!(app.next().await, UiEvent::HotkeyRestored);
        let built = app.keyboard.built();
        app.assert_quiet().await;
        assert_eq!(app.keyboard.built(), built, "rebuilt once it listened");
        assert_eq!(app.keyboard.listening(), 1);
        app.dictate().await;
    }
    app.pipeline.shutdown().await;
    assert_eq!(app.keyboard.listening(), 0);
}

/// Criterion 1: a hotkey that can't start when the app starts (no
/// Accessibility at launch) doesn't stop the app: it shows as unavailable
/// and starts by itself once allowed (#50's case, while the app runs).
#[tokio::test(flavor = "multi_thread")]
async fn criterion_1_no_access_at_launch_then_granted_starts_the_hotkey() {
    let keyboard = Keyboard::default();
    keyboard.deny_access(true);
    let mut app = App::start(keyboard, FAST);
    assert_eq!(
        app.next().await,
        UiEvent::Error(DictationError::HotkeyFailed)
    );
    assert_eq!(app.keyboard.listening(), 0);
    app.keyboard.deny_access(false);
    assert_eq!(app.next().await, UiEvent::HotkeyRestored);
    app.dictate().await;
    app.pipeline.shutdown().await;
}

/// Criterion 2: the hotkey saved in Settings while it is down is the one
/// that comes back, and the save says so instead of asking for a restart.
#[tokio::test(flavor = "multi_thread")]
async fn criterion_2_a_settings_change_while_down_is_the_hotkey_that_comes_back() {
    let mut app = App::start(Keyboard::default(), FAST);
    app.keyboard.deny_access(true);
    app.kill_hotkey().await;
    wait_for("a retry", || app.keyboard.built() >= 2).await;

    let mut settings = Settings {
        hotkey: "F6".into(),
        ..Settings::default()
    };
    let change = hotkey_setting::apply(&app.control, &mut settings);
    assert_eq!(
        change,
        HotkeyChange::NotListening {
            wanted: spec("F6"),
            reason: Some(HotkeyError::PermissionDenied),
        }
    );
    assert_eq!(settings.hotkey, "F6", "the save keeps the new hotkey");
    assert_eq!(app.control.spec(), spec("F6"));

    // Every try from now on is for F6.
    let tried = app.keyboard.built();
    wait_for("a retry for F6", || app.keyboard.built() > tried).await;
    app.keyboard.deny_access(false);
    assert_eq!(app.next().await, UiEvent::HotkeyRestored);
    let built = app.keyboard.built.lock().unwrap().clone();
    assert!(
        built[tried..].iter().all(|b| b.0 == spec("F6")),
        "a try for the old hotkey after the save"
    );
    assert_eq!(app.dictate().await, spec("F6"));
    app.pipeline.shutdown().await;
}

/// Criterion 2: a save while it is down, once access is back already, makes
/// the new hotkey listen at once, without waiting for the next try.
#[tokio::test(flavor = "multi_thread")]
async fn criterion_2_a_save_while_down_with_access_back_listens_at_once() {
    let slow = RetryPolicy {
        base_delay: Duration::from_secs(30),
        max_delay: Duration::from_secs(30),
        ..RESTART_POLICY
    };
    let mut app = App::start(Keyboard::default(), slow);
    app.keyboard.deny_access(true);
    app.kill_hotkey().await;
    app.keyboard.deny_access(false);

    let mut settings = Settings {
        hotkey: "F6".into(),
        ..Settings::default()
    };
    assert!(matches!(
        hotkey_setting::apply(&app.control, &mut settings),
        HotkeyChange::Switched { .. }
    ));
    assert_eq!(app.next().await, UiEvent::HotkeyRestored);
    assert_eq!(app.dictate().await, spec("F6"));
    app.pipeline.shutdown().await;
}
