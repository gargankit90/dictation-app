//! The menu-bar menu as Vocet's home (U5): what the tray's icon, tooltip and
//! menu show, as pure data the shell applies to a native menu.
//!
//! The menu, top to bottom: a status line (ready and which key and engine,
//! a download's progress, or the one problem), one fix row while there is a
//! problem, "Copy last dictation", Settings, Troubleshoot, Report a problem
//! (and Resume setup while onboarding is incomplete), then updates and Quit.
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
    /// The engine line under a ready status.
    pub const ENGINE: &str = "engine";
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
    /// "Settings…".
    pub const SETTINGS: &str = "settings";
    /// "Troubleshoot…".
    pub const TROUBLESHOOT: &str = "troubleshoot";
    /// "Report a problem…".
    pub const REPORT: &str = "report";
    /// "Resume setup…".
    pub const RESUME_SETUP: &str = "onboarding";
    /// "Check for updates…" (the updater's own id).
    pub const CHECK_UPDATES: &str = "check-updates";
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
    /// Settings, on the shortcut.
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

/// The one sentence the status line gives for `code`.
#[must_use]
pub fn problem_sentence(code: ErrorCode) -> &'static str {
    code.info().message
}

/// The keycap text for a stored hotkey, as Settings' recorder shows it:
/// `RightAlt` is `Right ⌥`, `Ctrl+Shift+Space` is `⌃⇧Space`. Anything
/// unrecognised is shown as written. Mirrors `keycapLabel` in
/// `src/hotkeyKeys.ts`.
#[must_use]
pub fn keycap_label(spec: &str) -> String {
    let labels: Vec<(String, Option<&'static str>)> = spec
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            let (side, rest) = if let Some(rest) = lower.strip_prefix("left") {
                (Some("Left"), rest.to_owned())
            } else if let Some(rest) = lower.strip_prefix("right") {
                (Some("Right"), rest.to_owned())
            } else {
                (None, lower)
            };
            let symbol = match rest.as_str() {
                "ctrl" | "control" => Some("⌃"),
                "alt" | "option" | "opt" => Some("⌥"),
                "shift" => Some("⇧"),
                "cmd" | "command" | "meta" | "super" | "win" => Some("⌘"),
                _ => None,
            };
            match symbol {
                Some(symbol) => (symbol.to_owned(), side),
                None => (part.to_owned(), None),
            }
        })
        .collect();
    if labels.iter().any(|(_, side)| side.is_some()) {
        labels
            .iter()
            .map(|(text, side)| match side {
                Some(side) => format!("{side} {text}"),
                None => text.clone(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        labels.iter().map(|(text, _)| text.as_str()).collect()
    }
}

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

/// A model downloading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    /// The model's display name.
    pub name: String,
    /// Percent done, 0 to 100.
    pub percent: u8,
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
    /// Whether a dictation succeeded in this run.
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
                self.hotkey_down = true;
                self.error = None;
            }
            UiEvent::Error(error) => self.error = Some(ErrorCode::of(error)),
            UiEvent::HotkeyRestored => self.hotkey_down = false,
            UiEvent::StateChanged(phase) => {
                self.recording = *phase == Phase::Recording;
                if self.recording {
                    // The microphone and the rest work after all.
                    self.error = None;
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
        self.ambient = ambient;
        true
    }

    /// Whether a dictation succeeded, which enables "Copy last dictation".
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

    /// The fix row's action for the current problem, if there is one.
    #[must_use]
    pub fn fix(&self) -> Option<Fix> {
        let fix = fix_for(self.problem()?);
        Some(if self.grant_opened && fix.is_grant() {
            Fix::Relaunch
        } else {
            fix
        })
    }

    /// What the tray shows now.
    #[must_use]
    pub fn menu(&self) -> MenuModel {
        // A runtime problem outranks a download; a download outranks what
        // the engine's state says (the model it waits for is coming).
        let runtime = self.runtime_problem();
        let download = if runtime.is_none() {
            self.ambient.download.as_ref()
        } else {
            None
        };
        let problem = if download.is_some() {
            None
        } else {
            self.problem()
        };
        let mut entries = Vec::new();
        let tooltip = if let Some(code) = problem {
            let sentence = problem_sentence(code);
            entries.push(Entry::disabled(id::STATUS, format!("! {sentence}")));
            if let Some(fix) = self.fix() {
                entries.push(Entry::item(fix.id(), fix.label()));
            }
            format!("{APP_NAME}: {sentence}")
        } else if let Some(download) = download {
            let text = format!("↓ Downloading {}, {}%", download.name, download.percent);
            entries.push(Entry::disabled(id::STATUS, text));
            format!("{APP_NAME}: downloading {}", download.name)
        } else {
            let key = keycap_label(&self.ambient.hotkey);
            entries.push(Entry::disabled(id::STATUS, format!("✓ Ready · Hold {key}")));
            entries.push(Entry::disabled(id::ENGINE, self.ambient.engine.clone()));
            format!("{APP_NAME}: Ready")
        };
        entries.push(Entry::Separator);
        entries.push(Entry::Item {
            id: id::COPY_LAST,
            label: "Copy last dictation".to_owned(),
            enabled: self.has_last,
            accelerator: None,
        });
        entries.push(Entry::Separator);
        entries.push(Entry::Item {
            id: id::SETTINGS,
            label: "Settings…".to_owned(),
            enabled: true,
            accelerator: Some("CmdOrCtrl+,"),
        });
        entries.push(Entry::item(id::TROUBLESHOOT, "Troubleshoot…"));
        entries.push(Entry::item(id::REPORT, "Report a problem…"));
        if self.ambient.setup_incomplete {
            entries.push(Entry::item(id::RESUME_SETUP, "Resume setup…"));
        }
        entries.push(Entry::Separator);
        entries.push(Entry::item(id::CHECK_UPDATES, "Check for updates…"));
        entries.push(Entry::Item {
            id: id::QUIT,
            label: format!("Quit {APP_NAME}"),
            enabled: true,
            accelerator: Some("CmdOrCtrl+Q"),
        });
        let icon = if problem.is_some() {
            TrayIcon::Attention
        } else if self.recording {
            TrayIcon::Recording
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

    /// U5 criterion 1: the order for a ready Mac with the on-device engine.
    #[test]
    fn criterion_1_ready_local_menu_in_order() {
        let tray = ready("RightAlt", "On this Mac (Parakeet English)");
        let menu = tray.menu();
        assert_eq!(
            menu.outline(),
            [
                ("status", "✓ Ready · Hold Right ⌥"),
                ("engine", "On this Mac (Parakeet English)"),
                ("-", ""),
                ("copy-last", "Copy last dictation"),
                ("-", ""),
                ("settings", "Settings…"),
                ("troubleshoot", "Troubleshoot…"),
                ("report", "Report a problem…"),
                ("-", ""),
                ("check-updates", "Check for updates…"),
                ("quit", "Quit Vocet"),
            ]
            .map(|(id, label)| (id.to_owned(), label.to_owned()))
        );
        assert_eq!(menu.icon, TrayIcon::Normal);
        assert_eq!(menu.tooltip, "Vocet: Ready");
        // The status lines can't be clicked.
        assert_eq!(enabled(&menu, id::STATUS), Some(false));
        assert_eq!(enabled(&menu, id::ENGINE), Some(false));
    }

    #[test]
    fn criterion_1_ready_with_groq() {
        let menu = ready("Ctrl+Shift+Space", "Groq").menu();
        assert_eq!(
            label_of(&menu, id::STATUS).as_deref(),
            Some("✓ Ready · Hold ⌃⇧Space")
        );
        assert_eq!(label_of(&menu, id::ENGINE).as_deref(), Some("Groq"));
    }

    #[test]
    fn criterion_1_downloading_shows_progress_and_no_fix() {
        let mut tray = ready("RightAlt", "On this Mac (Parakeet English)");
        tray.set_ambient(Ambient {
            download: Some(Download {
                name: "Parakeet English".to_owned(),
                percent: 42,
            }),
            problem: Some(ErrorCode::ModelMissing),
            ..ambient("RightAlt", "On this Mac (Parakeet English)")
        });
        let menu = tray.menu();
        assert_eq!(
            menu.outline().first().cloned(),
            Some((
                "status".to_owned(),
                "↓ Downloading Parakeet English, 42%".to_owned()
            ))
        );
        assert_eq!(&ids(&menu)[1], "-");
        assert_eq!(menu.icon, TrayIcon::Normal);
    }

    #[test]
    fn criterion_1_setup_incomplete_adds_resume_setup_after_report() {
        let mut tray = ready("RightAlt", "Groq");
        tray.set_ambient(Ambient {
            setup_incomplete: true,
            ..ambient("RightAlt", "Groq")
        });
        let listed = ids(&tray.menu());
        let report = listed.iter().position(|id| id == "report").unwrap();
        assert_eq!(listed[report + 1], "onboarding");
        assert_eq!(listed[report + 2], "-");
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
            assert_eq!(
                outline[0],
                ("status".to_owned(), format!("! {}", problem_sentence(code))),
                "{code}"
            );
            assert_eq!(outline[1], (fix_id.to_owned(), label.to_owned()), "{code}");
            assert_eq!(outline[2].0, "-", "{code}: exactly one fix row");
            assert_eq!(enabled(&menu, fix_id), Some(true), "{code}");
            assert_eq!(menu.icon, TrayIcon::Attention, "{code}");
            assert_eq!(
                menu.tooltip,
                format!("Vocet: {}", problem_sentence(code)),
                "{code}"
            );
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
            Some("! The provider rejected your API key.")
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
        assert_eq!(label_of(&menu, id::ENGINE).as_deref(), Some("OpenAI"));
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
