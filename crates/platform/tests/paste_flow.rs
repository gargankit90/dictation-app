//! Tester's black-box checks of the M4.1 paste flow through the public API:
//! edge cases the unit tests don't cover (empty and huge transcripts,
//! back-to-back runs, a failed paste when the user copied meanwhile, the
//! layout lookup for several keyboards).
//! Run with `cargo nextest run -p dictation-platform --features testing`.
#![cfg(feature = "testing")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_platform::fakes::MemClipboard;
use dictation_platform::insert::{
    ANSI_V_KEY_CODE, ChordModifier, Os, PasteInserter, PasteKeys, Settle, paste_chord,
};
use dictation_ports::{Clipboard, ClipboardMarkers, InsertError, TextInserter};

#[cfg(test)]
mod support {
    use super::*;

    /// Records what the focused app would paste; optionally fails and/or
    /// has the user copy something while the keys are sent.
    #[derive(Clone, Default)]
    pub struct Keys {
        pub clipboard: Option<MemClipboard>,
        pub pasted: Arc<Mutex<Vec<Option<String>>>>,
        pub fail: Option<InsertError>,
        pub user_copies: Option<String>,
    }

    impl PasteKeys for Keys {
        fn paste(&self) -> Result<(), InsertError> {
            let clipboard = self.clipboard.as_ref().unwrap();
            self.pasted
                .lock()
                .unwrap()
                .push(clipboard.read_text().unwrap());
            if let Some(text) = &self.user_copies {
                clipboard.user_copies(text.clone());
            }
            self.fail.clone().map_or(Ok(()), Err)
        }
    }

    #[derive(Clone, Default)]
    pub struct NoWait(pub Arc<Mutex<u32>>);

    impl Settle for NoWait {
        fn settle(&self, _: Duration) {
            *self.0.lock().unwrap() += 1;
        }
    }

    pub fn flow(clipboard: &MemClipboard, keys: Keys) -> PasteInserter<MemClipboard, Keys, NoWait> {
        PasteInserter::new(
            clipboard.clone(),
            keys,
            NoWait::default(),
            Duration::from_millis(1),
        )
    }

    pub fn keys(clipboard: &MemClipboard) -> Keys {
        Keys {
            clipboard: Some(clipboard.clone()),
            ..Keys::default()
        }
    }
}
use support::{Keys, NoWait, flow, keys};

#[test]
fn an_empty_transcript_still_pastes_and_restores() {
    let clipboard = MemClipboard::with_text("mine");
    let k = keys(&clipboard);
    assert_eq!(flow(&clipboard, k.clone()).insert(""), Ok(()));
    assert_eq!(*k.pasted.lock().unwrap(), vec![Some(String::new())]);
    assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("mine"));
    assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
}

#[test]
fn a_very_long_unicode_transcript_goes_through_intact() {
    let text = "héllo wörld 日本語 🎤 ".repeat(50_000); // ~1 MB
    let clipboard = MemClipboard::with_text("mine");
    let k = keys(&clipboard);
    assert_eq!(flow(&clipboard, k.clone()).insert(&text), Ok(()));
    assert_eq!(k.pasted.lock().unwrap()[0].as_deref(), Some(text.as_str()));
    assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("mine"));
}

#[test]
fn the_text_on_the_clipboard_during_the_paste_carries_all_markers() {
    let clipboard = MemClipboard::with_text("mine");
    struct Probe(MemClipboard, Mutex<Option<(ClipboardMarkers, bool)>>);
    impl PasteKeys for Probe {
        fn paste(&self) -> Result<(), InsertError> {
            *self.1.lock().unwrap() = Some((self.0.markers(), self.0.session().is_some()));
            Ok(())
        }
    }
    let probe = Arc::new(Probe(clipboard.clone(), Mutex::new(None)));
    struct Shared(Arc<Probe>);
    impl PasteKeys for Shared {
        fn paste(&self) -> Result<(), InsertError> {
            self.0.paste()
        }
    }
    let inserter = PasteInserter::new(
        clipboard.clone(),
        Shared(probe.clone()),
        NoWait::default(),
        Duration::ZERO,
    );
    inserter.insert("x").unwrap();
    let (markers, has_session) = probe.1.lock().unwrap().unwrap();
    assert_eq!(markers, ClipboardMarkers::DICTATION);
    assert!(has_session);
}

#[test]
fn a_failed_paste_leaves_a_clipboard_the_user_changed_alone() {
    let clipboard = MemClipboard::with_text("old");
    let k = Keys {
        fail: Some(InsertError::Failed("boom".into())),
        user_copies: Some("copied during the paste".into()),
        ..keys(&clipboard)
    };
    let result = flow(&clipboard, k).insert("dictated");
    assert_eq!(result, Err(InsertError::Failed("boom".into())));
    assert_eq!(
        clipboard.read_text().unwrap().as_deref(),
        Some("copied during the paste")
    );
}

