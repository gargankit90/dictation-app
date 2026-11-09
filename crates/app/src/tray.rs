//! The menu-bar menu as Vocet's home (U5): what the tray's icon, tooltip and
//! menu show, as pure data the shell applies to a native menu.
//!
//! The menu, top to bottom (U12): a status line (ready and which key, a
//! download's progress, or the one problem), one fix row while there is a
//! problem, "Copy last dictation", History, Settings, Help (and Resume setup
//! while onboarding is incomplete), then Quit. Updates, Troubleshoot and
//! Report a problem live in Settings.
//! The tray never has a text title: the problem state is a second icon.
//!
//! [`TrayStatus`] follows the pipeline's [`UiEvent`]s and what the shell
//! reads (settings, engine, downloads, permissions); [`TrayStatus::menu`]
//! turns that into a [`MenuModel`] the shell builds an `NSMenu` from.

use dictation_core::coordinator::Phase;
use dictation_core::diagnostics::ErrorCode;
use dictation_core::error::DictationError;

use crate::pipeline::UiEvent;

/// The app's name as the menu says it.
pub const APP_NAME: &str = "Vocet";

/// Item ids, shared with the shell's click handler.
pub mod id {
    /// The status line.
    pub const STATUS: &str = "status";
    /// "Open Speech settings…".
    pub const FIX_SPEECH: &str = "fix-speech-settings";
    /// "Open Microphone settings…".
    pub const FIX_MICROPHONE: &str = "fix-microphone-settings";
    /// "Open Accessibility settings…".
    pub const FIX_ACCESSIBILITY: &str = "fix-accessibility-settings";
    /// "Relaunch Vocet".
    pub const FIX_RELAUNCH: &str = "fix-relaunch";
    /// "Download the model…".
    pub const FIX_MODEL: &str = "fix-download-model";
    /// "Choose another key…".
    pub const FIX_KEY: &str = "fix-choose-key";
    /// "Troubleshoot…" as the fix.
    pub const FIX_TROUBLESHOOT: &str = "fix-troubleshoot";
    /// "Copy last dictation".
    pub const COPY_LAST: &str = "copy-last";
    /// "History…" (U7).
    pub const HISTORY: &str = "history";
    /// "Settings…".
    pub const SETTINGS: &str = "settings";
    /// "Help…": Settings, on Help.
    pub const HELP: &str = "help";
    /// "Resume setup…".
    pub const RESUME_SETUP: &str = "onboarding";
    /// "Quit Vocet".
    pub const QUIT: &str = "quit";
}

/// What a problem's one fix item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    /// Settings, on the key and engine.
    OpenSpeechSettings,
    /// System Settings, Privacy & Security, Microphone.
    OpenMicrophoneSettings,
    /// System Settings, Privacy & Security, Accessibility.
    OpenAccessibilitySettings,
    /// Restart the app, so a grant takes effect.
    Relaunch,
    /// Settings, Speech, where models download.
    DownloadModel,
    /// Settings, on the dictation key.
    ChooseAnotherKey,
    /// The Troubleshoot page.
    Troubleshoot,
}

impl Fix {
    const ALL: [Self; 7] = [
        Self::OpenSpeechSettings,
        Self::OpenMicrophoneSettings,
        Self::OpenAccessibilitySettings,
        Self::Relaunch,
        Self::DownloadModel,
        Self::ChooseAnotherKey,
        Self::Troubleshoot,
    ];

    /// The item's id.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::OpenSpeechSettings => id::FIX_SPEECH,
            Self::OpenMicrophoneSettings => id::FIX_MICROPHONE,
            Self::OpenAccessibilitySettings => id::FIX_ACCESSIBILITY,
            Self::Relaunch => id::FIX_RELAUNCH,
            Self::DownloadModel => id::FIX_MODEL,
            Self::ChooseAnotherKey => id::FIX_KEY,
            Self::Troubleshoot => id::FIX_TROUBLESHOOT,
        }
    }

    /// The item's label: it names the action.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::OpenSpeechSettings => "Open Speech settings…",
            Self::OpenMicrophoneSettings => "Open Microphone settings…",
            Self::OpenAccessibilitySettings => "Open Accessibility settings…",
            Self::Relaunch => "Relaunch Vocet",
            Self::DownloadModel => "Download the model…",
            Self::ChooseAnotherKey => "Choose another key…",
            Self::Troubleshoot => "Troubleshoot…",
        }
    }

    /// The fix for the item `item`, if it is one.
    #[must_use]
    pub fn from_id(item: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|fix| fix.id() == item)
    }

    /// Whether this fix sends the user to System Settings for a grant that
    /// reaches the app only after a relaunch.
    #[must_use]
    pub const fn is_grant(self) -> bool {
        matches!(
            self,
            Self::OpenMicrophoneSettings | Self::OpenAccessibilitySettings
        )
    }
}

/// The one fix for `code`; codes with no special action get Troubleshoot.
#[must_use]
pub const fn fix_for(code: ErrorCode) -> Fix {
    match code {
        ErrorCode::KeyMissing
        | ErrorCode::KeyRejected
        | ErrorCode::KeyUnreadable
        | ErrorCode::ProviderRedirect => Fix::OpenSpeechSettings,
        ErrorCode::MicDenied => Fix::OpenMicrophoneSettings,
        ErrorCode::AccessibilityMissing | ErrorCode::PasteFailed => Fix::OpenAccessibilitySettings,
        ErrorCode::ModelMissing | ErrorCode::ModelLoadingFailed => Fix::DownloadModel,
        ErrorCode::HotkeyUnregistered => Fix::ChooseAnotherKey,
        ErrorCode::MicFailed
        | ErrorCode::NetworkUnreachable
        | ErrorCode::ProviderQuota
        | ErrorCode::ModelLoading
        | ErrorCode::EngineFailed
        | ErrorCode::NoSpeech
        | ErrorCode::SecureInput => Fix::Troubleshoot,
    }
}

