//! U6's one-field commands, against a settings file in a temp folder, a
//! memory keychain and fake on-device models.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use dictation_core::hotkey_spec::HotkeySpec;
use dictation_pipeline::hotkey_setting::ConfigurableHotkey;
use dictation_platform::fakes::{FakeHotkey, MemSecretStore};
use dictation_ports::HotkeySource;
use dictation_transcribe::engine::Transcribers;
use dictation_transcribe::local::SpeechModel;
use dictation_transcribe::mock::MockTranscriber;
use dictation_transcribe::models::ModelStore;

use super::*;
use crate::engine::{Loader, LocalModels};

const ENGLISH: &str = local_models::DEFAULT_MODEL;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("u6-prefs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Silent;

impl SpeechModel for Silent {
    fn run(&mut self, _: &[f32], _: Option<&str>) -> Result<String, String> {
        Ok(String::new())
    }
}

fn loads() -> Loader {
    Arc::new(|_: &Path| Ok(Box::new(Silent) as Box<dyn SpeechModel>))
}

fn fails() -> Loader {
    Arc::new(|_: &Path| Err(TranscribeError::Engine("the file is damaged".to_owned())))
}

/// A started hotkey, as the running app has.
fn hotkey() -> HotkeyControl {
    let (mut hotkey, control) =
        ConfigurableHotkey::new(HotkeySpec::default(), |_| FakeHotkey::new());
    hotkey.start(Box::new(|_| {})).unwrap();
    control
}

/// A started hotkey that can't listen: Accessibility isn't allowed.
fn waiting_for_accessibility() -> HotkeyControl {
    let (mut hotkey, control) = ConfigurableHotkey::new(HotkeySpec::default(), |_| {
        let fake = FakeHotkey::new();
        fake.fail_next_start(dictation_ports::HotkeyError::PermissionDenied);
        fake
    });
    hotkey.start(Box::new(|_| {})).unwrap();
    control
}

struct Rig {
    dir: TempDir,
    config: Arc<Config<MemSecretStore>>,
    live: LiveEngine,
    transcribers: Transcribers,
    control: HotkeyControl,
}

/// Settings in a temp folder; the engine starts as a stand-in "previous
/// engine" (the mock), so a test can see when it is replaced.
fn rig(tag: &str, loader: Loader) -> Rig {
    let dir = TempDir::new(tag);
    let config = Arc::new(Config::new(dir.0.join("config"), MemSecretStore::new()));
    let store = ModelStore::new(dir.0.join("models"), Arc::new(|_| Ok(u64::MAX))).unwrap();
    let transcribers = Transcribers::new(Engine::Mock(MockTranscriber::new("previous")));
    let live =
        LiveEngine::follow(transcribers.clone()).with_local(LocalModels::trusting(store, loader));
    Rig {
        dir,
        config,
        live,
        transcribers,
        control: hotkey(),
    }
}

/// Puts a stand-in of the catalogue size where model `id` goes.
fn downloaded(rig: &Rig, id: &str) {
    let model = local_models::find(id).unwrap();
    let dir = rig.dir.0.join("models");
    std::fs::create_dir_all(&dir).unwrap();
    let file = std::fs::File::create(dir.join(model.file_name)).unwrap();
    file.set_len(model.size_bytes).unwrap();
}

fn is_mock(transcribers: &Transcribers) -> bool {
    matches!(*transcribers.current(), Engine::Mock(_))
}

#[test]
fn criterion_2_language_saves_on_its_own_and_keeps_every_other_field() {
    let rig = rig("language", loads());
    // Another control's newer values are already stored.
    let stored = Settings {
        hotkey: "F6".into(),
        share_usage_events: false,
        launch_at_login: true,
        ..Settings::default()
    };
    rig.config.save_settings(&stored).unwrap();

    language_change(&rig.config, &rig.live, &rig.control, Some(" hi ".into())).unwrap();
    let after = rig.config.settings().unwrap();
    assert_eq!(after.language.as_deref(), Some("hi"));
    assert_eq!(
        (
            after.hotkey.as_str(),
            after.share_usage_events,
            after.launch_at_login
        ),
        ("F6", false, true)
    );

    language_change(&rig.config, &rig.live, &rig.control, Some(String::new())).unwrap();
    assert_eq!(
        rig.config.settings().unwrap().language,
        None,
        "empty: detect it"
    );
}

#[test]
fn criterion_2_an_invalid_language_changes_nothing() {
    let rig = rig("bad-language", loads());
    let error = language_change(
        &rig.config,
        &rig.live,
        &rig.control,
        Some("not a code!".into()),
    )
    .unwrap_err();
    assert!(!error.is_empty());
    assert_eq!(
        rig.config.settings().unwrap().language,
        Settings::default().language
    );
}

#[test]
fn criterion_4_choosing_a_provider_switches_to_its_default_model() {
    let mut settings = Settings {
        transcription: Transcription::Local,
        ..Settings::default()
    };
    choose_cloud(&mut settings, Provider::OpenAi);
    assert_eq!(settings.transcription, Transcription::Cloud);
    assert_eq!(settings.provider, Provider::OpenAi);
    assert_eq!(settings.model, Provider::OpenAi.default_model());

    settings.model = "custom".into();
    choose_cloud(&mut settings, Provider::OpenAi);
    assert_eq!(
        settings.model, "custom",
        "the same provider keeps its model"
    );
}

#[test]
fn criterion_4_use_this_switches_only_once_the_model_is_ready() {
    let (entered, entered_rx) = mpsc::channel::<()>();
    let (release, release_rx) = mpsc::channel::<()>();
    let release_rx = std::sync::Mutex::new(release_rx);
    let slow: Loader = Arc::new(move |_: &Path| {
        let _ = entered.send(());
        let _ = release_rx
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10));
        Ok(Box::new(Silent) as Box<dyn SpeechModel>)
    });
    let rig = rig("use-this", slow);
    downloaded(&rig, ENGLISH);

    let (config, live, control) = (
        Arc::clone(&rig.config),
        rig.live.clone(),
        rig.control.clone(),
    );
    let switching = std::thread::spawn(move || switch_to_local(&config, &live, &control, ENGLISH));
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    // Loading: the previous engine is still the one in use, and the
    // settings still choose it.
    assert!(is_mock(&rig.transcribers));
    assert_eq!(
        rig.config.settings().unwrap().transcription,
        Transcription::Cloud
    );

    release.send(()).unwrap();
    switching.join().unwrap().unwrap();
    let after = rig.config.settings().unwrap();
    assert_eq!(after.transcription, Transcription::Local);
    assert_eq!(after.local_model, ENGLISH);
    assert!(matches!(*rig.transcribers.current(), Engine::Local(_)));
    assert_eq!(get_status(&rig).state, EngineState::Ready);
}