#[test]
fn a_failed_paste_on_an_empty_clipboard_leaves_the_transcript_on_it() {
    // P1 round 3 (Lead): a failed paste keeps the transcript on the
    // clipboard for the user to paste; it used to clear it again.
    let clipboard = MemClipboard::new();
    let k = Keys {
        fail: Some(InsertError::PermissionDenied),
        ..keys(&clipboard)
    };
    assert_eq!(
        flow(&clipboard, k).insert("dictated"),
        Err(InsertError::PermissionDenied)
    );
    assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("dictated"));
}

#[test]
fn an_unavailable_clipboard_never_pastes_and_never_leaks_the_text() {
    let clipboard = MemClipboard::with_text("old");
    clipboard.set_unavailable(true);
    let k = keys(&clipboard);
    let Err(InsertError::Failed(message)) = flow(&clipboard, k.clone()).insert("secret words")
    else {
        panic!("expected Failed")
    };
    assert!(!message.contains("secret words"));
    assert!(k.pasted.lock().unwrap().is_empty());
    clipboard.set_unavailable(false);
    assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("old"));
}

#[test]
fn back_to_back_inserts_each_restore_the_original() {
    let clipboard = MemClipboard::with_text("mine");
    let k = keys(&clipboard);
    let inserter = flow(&clipboard, k.clone());
    for text in ["one", "two", "three"] {
        assert_eq!(inserter.insert(text), Ok(()));
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("mine"));
    }
    let pasted: Vec<_> = k.pasted.lock().unwrap().iter().cloned().collect();
    assert_eq!(
        pasted,
        vec![Some("one".into()), Some("two".into()), Some("three".into())]
    );
}

#[test]
fn a_change_during_the_settle_delay_is_left_alone_on_every_run() {
    struct CopyDuring(MemClipboard);
    impl Settle for CopyDuring {
        fn settle(&self, _: Duration) {
            self.0.user_copies("user's newer copy");
        }
    }
    let clipboard = MemClipboard::with_text("old");
    let inserter = PasteInserter::new(
        clipboard.clone(),
        keys(&clipboard),
        CopyDuring(clipboard.clone()),
        Duration::from_millis(300),
    );
    assert_eq!(inserter.insert("dictated"), Ok(()));
    assert_eq!(
        clipboard.read_text().unwrap().as_deref(),
        Some("user's newer copy")
    );
}

/// A layout table from `(code, char)` pairs, as the macOS lookup gives with ⌘ held.
fn table(pairs: &'static [(u16, char)]) -> impl Fn(u16) -> Option<char> {
    move |code| pairs.iter().find(|(c, _)| *c == code).map(|(_, ch)| *ch)
}

#[test]
fn macos_chord_across_layouts() {
    // QWERTY and AZERTY (⌘ keeps V on the ANSI V position), Dvorak (V is
    // where QWERTY has '.'), Russian (⌘ gives Latin QWERTY).
    let qwerty = paste_chord(Os::MacOs, table(&[(9, 'v')]));
    let azerty = paste_chord(Os::MacOs, table(&[(9, 'v'), (12, 'a')]));
    let dvorak = paste_chord(Os::MacOs, table(&[(9, 'q'), (47, 'v')]));
    let russian = paste_chord(Os::MacOs, table(&[(9, 'v')]));
    let none = paste_chord(Os::MacOs, |_| None);
    for chord in [qwerty, azerty, russian, none] {
        assert_eq!(chord.modifier, ChordModifier::Command);
        assert_eq!(chord.key_code, ANSI_V_KEY_CODE);
    }
    assert_eq!(dvorak.modifier, ChordModifier::Command);
    assert_eq!(dvorak.key_code, 47);
}

#[test]
fn macos_chord_ignores_codes_past_127_and_uppercase_matches() {
    assert_eq!(
        paste_chord(Os::MacOs, table(&[(200, 'v')])).key_code,
        ANSI_V_KEY_CODE
    );
    assert_eq!(paste_chord(Os::MacOs, table(&[(3, 'V')])).key_code, 3);
    // Lowest matching code wins when two keys type v.
    assert_eq!(
        paste_chord(Os::MacOs, table(&[(40, 'v'), (20, 'v')])).key_code,
        20
    );
}

#[test]
fn windows_chord_never_consults_the_layout() {
    let chord = paste_chord(Os::Windows, |_| panic!("layout must not be read"));
    assert_eq!(chord.modifier, ChordModifier::Control);
    assert_eq!(chord.key_code, 0x56);
}