/// The status line while Accessibility was just granted but the key may
/// still be dead until Vocet relaunches.
pub const RELAUNCH_SENTENCE: &str = "Relaunch Vocet to finish turning on Accessibility";

/// The one sentence the status line gives for `code`.
#[must_use]
pub fn problem_sentence(code: ErrorCode) -> &'static str {
    code.info().message
}

/// Whether `code` comes from the engine, key or model, so changing any of
/// them may have fixed it. Permissions, the hotkey and a single dictation's
/// outcome (no speech, paste, password field) are not.
#[must_use]
pub const fn is_engine_error(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::KeyMissing
            | ErrorCode::KeyRejected
            | ErrorCode::KeyUnreadable
            | ErrorCode::NetworkUnreachable
            | ErrorCode::ProviderQuota
            | ErrorCode::ProviderRedirect
            | ErrorCode::ModelMissing
            | ErrorCode::ModelLoading
            | ErrorCode::ModelLoadingFailed
            | ErrorCode::EngineFailed
    )
}

/// Whether a download answers the engine error `code`: an engine that isn't
/// set up yet, or any engine error when the download is the chosen engine
/// (the user is already fixing it, Design Lead item 11).
#[must_use]
pub const fn download_answers(code: ErrorCode, chosen: bool) -> bool {
    is_engine_error(code)
        && (chosen
            || matches!(
                code,
                ErrorCode::KeyMissing | ErrorCode::ModelMissing | ErrorCode::ModelLoading
            ))
}

pub use dictation_core::hotkey_spec::keycap_label;

/// Which template image the tray shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayIcon {
    /// The three-bar mark.
    #[default]
    Normal,
    /// The mark with a dot: there is a problem.
    Attention,
    /// The mark filled: recording.
    Recording,
}

/// A model downloading. The menu calls it "speech model": it doesn't bring
/// back the names Settings leaves to the "…" menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    /// Percent done, 0 to 100.
    pub percent: u8,
    /// Whether the settings already choose this model as the engine.
    pub chosen: bool,
}

/// What the shell reads each second, apart from the pipeline's events.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ambient {
    /// The saved hotkey, as the settings write it (`RightAlt`).
    pub hotkey: String,
    /// The engine's display name ("Groq", "On this Mac (Parakeet English)").
    pub engine: String,
    /// A model downloading now.
    pub download: Option<Download>,
    /// Why dictation can't work now: no key, no model, or a permission the
    /// OS refuses.
    pub problem: Option<ErrorCode>,
    /// Whether onboarding is incomplete.
    pub setup_incomplete: bool,
    /// Which engine is built right now (changes when a key is saved, the
    /// engine or model is switched, or a download finishes). An opaque,
    /// non-secret number.
    pub engine_generation: usize,
}

/// One entry of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// An item.
    Item {
        /// Its id.
        id: &'static str,
        /// Its label.
        label: String,
        /// Whether it can be clicked.
        enabled: bool,
        /// Its shortcut, if any.
        accelerator: Option<&'static str>,
    },
    /// A separator line.
    Separator,
}

impl Entry {
    fn item(id: &'static str, label: impl Into<String>) -> Self {
        Self::Item {
            id,
            label: label.into(),
            enabled: true,
            accelerator: None,
        }
    }

    fn disabled(id: &'static str, label: impl Into<String>) -> Self {
        Self::Item {
            id,
            label: label.into(),
            enabled: false,
            accelerator: None,
        }
    }
}

/// What the tray shows: the icon, the tooltip and the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuModel {
    /// The template image.
    pub icon: TrayIcon,
    /// The hover text.
    pub tooltip: String,
    /// The menu, top to bottom.
    pub entries: Vec<Entry>,
}

impl MenuModel {
    /// The ids and labels of the items, separators as `("-", "")`, for tests
    /// and the e2e log line.
    #[must_use]
    pub fn outline(&self) -> Vec<(String, String)> {
        self.entries
            .iter()
            .map(|entry| match entry {
                Entry::Item { id, label, .. } => ((*id).to_owned(), label.clone()),
                Entry::Separator => ("-".to_owned(), String::new()),
            })
            .collect()
    }
}

/// The tray's state.
#[derive(Debug, Default)]
pub struct TrayStatus {
    /// The dictation error showing, until the next recording starts.
    error: Option<ErrorCode>,
    /// Whether the hotkey doesn't work.
    hotkey_down: bool,
    /// Whether the user went to System Settings for a grant: only a
    /// relaunch picks it up, so the fix row says so.
    grant_opened: bool,
    /// Whether a recording is in progress.
    recording: bool,
    /// Whether Accessibility was missing (as the OS reports it) at some
    /// point since the hotkey last worked. While the hotkey is still down
    /// after the grant, macOS has not applied it to this process: the menu
    /// asks for a relaunch. Cleared when the hotkey is restored or a
    /// recording starts.
    lacked_accessibility: bool,
    /// Whether there is a dictation to copy.
    has_last: bool,
    ambient: Ambient,
}

impl TrayStatus {
    /// The tray at launch: `microphone_denied` if the OS already refuses
    /// the microphone, so the problem shows before the first press.
    #[must_use]
    pub fn new(microphone_denied: bool) -> Self {
        Self {
            error: microphone_denied.then_some(ErrorCode::MicDenied),
            ..Self::default()
        }
    }