fn get_status(rig: &Rig) -> EngineStatus {
    status_of(&rig.transcribers.current())
}

#[test]
fn criterion_4_a_model_that_fails_to_load_leaves_the_previous_engine_chosen() {
    let rig = rig("load-fails", fails());
    downloaded(&rig, ENGLISH);
    let error = switch_to_local(&rig.config, &rig.live, &rig.control, ENGLISH).unwrap_err();
    assert!(error.starts_with("Couldn't load "), "{error}");
    assert_eq!(
        rig.config.settings().unwrap().transcription,
        Transcription::Cloud
    );
}

/// U6 review round 1, finding 2: from on-device model A, choosing B loads B
/// first (and frees A); if the settings then can't be written, A must be
/// the engine again, as the file and the window still say.
#[test]
#[cfg(unix)]
fn criterion_4_a_failed_write_after_loading_puts_the_old_model_back() {
    use std::os::unix::fs::PermissionsExt as _;

    const MULTI: &str = "qwen3-asr-0.6b-q4_k_m";
    let rig = rig("write-fails", loads());
    downloaded(&rig, ENGLISH);
    downloaded(&rig, MULTI);
    switch_to_local(&rig.config, &rig.live, &rig.control, ENGLISH).unwrap();
    let local = rig.live.local().unwrap();
    assert_eq!(local.loaded(), Some(ENGLISH));

    // A full disk: nothing can be written in the settings folder.
    let folder = rig.dir.0.join("config");
    let mode = |bits| std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(bits));
    mode(0o500).unwrap();
    let error = switch_to_local(&rig.config, &rig.live, &rig.control, MULTI);
    mode(0o700).unwrap();

    assert!(error.is_err(), "the write was refused, so the switch fails");
    assert_eq!(rig.config.settings().unwrap().local_model, ENGLISH);
    assert_eq!(local.loaded(), Some(ENGLISH), "the live model is A again");
    assert!(matches!(*rig.transcribers.current(), Engine::Local(_)));
}

#[test]
fn criterion_4_a_model_that_is_not_downloaded_is_refused() {
    let rig = rig("missing", loads());
    let error = switch_to_local(&rig.config, &rig.live, &rig.control, ENGLISH).unwrap_err();
    assert!(error.contains("download"), "{error}");
    assert_eq!(
        rig.config.settings().unwrap().transcription,
        Transcription::Cloud
    );
    let error = switch_to_local(&rig.config, &rig.live, &rig.control, "no-such-model").unwrap_err();
    assert!(error.contains("no-such-model"), "{error}");
}

#[test]
fn criterion_4_status_says_loading_ready_or_why_it_failed() {
    assert_eq!(
        status_of(&Engine::Unavailable(TranscribeError::ModelLoading)).state,
        EngineState::Loading
    );
    assert_eq!(
        status_of(&Engine::Mock(MockTranscriber::new("x"))),
        EngineStatus {
            state: EngineState::Ready,
            message: None
        }
    );
    let failed = status_of(&Engine::Unavailable(TranscribeError::Engine("bad".into())));
    assert_eq!(failed.state, EngineState::Failed);
    assert!(failed.message.is_some());
}