    /// Applies a pipeline event; `true` if the menu may have changed.
    ///
    /// A dictation error shows until the next recording starts. While the
    /// hotkey doesn't work ([`DictationError::HotkeyFailed`] until
    /// [`UiEvent::HotkeyRestored`]) that is the problem, unless a newer
    /// dictation error shows over it.
    pub fn on_event(&mut self, event: &UiEvent) -> bool {
        let before = (self.error, self.hotkey_down, self.recording);
        match event {
            UiEvent::Error(DictationError::HotkeyFailed) => {
                // Only missing Accessibility (seen in `set_ambient`) makes a
                // dead hotkey an Accessibility matter; any other cause is
                // just the hotkey's own problem.
                self.hotkey_down = true;
                self.error = None;
            }
            UiEvent::Error(error) => {
                let code = ErrorCode::of(error);
                if code == ErrorCode::AccessibilityMissing {
                    self.lacked_accessibility = true;
                }
                self.error = Some(code);
            }
            // A one-off notice, not a problem to fix: no attention (H2).
            UiEvent::NoSound => {}
            UiEvent::HotkeyRestored => {
                // The listener runs again: every hotkey problem is over.
                self.hotkey_down = false;
                self.lacked_accessibility = false;
            }
            UiEvent::StateChanged(phase) => {
                self.recording = *phase == Phase::Recording;
                if self.recording {
                    // The microphone and the rest work after all, and the
                    // key demonstrably does.
                    self.error = None;
                    self.lacked_accessibility = false;
                }
            }
        }
        if self.runtime_problem().is_none() {
            self.grant_opened = false;
        }
        before != (self.error, self.hotkey_down, self.recording)
    }

    /// Replaces what the shell read; `true` if it changed.
    pub fn set_ambient(&mut self, ambient: Ambient) -> bool {
        if self.ambient == ambient {
            return false;
        }
        // The user changed the engine, its key or its model, or started a
        // download: an error from the old setup no longer holds. Others stay.
        if ambient.problem == Some(ErrorCode::AccessibilityMissing) {
            self.lacked_accessibility = true;
        }
        let reconfigured = ambient.engine_generation != self.ambient.engine_generation
            || ambient.engine != self.ambient.engine
            || (ambient.download.is_some() && self.ambient.download.is_none());
        if reconfigured && self.error.is_some_and(is_engine_error) {
            self.error = None;
        }
        self.ambient = ambient;
        true
    }

    /// Whether there is a dictation to copy, which enables "Copy last
    /// dictation".
    pub fn set_has_last(&mut self, has_last: bool) -> bool {
        let changed = self.has_last != has_last;
        self.has_last = has_last;
        changed
    }

    /// The user clicked a fix that goes to System Settings for a grant.
    pub fn grant_opened(&mut self) {
        self.grant_opened = true;
    }

    /// What went wrong while dictating, or the hotkey.
    fn runtime_problem(&self) -> Option<ErrorCode> {
        self.error
            .or(self.hotkey_down.then_some(ErrorCode::HotkeyUnregistered))
    }

    /// The problem the status line gives: what happened last, then the
    /// hotkey, then what the shell found wrong.
    #[must_use]
    pub fn problem(&self) -> Option<ErrorCode> {
        self.runtime_problem().or(self.ambient.problem)
    }

    /// The problem as shown: a dead hotkey while Accessibility is missing is
    /// the Accessibility problem, whose fix is the grant.
    fn shown_problem(&self) -> Option<ErrorCode> {
        let problem = self.problem()?;
        if problem == ErrorCode::HotkeyUnregistered
            && self.ambient.problem == Some(ErrorCode::AccessibilityMissing)
        {
            return Some(ErrorCode::AccessibilityMissing);
        }
        Some(problem)
    }

    /// Whether the menu must ask for a relaunch: Accessibility is granted
    /// now, but was missing earlier in this run, and no recording has
    /// shown the key works.
    #[must_use]
    pub fn relaunch_needed(&self) -> bool {
        self.lacked_accessibility
            && self.hotkey_down
            && self.error.is_none()
            && self.ambient.problem != Some(ErrorCode::AccessibilityMissing)
    }

    /// The fix row's action for the current problem, if there is one.
    #[must_use]
    pub fn fix(&self) -> Option<Fix> {
        if self.relaunch_needed() {
            return Some(Fix::Relaunch);
        }
        let fix = fix_for(self.shown_problem()?);
        Some(if self.grant_opened && fix.is_grant() {
            Fix::Relaunch
        } else {
            fix
        })
    }

    /// What the tray shows now.
    #[must_use]
    pub fn menu(&self) -> MenuModel {
        // A runtime problem outranks a download, unless the download is the
        // fix for it; a download outranks what the engine's state says (the
        // model it waits for is coming).
        let runtime = self.runtime_problem();
        let download =
            self.ambient.download.as_ref().filter(|download| {
                runtime.is_none_or(|code| download_answers(code, download.chosen))
            });
        let relaunch = self.relaunch_needed();
        let problem = if download.is_some() || relaunch {
            None
        } else {
            self.shown_problem()
        };
        let mut entries = Vec::new();
        let tooltip = if let Some(code) = problem {
            let sentence = if code == ErrorCode::HotkeyUnregistered {
                format!(
                    "{} isn't working. Choose another key.",
                    keycap_label(&self.ambient.hotkey)
                )
            } else {
                problem_sentence(code).to_owned()
            };
            entries.push(Entry::disabled(id::STATUS, format!("! {sentence}")));
            if let Some(fix) = self.fix() {
                entries.push(Entry::item(fix.id(), fix.label()));
            }
            format!("{APP_NAME}: {sentence}")
        } else if relaunch {
            entries.push(Entry::disabled(
                id::STATUS,
                format!("! {RELAUNCH_SENTENCE}"),
            ));
            entries.push(Entry::item(Fix::Relaunch.id(), Fix::Relaunch.label()));
            format!("{APP_NAME}: {RELAUNCH_SENTENCE}")
        } else if let Some(download) = download {
            let text = format!("↓ Downloading speech model, {}%", download.percent);
            entries.push(Entry::disabled(id::STATUS, text));
            format!("{APP_NAME}: downloading speech model")
        } else {
            let key = keycap_label(&self.ambient.hotkey);
            entries.push(Entry::disabled(id::STATUS, format!("✓ Ready · Hold {key}")));
            format!("{APP_NAME}: Ready")
        };
        entries.push(Entry::Separator);
        entries.push(Entry::Item {
            id: id::COPY_LAST,
            label: "Copy last dictation".to_owned(),
            enabled: self.has_last,
            accelerator: None,
        });
        entries.push(Entry::item(id::HISTORY, "History…"));
        entries.push(Entry::Separator);
        entries.push(Entry::Item {
            id: id::SETTINGS,
            label: "Settings…".to_owned(),
            enabled: true,
            accelerator: Some("CmdOrCtrl+,"),
        });
        entries.push(Entry::item(id::HELP, "Help…"));
        if self.ambient.setup_incomplete {
            entries.push(Entry::item(id::RESUME_SETUP, "Resume setup…"));
        }
        entries.push(Entry::Separator);
        entries.push(Entry::Item {
            id: id::QUIT,
            label: format!("Quit {APP_NAME}"),
            enabled: true,
            accelerator: Some("CmdOrCtrl+Q"),
        });
        // While the key is held the recording icon wins; the dot is back
        // after, if the problem remains.
        let icon = if self.recording {
            TrayIcon::Recording
        } else if problem.is_some() || relaunch {
            TrayIcon::Attention
        } else {
            TrayIcon::Normal
        };
        MenuModel {
            icon,
            tooltip,
            entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use dictation_core::error::Permission;

    use super::*;

    fn ambient(hotkey: &str, engine: &str) -> Ambient {
        Ambient {
            hotkey: hotkey.to_owned(),
            engine: engine.to_owned(),
            ..Ambient::default()
        }
    }

    fn ready(hotkey: &str, engine: &str) -> TrayStatus {
        let mut tray = TrayStatus::default();
        tray.set_ambient(ambient(hotkey, engine));
        tray
    }

    fn ids(model: &MenuModel) -> Vec<String> {
        model.outline().into_iter().map(|(id, _)| id).collect()
    }

    fn label_of(model: &MenuModel, wanted: &str) -> Option<String> {
        model
            .outline()
            .into_iter()
            .find(|(id, _)| id == wanted)
            .map(|(_, label)| label)
    }

    fn enabled(model: &MenuModel, wanted: &str) -> Option<bool> {
        model.entries.iter().find_map(|entry| match entry {
            Entry::Item { id, enabled, .. } if *id == wanted => Some(*enabled),
            _ => None,
        })
    }

    fn error(error: DictationError) -> UiEvent {
        UiEvent::Error(error)
    }

    fn transcription_failed(reason: &str) -> UiEvent {
        error(DictationError::TranscriptionFailed {
            reason: reason.to_owned(),
        })
    }

    /// U12 criterion 7: the menu, exactly, whichever engine is in use.
    #[test]
    fn u12_criterion_7_ready_menu_in_order() {
        let tray = ready("RightAlt", "On this Mac (Parakeet English)");
        let menu = tray.menu();
        assert_eq!(
            menu.outline(),
            [
                ("status", "✓ Ready · Hold Right ⌥"),
                ("-", ""),
                ("copy-last", "Copy last dictation"),
                ("history", "History…"),
                ("-", ""),
                ("settings", "Settings…"),
                ("help", "Help…"),
                ("-", ""),
                ("quit", "Quit Vocet"),
            ]
            .map(|(id, label)| (id.to_owned(), label.to_owned()))
        );
        assert_eq!(menu.icon, TrayIcon::Normal);
        assert_eq!(menu.tooltip, "Vocet: Ready");
        // The status lines can't be clicked.
        assert_eq!(enabled(&menu, id::STATUS), Some(false));
        // The shortcuts the design shows.
        let accelerator = |wanted: &str| {
            menu.entries.iter().find_map(|entry| match entry {
                Entry::Item {
                    id, accelerator, ..
                } if *id == wanted => *accelerator,
                _ => None,
            })
        };
        assert_eq!(accelerator(id::SETTINGS), Some("CmdOrCtrl+,"));
        assert_eq!(accelerator(id::QUIT), Some("CmdOrCtrl+Q"));
        assert_eq!(accelerator(id::HELP), None);
    }

    /// U12 criterion 7: no engine line, no Troubleshoot, Report a problem or
    /// Check for updates, in any state but a fix row of its own.
    #[test]
    fn u12_criterion_7_the_old_items_are_gone() {
        let mut problem = ready("RightAlt", "Groq");
        problem.on_event(&transcription_failed("The provider rejected your API key."));
        for tray in [ready("RightAlt", "Groq"), problem] {
            for (item, label) in tray.menu().outline() {
                assert_ne!(item, "engine");
                assert_ne!(label, "Groq");
                for gone in ["Troubleshoot", "Report a problem", "Check for updates"] {
                    assert!(!label.contains(gone), "{label}");
                }
            }
        }
    }

    /// U12 criterion 7, Design Lead item 11: while the chosen model downloads
    /// the menu says so, whatever a press with no engine said meanwhile.
    #[test]
    fn u12_criterion_7_a_download_of_the_chosen_model_outranks_no_key() {
        let local = "On this Mac (Parakeet English)";
        let downloading = |chosen| Ambient {
            download: Some(Download {
                percent: 42,
                chosen,
            }),
            ..ambient("RightAlt", local)
        };
        for chosen in [true, false] {
            // The ambient problem: no key saved, a download running.
            let mut tray = ready("RightAlt", local);
            tray.set_ambient(Ambient {
                problem: Some(ErrorCode::KeyMissing),
                ..downloading(chosen)
            });
            // A press after the download began: the pipeline says no key.
            tray.on_event(&transcription_failed("Add your API key in Settings."));
            assert_eq!(tray.problem(), Some(ErrorCode::KeyMissing));
            let menu = tray.menu();
            assert_eq!(
                label_of(&menu, id::STATUS).as_deref(),
                Some("↓ Downloading speech model, 42%"),
                "chosen: {chosen}"
            );
            assert!(label_of(&menu, id::FIX_SPEECH).is_none());
            assert_eq!(menu.icon, TrayIcon::Normal);
        }
    }

    /// A download of another model doesn't hide what is wrong with the engine
    /// that is in use, and nothing hides a problem a download can't fix.
    #[test]
    fn u12_criterion_7_a_download_doesnt_hide_other_problems() {
        let downloading = |chosen| Ambient {
            download: Some(Download {
                percent: 42,
                chosen,
            }),
            ..ambient("RightAlt", "Groq")
        };
        let mut tray = ready("RightAlt", "Groq");
        tray.set_ambient(downloading(false));
        tray.on_event(&transcription_failed("The provider rejected your API key."));
        assert!(label_of(&tray.menu(), id::STATUS).unwrap().starts_with('!'));
        // The chosen model's download doesn't answer a microphone refusal.
        let mut tray = TrayStatus::new(true);
        tray.set_ambient(downloading(true));
        assert_eq!(
            label_of(&tray.menu(), id::FIX_MICROPHONE).as_deref(),
            Some("Open Microphone settings…")
        );
        let mut tray = ready("RightAlt", "Groq");
        tray.set_ambient(downloading(true));
        tray.on_event(&error(DictationError::HotkeyFailed));
        assert!(label_of(&tray.menu(), id::STATUS).unwrap().starts_with('!'));
    }

    #[test]
    fn criterion_1_ready_with_groq() {
        let menu = ready("Ctrl+Shift+Space", "Groq").menu();
        assert_eq!(
            label_of(&menu, id::STATUS).as_deref(),
            Some("✓ Ready · Hold ⌃⇧Space")
        );
    }

    #[test]
    fn criterion_1_downloading_shows_progress_and_no_fix() {
        let mut tray = ready("RightAlt", "On this Mac (Parakeet English)");
        tray.set_ambient(Ambient {
            download: Some(Download {
                percent: 42,
                chosen: true,
            }),
            problem: Some(ErrorCode::ModelMissing),
            ..ambient("RightAlt", "On this Mac (Parakeet English)")
        });
        let menu = tray.menu();
        assert_eq!(
            menu.outline().first().cloned(),
            Some((
                "status".to_owned(),
                "↓ Downloading speech model, 42%".to_owned()
            ))
        );
        assert_eq!(&ids(&menu)[1], "-");
        assert_eq!(menu.icon, TrayIcon::Normal);
    }

    #[test]
    fn criterion_1_setup_incomplete_adds_resume_setup_after_help() {
        let mut tray = ready("RightAlt", "Groq");
        tray.set_ambient(Ambient {
            setup_incomplete: true,
            ..ambient("RightAlt", "Groq")
        });
        let listed = ids(&tray.menu());
        let help = listed.iter().position(|id| id == "help").unwrap();
        assert_eq!(listed[help + 1], "onboarding");
        assert_eq!(listed[help + 2], "-");
        let complete = ids(&ready("RightAlt", "Groq").menu());
        assert!(!complete.contains(&"onboarding".to_owned()));
    }

    /// Criterion 1 and 3: every user-facing problem is the status line, the
    /// fix row right under it, and the attention icon.
    #[test]
    fn criterion_3_each_code_maps_to_exactly_one_fix_row() {
        let expected = [
            (
                ErrorCode::MicDenied,
                "fix-microphone-settings",
                "Open Microphone settings…",
            ),
            (ErrorCode::MicFailed, "fix-troubleshoot", "Troubleshoot…"),
            (
                ErrorCode::AccessibilityMissing,
                "fix-accessibility-settings",
                "Open Accessibility settings…",
            ),
            (
                ErrorCode::HotkeyUnregistered,
                "fix-choose-key",
                "Choose another key…",
            ),
            (
                ErrorCode::KeyMissing,
                "fix-speech-settings",
                "Open Speech settings…",
            ),
            (
                ErrorCode::KeyRejected,
                "fix-speech-settings",
                "Open Speech settings…",
            ),
            (
                ErrorCode::KeyUnreadable,
                "fix-speech-settings",
                "Open Speech settings…",
            ),
            (
                ErrorCode::NetworkUnreachable,
                "fix-troubleshoot",
                "Troubleshoot…",
            ),
            (
                ErrorCode::ProviderQuota,
                "fix-troubleshoot",
                "Troubleshoot…",
            ),
            (
                ErrorCode::ProviderRedirect,
                "fix-speech-settings",
                "Open Speech settings…",
            ),
            (
                ErrorCode::ModelMissing,
                "fix-download-model",
                "Download the model…",
            ),
            (ErrorCode::ModelLoading, "fix-troubleshoot", "Troubleshoot…"),
            (
                ErrorCode::ModelLoadingFailed,
                "fix-download-model",
                "Download the model…",
            ),
            (ErrorCode::EngineFailed, "fix-troubleshoot", "Troubleshoot…"),
            (ErrorCode::NoSpeech, "fix-troubleshoot", "Troubleshoot…"),
            (
                ErrorCode::PasteFailed,
                "fix-accessibility-settings",
                "Open Accessibility settings…",
            ),
            (ErrorCode::SecureInput, "fix-troubleshoot", "Troubleshoot…"),
        ];
        // Every code is covered, in the table's order.
        let codes: Vec<_> = expected.iter().map(|(code, _, _)| *code).collect();
        assert_eq!(codes, ErrorCode::ALL);
        for (code, fix_id, label) in expected {
            let mut tray = ready("RightAlt", "Groq");
            tray.set_ambient(Ambient {
                problem: Some(code),
                ..ambient("RightAlt", "Groq")
            });
            let menu = tray.menu();
            let outline = menu.outline();
            let sentence = if code == ErrorCode::HotkeyUnregistered {
                "Right ⌥ isn't working. Choose another key.".to_owned()
            } else {
                problem_sentence(code).to_owned()
            };
            assert_eq!(
                outline[0],
                ("status".to_owned(), format!("! {sentence}")),
                "{code}"
            );
            assert_eq!(outline[1], (fix_id.to_owned(), label.to_owned()), "{code}");
            assert_eq!(outline[2].0, "-", "{code}: exactly one fix row");
            assert_eq!(enabled(&menu, fix_id), Some(true), "{code}");
            assert_eq!(menu.icon, TrayIcon::Attention, "{code}");
            assert_eq!(menu.tooltip, format!("Vocet: {sentence}"), "{code}");
            assert_eq!(Fix::from_id(fix_id), Some(fix_for(code)), "{code}");
        }
    }

    /// A code the table doesn't list, if one were added, still lands on
    /// Troubleshoot because `fix_for`'s match is exhaustive; and an unknown
    /// item id is no fix.
    #[test]
    fn unknown_ids_are_not_fixes() {
        assert_eq!(Fix::from_id("nope"), None);
        assert_eq!(Fix::from_id(id::SETTINGS), None);
    }

    /// The two real-world sources of a problem, from the pipeline.
    #[test]
    fn criterion_2_events_raise_and_clear_the_problem() {
        let mut tray = ready("RightAlt", "Groq");
        assert_eq!(tray.problem(), None);
        assert!(tray.on_event(&transcription_failed("The provider rejected your API key.")));
        let menu = tray.menu();
        assert_eq!(
            label_of(&menu, id::STATUS).as_deref(),
            Some("! Your API key was rejected.")
        );
        assert_eq!(menu.icon, TrayIcon::Attention);
        // Idle doesn't clear it; the next recording does.
        assert!(!tray.on_event(&UiEvent::StateChanged(Phase::Idle)));
        assert!(tray.problem().is_some());
        assert!(tray.on_event(&UiEvent::StateChanged(Phase::Recording)));
        let menu = tray.menu();
        assert_eq!(tray.problem(), None);
        assert_eq!(menu.icon, TrayIcon::Recording);
        assert!(label_of(&menu, id::STATUS).unwrap().starts_with("✓ Ready"));
        assert!(tray.on_event(&UiEvent::StateChanged(Phase::Idle)));
        assert_eq!(tray.menu().icon, TrayIcon::Normal);
    }

    #[test]
    fn the_status_follows_a_changed_key_and_engine() {
        let mut tray = ready("RightAlt", "Groq");
        assert!(!tray.set_ambient(ambient("RightAlt", "Groq")));
        assert!(tray.set_ambient(ambient("LeftCmd", "OpenAI")));
        let menu = tray.menu();
        assert_eq!(
            label_of(&menu, id::STATUS).as_deref(),
            Some("✓ Ready · Hold Left ⌘")
        );
    }

    #[test]
    fn the_hotkey_outage_is_a_problem_until_restored() {
        let mut tray = ready("RightAlt", "Groq");
        assert!(tray.on_event(&error(DictationError::HotkeyFailed)));
        assert_eq!(tray.fix(), Some(Fix::ChooseAnotherKey));
        // A newer dictation error shows over it, and the outage returns
        // after it clears.
        tray.on_event(&transcription_failed("Add your API key in Settings."));
        assert_eq!(tray.problem(), Some(ErrorCode::KeyMissing));
        tray.on_event(&UiEvent::StateChanged(Phase::Recording));
        assert_eq!(tray.problem(), Some(ErrorCode::HotkeyUnregistered));
        assert!(tray.on_event(&UiEvent::HotkeyRestored));
        assert_eq!(tray.problem(), None);
        assert!(!tray.on_event(&UiEvent::HotkeyRestored));
    }

    /// Design Lead: the dead-hotkey fix depends on Accessibility.
    #[test]
    fn the_hotkey_fix_is_the_grant_when_accessibility_is_missing() {
        let mut tray = ready("RightAlt", "Groq");
        tray.on_event(&error(DictationError::HotkeyFailed));
        let menu = tray.menu();
        assert_eq!(
            label_of(&menu, id::STATUS).as_deref(),
            Some("! Right ⌥ isn't working. Choose another key.")
        );
        assert_eq!(
            label_of(&menu, id::FIX_KEY).as_deref(),
            Some("Choose another key…")
        );
        tray.set_ambient(Ambient {
            problem: Some(ErrorCode::AccessibilityMissing),
            ..ambient("RightAlt", "Groq")
        });
        let menu = tray.menu();
        assert_eq!(tray.fix(), Some(Fix::OpenAccessibilitySettings));
        assert_eq!(
            label_of(&menu, id::STATUS).as_deref(),
            Some("! Vocet can't paste into other apps yet.")
        );
        assert!(label_of(&menu, id::FIX_KEY).is_none());
    }

    /// Design Lead: no "dictation-app" in any sentence the menu shows.
    #[test]
    fn no_problem_sentence_names_the_old_app() {
        for code in ErrorCode::ALL {
            assert!(!problem_sentence(code).contains("dictation-app"), "{code}");
        }
        assert_eq!(
            problem_sentence(ErrorCode::AccessibilityMissing),
            "Vocet can't paste into other apps yet."
        );
    }

    /// Design Lead: recording beats the dot, and the dot returns after.
    #[test]
    fn the_recording_icon_wins_over_the_attention_dot() {
        let mut tray = ready("RightAlt", "Groq");
        tray.set_ambient(Ambient {
            problem: Some(ErrorCode::KeyMissing),
            ..ambient("RightAlt", "Groq")
        });
        assert_eq!(tray.menu().icon, TrayIcon::Attention);
        tray.on_event(&UiEvent::StateChanged(Phase::Recording));
        let menu = tray.menu();
        assert_eq!(menu.icon, TrayIcon::Recording);
        // The status still says what is wrong.
        assert!(label_of(&menu, id::STATUS).unwrap().starts_with('!'));
        tray.on_event(&UiEvent::StateChanged(Phase::Transcribing));
        assert_eq!(tray.menu().icon, TrayIcon::Attention);
    }

    /// Codex round 1 #4: an error from the old key or engine goes when the
    /// setting changes; unrelated errors stay.
    #[test]
    fn a_rejected_key_clears_when_the_engine_is_switched_or_the_key_replaced() {
        let rejected = || transcription_failed("The provider rejected your API key.");
        // Switched to the local engine.
        let mut tray = ready("RightAlt", "Groq");
        tray.on_event(&rejected());
        assert_eq!(tray.problem(), Some(ErrorCode::KeyRejected));
        tray.set_ambient(Ambient {
            engine_generation: 2,
            ..ambient("RightAlt", "On this Mac (Parakeet English)")
        });
        assert_eq!(tray.problem(), None);
        assert_eq!(tray.menu().icon, TrayIcon::Normal);
        // Same provider, new key: the engine is rebuilt.
        let mut tray = TrayStatus::default();
        tray.set_ambient(Ambient {
            engine_generation: 1,
            ..ambient("RightAlt", "Groq")
        });
        tray.on_event(&rejected());
        assert!(tray.problem().is_some());
        // An unrelated change keeps it.
        tray.set_ambient(Ambient {
            engine_generation: 1,
            hotkey: "LeftAlt".to_owned(),
            ..ambient("RightAlt", "Groq")
        });
        assert!(tray.problem().is_some());
        tray.set_ambient(Ambient {
            engine_generation: 2,
            ..ambient("RightAlt", "Groq")
        });
        assert_eq!(tray.problem(), None);
    }

    #[test]
    fn errors_unrelated_to_the_engine_survive_a_new_engine() {
        for event in [
            error(DictationError::PermissionMissing(Permission::Accessibility)),
            error(DictationError::EmptyTranscript),
            error(DictationError::HotkeyFailed),
        ] {
            let mut tray = TrayStatus::default();
            tray.set_ambient(Ambient {
                engine_generation: 1,
                ..ambient("RightAlt", "Groq")
            });
            tray.on_event(&event);
            let before = tray.problem();
            assert!(before.is_some());
            tray.set_ambient(Ambient {
                engine_generation: 2,
                ..ambient("RightAlt", "Groq")
            });
            assert_eq!(tray.problem(), before, "{event:?}");
        }
    }

    /// Missing model, then a download starts, then the model is ready.
    #[test]
    fn a_missing_model_error_clears_when_its_download_starts_and_finishes() {
        let mut tray = TrayStatus::default();
        let local = "On this Mac (Parakeet English)";
        tray.set_ambient(Ambient {
            engine_generation: 1,
            problem: Some(ErrorCode::ModelMissing),
            ..ambient("RightAlt", local)
        });
        tray.on_event(&transcription_failed(
            "The on-device model isn't downloaded. Download it in Settings.",
        ));
        assert_eq!(tray.problem(), Some(ErrorCode::ModelMissing));
        tray.set_ambient(Ambient {
            engine_generation: 1,
            download: Some(Download {
                percent: 10,
                chosen: true,
            }),
            problem: Some(ErrorCode::ModelMissing),
            ..ambient("RightAlt", local)
        });
        let menu = tray.menu();
        assert!(
            label_of(&menu, id::STATUS)
                .unwrap()
                .starts_with("↓ Downloading")
        );
        tray.set_ambient(Ambient {
            engine_generation: 2,
            ..ambient("RightAlt", local)
        });
        assert!(
            label_of(&tray.menu(), id::STATUS)
                .unwrap()
                .starts_with("✓ Ready")
        );
        assert_eq!(tray.menu().icon, TrayIcon::Normal);
    }

    /// Codex round 2: a hotkey failure with no Accessibility cause shows its
    /// own row, and the restore returns to Ready at once, no dot.
    #[test]
    fn a_hotkey_failure_without_an_accessibility_cause_is_restored_to_ready() {
        let mut tray = ready("RightAlt", "Groq");
        tray.set_ambient(ambient("RightAlt", "Groq"));
        tray.on_event(&error(DictationError::HotkeyFailed));
        let menu = tray.menu();
        assert_eq!(
            label_of(&menu, id::STATUS).as_deref(),
            Some("! Right ⌥ isn't working. Choose another key.")
        );
        assert!(!tray.relaunch_needed());
        assert_eq!(tray.fix(), Some(Fix::ChooseAnotherKey));
        tray.on_event(&UiEvent::HotkeyRestored);
        let menu = tray.menu();
        assert!(label_of(&menu, id::STATUS).unwrap().starts_with("✓ Ready"));
        assert_eq!(menu.icon, TrayIcon::Normal);
        assert!(!tray.relaunch_needed());
    }

    /// Accessibility revoked, then granted again, the tap not live yet: the
    /// menu asks for a relaunch.
    #[test]
    fn a_regrant_with_the_tap_not_live_asks_for_a_relaunch() {
        let mut tray = ready("RightAlt", "Groq");
        tray.on_event(&error(DictationError::HotkeyFailed));
        tray.set_ambient(Ambient {
            problem: Some(ErrorCode::AccessibilityMissing),
            ..ambient("RightAlt", "Groq")
        });
        assert_eq!(tray.fix(), Some(Fix::OpenAccessibilitySettings));
        // Granted again; the hotkey is still down.
        tray.set_ambient(ambient("RightAlt", "Groq"));
        let menu = tray.menu();
        assert_eq!(
            label_of(&menu, id::STATUS).as_deref(),
            Some("! Relaunch Vocet to finish turning on Accessibility")
        );
        assert_eq!(
            label_of(&menu, id::FIX_RELAUNCH).as_deref(),
            Some("Relaunch Vocet")
        );
        assert!(label_of(&menu, id::FIX_KEY).is_none());
        assert_eq!(menu.icon, TrayIcon::Attention);
        assert_eq!(
            menu.tooltip,
            "Vocet: Relaunch Vocet to finish turning on Accessibility"
        );
    }

    /// Re-granted and the tap restarted: Ready at once.
    #[test]
    fn a_regrant_with_the_tap_restarted_is_ready() {
        let mut tray = ready("RightAlt", "Groq");
        tray.on_event(&error(DictationError::HotkeyFailed));
        tray.set_ambient(Ambient {
            problem: Some(ErrorCode::AccessibilityMissing),
            ..ambient("RightAlt", "Groq")
        });
        tray.set_ambient(ambient("RightAlt", "Groq"));
        tray.on_event(&UiEvent::HotkeyRestored);
        let menu = tray.menu();
        assert!(label_of(&menu, id::STATUS).unwrap().starts_with("✓ Ready"));
        assert_eq!(menu.icon, TrayIcon::Normal);
        assert!(!tray.relaunch_needed());
    }

    #[test]
    fn a_grant_that_was_never_missing_needs_no_relaunch() {
        let mut tray = ready("RightAlt", "Groq");
        tray.on_event(&UiEvent::StateChanged(Phase::Recording));
        tray.on_event(&UiEvent::StateChanged(Phase::Idle));
        assert!(!tray.relaunch_needed());
        assert!(
            label_of(&tray.menu(), id::STATUS)
                .unwrap()
                .starts_with("✓ Ready")
        );
    }

    #[test]
    fn a_microphone_denied_at_launch_shows_at_once() {
        let tray = TrayStatus::new(true);
        assert_eq!(tray.problem(), Some(ErrorCode::MicDenied));
        assert_eq!(tray.fix(), Some(Fix::OpenMicrophoneSettings));
        assert_eq!(TrayStatus::new(false).problem(), None);
    }

    /// Criterion 6: Relaunch Vocet is only the fix row, offered once the
    /// user went to System Settings for a grant.
    #[test]
    fn criterion_6_relaunch_replaces_the_grant_fix_after_it_was_opened() {
        let mut tray = TrayStatus::new(true);
        assert_eq!(tray.fix(), Some(Fix::OpenMicrophoneSettings));
        tray.grant_opened();
        assert_eq!(tray.fix(), Some(Fix::Relaunch));
        let menu = tray.menu();
        assert_eq!(
            label_of(&menu, id::FIX_RELAUNCH).as_deref(),
            Some("Relaunch Vocet")
        );
        // Never anywhere else.
        assert_eq!(
            ids(&menu)
                .iter()
                .filter(|id| id.contains("relaunch"))
                .count(),
            1
        );
        // A recording means the microphone works: the problem and the
        // relaunch go.
        tray.on_event(&UiEvent::StateChanged(Phase::Recording));
        assert_eq!(tray.fix(), None);
        assert!(!ids(&tray.menu()).contains(&"fix-relaunch".to_owned()));
        // A fix that isn't a grant stays itself.
        let mut tray = ready("RightAlt", "Groq");
        tray.on_event(&transcription_failed("Add your API key in Settings."));
        tray.grant_opened();
        assert_eq!(tray.fix(), Some(Fix::OpenSpeechSettings));
    }

    #[test]
    fn criterion_5_copy_last_is_disabled_until_a_dictation_succeeded() {
        let mut tray = ready("RightAlt", "Groq");
        assert_eq!(enabled(&tray.menu(), id::COPY_LAST), Some(false));
        assert!(tray.set_has_last(true));
        assert!(!tray.set_has_last(true));
        assert_eq!(enabled(&tray.menu(), id::COPY_LAST), Some(true));
    }

    /// U7 criterion 6: "History…" sits right below "Copy last dictation" in
    /// every state, and no item shows dictation text: their labels are fixed.
    #[test]
    fn u7_criterion_6_history_is_below_copy_last_in_every_state() {
        let mut problem = ready("RightAlt", "Groq");
        problem.on_event(&transcription_failed("Add your API key in Settings."));
        for tray in [ready("RightAlt", "Groq"), problem, TrayStatus::new(true)] {
            let listed = ids(&tray.menu());
            let copy = listed.iter().position(|id| id == id::COPY_LAST).unwrap();
            assert_eq!(listed[copy + 1], id::HISTORY);
            assert_eq!(enabled(&tray.menu(), id::HISTORY), Some(true));
            assert_eq!(
                label_of(&tray.menu(), id::HISTORY).as_deref(),
                Some("History…")
            );
        }
    }

    #[test]
    fn criterion_6_the_menu_has_no_reveal_log() {
        let tray = ready("RightAlt", "Groq");
        let all = tray.menu().outline();
        assert!(!all.iter().any(|(id, label)| {
            id.contains("reveal") || label.to_lowercase().contains("reveal")
        }));
    }

    #[test]
    fn criterion_4_the_menu_model_has_no_title_field() {
        // The tray's text title is never set: nothing in the model carries
        // one, only the icon and the tooltip.
        let menu = TrayStatus::new(true).menu();
        assert_eq!(menu.icon, TrayIcon::Attention);
        assert!(menu.tooltip.starts_with("Vocet: "));
    }

    #[test]
    fn keycap_labels_match_the_recorders() {
        for (spec, label) in [
            ("RightAlt", "Right ⌥"),
            ("LeftCtrl+LeftShift", "Left ⌃ Left ⇧"),
            ("Ctrl+Shift+Space", "⌃⇧Space"),
            ("F5", "F5"),
            ("", ""),
        ] {
            assert_eq!(keycap_label(spec), label, "{spec}");
        }
    }

    #[test]
    fn permission_errors_map_like_the_table() {
        let mut tray = TrayStatus::default();
        tray.on_event(&error(DictationError::PermissionMissing(
            Permission::Accessibility,
        )));
        assert_eq!(tray.fix(), Some(Fix::OpenAccessibilitySettings));
        tray.on_event(&error(DictationError::InsertFailed {
            reason: "no focus".to_owned(),
        }));
        assert_eq!(tray.problem(), Some(ErrorCode::PasteFailed));
    }
}