#[test]
fn criterion_5_remove_key_deletes_it_from_the_keychain() {
    let rig = rig("remove-key", loads());
    rig.config
        .set_api_key(Provider::Groq, "gsk_remove-me-NOT-REAL-1234")
        .unwrap();
    assert!(rig.config.has_api_key(Provider::Groq).unwrap());
    rig.live.delete_key(&rig.config, Provider::Groq).unwrap();
    assert!(!rig.config.has_api_key(Provider::Groq).unwrap());
}

#[test]
fn links_are_a_closed_list_of_https_pages() {
    for link in [
        Link::GroqKeys,
        Link::OpenaiKeys,
        Link::Privacy,
        Link::Repository,
        Link::Licence,
        Link::Notice,
    ] {
        assert!(link.url().starts_with("https://"), "{link:?}");
    }
    assert!(Link::Privacy.url().ends_with("docs/privacy.md"));
}

#[test]
fn criterion_6_microphone_problems_say_what_to_do() {
    assert!(microphone_problem(&AudioError::PermissionDenied).contains("System Settings"));
    assert!(microphone_problem(&AudioError::NoDevice).contains("No microphone"));
    assert!(!microphone_problem(&AudioError::Device("raw detail".into())).contains("raw detail"));
}

#[test]
fn app_info_has_the_version() {
    assert_eq!(get_app_info().unwrap().version, env!("CARGO_PKG_VERSION"));
}

/// Every source file under `dir` (recursively) with one of `extensions`.
fn sources(dir: &Path, extensions: &[&str], found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !path.ends_with("target") && !path.ends_with("node_modules") {
                sources(&path, extensions, found);
            }
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension))
        {
            found.push(path);
        }
    }
}

/// U6 criterion 7b: the section is Speech now, so no user-facing sentence
/// may send the user to the old Transcription section (U1's error sentences
/// and helper lines). Searches the Rust crates, the shell and the UI.
#[test]
fn criterion_u6_7b_no_sentence_names_a_transcription_section() {
    let old = ["Settings → ", "Transcription"].concat();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for dir in ["crates", "src-tauri/src", "src"] {
        sources(&root.join(dir), &["rs", "ts", "tsx"], &mut files);
    }
    assert!(
        files.len() > 50,
        "the search found only {} files",
        files.len()
    );
    let named: Vec<String> = files
        .iter()
        .filter(|path| std::fs::read_to_string(path).is_ok_and(|text| text.contains(&old)))
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        named.is_empty(),
        "say Settings → Speech instead in: {named:?}"
    );
}

/// U9 criterion 1: the owner's stuck setup. A hotkey waiting for Accessibility
/// isn't a failed save, for any of the writes that go through [`write`].
#[tokio::test]
async fn a_hotkey_waiting_for_accessibility_fails_no_write() {
    let rig = rig("waiting", loads());
    let usage = Usage::disabled();
    let control = waiting_for_accessibility();
    // The first try has failed by the time `start` returns.
    assert!(
        control
            .notice()
            .is_some_and(|said| said.contains("Accessibility"))
    );

    let (live, keys) = (rig.live.clone(), control.clone());
    write(Arc::clone(&rig.config), &usage, move |config| {
        language_change(config, &live, &keys, Some("hi".into()))
    })
    .await
    .expect("the language is saved");
    assert_eq!(
        rig.config.settings().unwrap().language.as_deref(),
        Some("hi")
    );

    let (live, keys) = (rig.live.clone(), control.clone());
    write(Arc::clone(&rig.config), &usage, move |config| {
        commands::write_change(config, &live, &keys, |settings| {
            choose_cloud(settings, Provider::OpenAi);
        })
        .map_err(|error| error.to_string())
    })
    .await
    .expect("the own-key route's engine is saved");
    assert_eq!(rig.config.settings().unwrap().provider, Provider::OpenAi);

    let (live, keys) = (rig.live.clone(), control.clone());
    write(Arc::clone(&rig.config), &usage, move |config| {
        choose_local(config, &live, &keys, ENGLISH)
    })
    .await
    .expect("Download and continue's choice is saved");
    assert_eq!(
        rig.config.settings().unwrap().transcription,
        Transcription::Local
    );

    // Settings still has the message to show, in the key's label.
    let said = control.notice().expect("Settings tells the user");
    assert!(said.contains("Right ⌥"), "{said}");
    assert!(!said.contains("RightAlt"), "{said}");
}

/// A write that really failed still says so.
#[tokio::test]
async fn a_write_that_fails_is_still_an_error() {
    let rig = rig("fails", loads());
    let error = write(Arc::clone(&rig.config), &Usage::disabled(), |_| {
        Err("not saved".to_owned())
    })
    .await
    .unwrap_err();
    assert_eq!(error, "not saved");
}
