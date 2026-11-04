//! Inserting the transcript by paste, then giving the user their clipboard
//! back (`docs/ENGINEERING.md`, Clipboard).
//!
//! [`PasteInserter`] runs the flow over the [`Clipboard`] port and a
//! [`PasteKeys`] sender, so it is OS-neutral and tested with fakes:
//!
//! 0. if Secure Input is on (a password field has focus), stop with
//!    [`InsertError::SecureInput`] before touching anything: no paste, and
//!    the clipboard is never written (see [`SecureInputProbe`]);
//! 1. save everything on the clipboard ([`Clipboard::snapshot`]);
//! 2. ask again, then write the text with the privacy markers and a new
//!    session marker ([`Clipboard::write_dictation`]); the snapshot can
//!    take a while, and focus may have moved to a password field meanwhile;
//! 3. send the paste chord, asking a last time right before the keys go
//!    out ([`PasteKeys::paste_unless`]); if Secure Input came on after the
//!    write, the chord isn't sent and step 5 puts the clipboard back;
//! 4. wait a short settle delay, so the target app reads the clipboard;
//! 5. put the saved contents back only if our session marker is still on
//!    the clipboard ([`Clipboard::restore_snapshot_if_unchanged`]): if the
//!    user (or an app) copied something meanwhile, it's left alone.
//!
//! The probes shrink the race with the user's focus; they can't close it.
//! The OS has no "check focus and paste" in one step, so a password field
//! can still take focus in the few microseconds between the last probe and
//! the chord reaching the event queue (see [`PasteKeys::paste_unless`]).
//!
//! [`paste_chord`] picks the chord for the OS and keyboard layout. macOS
//! adapters: [`MacPasteboard`], [`MacPasteKeys`]. [`SystemInserter`] picks
//! this OS's.

#[cfg(target_os = "macos")]
mod macos;

use std::time::Duration;

use dictation_ports::{Clipboard, ClipboardError, InsertError, TextInserter};

#[cfg(target_os = "macos")]
pub use macos::{MacPasteKeys, MacPasteboard};

/// How long to wait after the paste chord before restoring the clipboard.
/// Apps read the pasteboard asynchronously; restoring too early would paste
/// the user's old contents instead of the transcript.
pub const DEFAULT_SETTLE: Duration = Duration::from_millis(300);

/// macOS's `kVK_ANSI_V`: where V is on a US keyboard, and the fallback when
/// the current layout can't be read.
pub const ANSI_V_KEY_CODE: u16 = 9;

/// Windows' `VK_V`. Windows sends Ctrl+V by virtual key, which already
/// follows the layout.
pub const WINDOWS_V_KEY: u16 = 0x56;

/// The modifier held for the paste chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChordModifier {
    /// ⌘ (macOS).
    Command,
    /// Ctrl (Windows, Linux).
    Control,
}

/// The key chord that pastes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteChord {
    /// The modifier to hold.
    pub modifier: ChordModifier,
    /// The key to press: a macOS virtual key code, or a Windows virtual
    /// key.
    pub key_code: u16,
}

/// Which OS's paste convention to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// ⌘ plus whichever key types `v` in the current layout.
    MacOs,
    /// Ctrl plus `VK_V`.
    Windows,
}

impl Os {
    /// The OS this build runs on, if it has a paste convention here.
    #[must_use]
    pub fn current() -> Option<Self> {
        if cfg!(target_os = "macos") {
            Some(Self::MacOs)
        } else if cfg!(target_os = "windows") {
            Some(Self::Windows)
        } else {
            None
        }
    }
}

/// The paste chord for `os`. On macOS, `layout` says which character a key
/// code types with ⌘ held in the current keyboard layout (`None` if it
/// types nothing); the first key that types `v` wins, so Dvorak uses its
/// own V key while Dvorak-QWERTY ⌘ and most non-Latin layouts (which switch
/// to QWERTY under ⌘) use the ANSI one. If no key types `v`, the ANSI V key
/// is the fallback. `layout` isn't called for Windows.
pub fn paste_chord(os: Os, layout: impl Fn(u16) -> Option<char>) -> PasteChord {
    match os {
        Os::MacOs => PasteChord {
            modifier: ChordModifier::Command,
            key_code: (0..128)
                .find(|&code| layout(code).is_some_and(|c| c.eq_ignore_ascii_case(&'v')))
                .unwrap_or(ANSI_V_KEY_CODE),
        },
        Os::Windows => PasteChord {
            modifier: ChordModifier::Control,
            key_code: WINDOWS_V_KEY,
        },
    }
}

/// Sends the paste chord to the focused app.
pub trait PasteKeys: Send + Sync {
    /// Presses and releases the paste chord.
    ///
    /// # Errors
    ///
    /// [`InsertError::PermissionDenied`] if the OS doesn't allow synthetic
    /// keys, [`InsertError::Failed`] otherwise.
    fn paste(&self) -> Result<(), InsertError>;

    /// Like [`paste`](Self::paste), but calls `blocked` as late as the
    /// adapter can, after all its slow preparation (permission check,
    /// keyboard layout lookup) and right before the keys go out, and sends
    /// nothing if it returns `true`.
    ///
    /// The default checks once, then pastes. An adapter that prepares first
    /// overrides it so the check sits after the preparation.
    ///
    /// The window that remains is between `blocked` returning and the OS
    /// delivering the chord to the focused app; no OS API closes it, so
    /// this narrows the race with a focus change rather than removing it.
    ///
    /// # Errors
    ///
    /// [`InsertError::SecureInput`] if `blocked` returned `true`; otherwise
    /// as [`paste`](Self::paste).
    fn paste_unless(&self, blocked: &dyn Fn() -> bool) -> Result<(), InsertError> {
        if blocked() {
            return Err(InsertError::SecureInput);
        }
        self.paste()
    }
}

/// Waits the settle delay. [`ThreadSettle`] sleeps; tests record instead.
pub trait Settle: Send + Sync {
    /// Waits `delay`.
    fn settle(&self, delay: Duration);
}

/// [`Settle`] with `std::thread::sleep`: the inserter runs on a blocking
/// thread (see [`TextInserter`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadSettle;

impl Settle for ThreadSettle {
    fn settle(&self, delay: Duration) {
        std::thread::sleep(delay);
    }
}

/// Answers "is Secure Input on right now?". Any `Fn() -> bool` works, so
/// the macOS adapter is `secure_input::is_secure_input_enabled` and tests
/// pass a closure.
pub trait SecureInputProbe: Send + Sync {
    /// Whether a password field (or an app with Secure Keyboard Entry) has
    /// Secure Input on.
    fn is_on(&self) -> bool;
}

impl<F: Fn() -> bool + Send + Sync> SecureInputProbe for F {
    fn is_on(&self) -> bool {
        self()
    }
}

/// The probe of a [`PasteInserter`] that doesn't check Secure Input: always
/// off. This is what [`PasteInserter::new`] gives; the system inserter
/// adds the real probe with [`PasteInserter::with_secure_input`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NeverSecure;

impl SecureInputProbe for NeverSecure {
    fn is_on(&self) -> bool {
        false
    }
}

/// The paste flow (see the module docs) over any clipboard and key sender.
#[derive(Debug)]
pub struct PasteInserter<C, K, S = ThreadSettle, P = NeverSecure> {
    clipboard: C,
    keys: K,
    settle: S,
    delay: Duration,
    secure_input: P,
}

impl<C, K, S> PasteInserter<C, K, S> {
    /// The flow with `settle` waiting `delay` between the paste and the
    /// restore. It doesn't check Secure Input; see
    /// [`with_secure_input`](Self::with_secure_input).
    pub fn new(clipboard: C, keys: K, settle: S, delay: Duration) -> Self {
        Self {
            clipboard,
            keys,
            settle,
            delay,
            secure_input: NeverSecure,
        }
    }
}

impl<C, K, S, P> PasteInserter<C, K, S, P> {
    /// The same flow, refusing to paste while `probe` says Secure Input is
    /// on.
    pub fn with_secure_input<Q: SecureInputProbe>(self, probe: Q) -> PasteInserter<C, K, S, Q> {
        PasteInserter {
            clipboard: self.clipboard,
            keys: self.keys,
            settle: self.settle,
            delay: self.delay,
            secure_input: probe,
        }
    }
}

fn failed(what: &str, error: &ClipboardError) -> InsertError {
    InsertError::Failed(format!("{what}: {error}"))
}

impl<C: Clipboard, K: PasteKeys, S: Settle, P: SecureInputProbe> TextInserter
    for PasteInserter<C, K, S, P>
{
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        // Before anything else: with a password field focused, don't put
        // the text on the clipboard (other apps can read it) or paste it.
        let blocked = || {
            let on = self.secure_input.is_on();
            if on {
                tracing::debug!("secure input is on; not pasting");
            }
            on
        };
        if blocked() {
            return Err(InsertError::SecureInput);
        }
        let saved = self
            .clipboard
            .snapshot()
            .map_err(|e| failed("could not save the clipboard", &e))?;
        // The snapshot copies every item and representation, so focus may
        // have moved to a password field since the first check. Ask again
        // before the text lands on the clipboard; nothing to undo yet.
        if blocked() {
            return Err(InsertError::SecureInput);
        }
        let session = self
            .clipboard
            .write_dictation(text)
            .map_err(|e| failed("could not write the clipboard", &e))?;
        // The last check is inside `paste_unless`, after the key sender's
        // own preparation. If it trips, nothing is sent and the restore
        // below puts the user's clipboard back (unless they copied since).
        let pasted = self.keys.paste_unless(&blocked);
        if pasted.is_ok() {
            self.settle.settle(self.delay);
        }
        match self
            .clipboard
            .restore_snapshot_if_unchanged(&session, &saved)
        {
            Ok(restored) => tracing::debug!(restored, "clipboard after paste"),
            // The text is pasted (or the paste error is reported below);
            // a failed restore isn't worth failing the dictation for.
            Err(error) => tracing::warn!(%error, "could not restore the clipboard"),
        }
        pasted
    }
}

/// This OS's inserter: [`PasteInserter`] over [`MacPasteboard`] and
/// [`MacPasteKeys`] on macOS. Elsewhere (until M-W.2) every insert fails
/// with [`InsertError::Failed`].
pub struct SystemInserter {
    #[cfg(target_os = "macos")]
    inner: PasteInserter<MacPasteboard, MacPasteKeys, ThreadSettle, fn() -> bool>,
}

impl SystemInserter {
    /// The inserter with the [`DEFAULT_SETTLE`] delay.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            inner: PasteInserter::new(
                MacPasteboard::new(),
                MacPasteKeys::new(),
                ThreadSettle,
                DEFAULT_SETTLE,
            )
            .with_secure_input(crate::secure_input::is_secure_input_enabled as fn() -> bool),
        }
    }
}

/// This OS's clipboard, for the menu's "Copy last dictation" (U5): text
/// only, written as if the user had copied it. Elsewhere (until M-W.2) the
/// write fails with [`ClipboardError::Unavailable`].
#[derive(Default)]
pub struct SystemClipboard {
    #[cfg(target_os = "macos")]
    inner: MacPasteboard,
}

impl SystemClipboard {
    /// The system clipboard.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            inner: MacPasteboard::new(),
        }
    }

    /// Replaces the clipboard's contents with `text`, with no markers.
    ///
    /// # Errors
    ///
    /// [`ClipboardError::Unavailable`] if it can't be written.
    pub fn copy(&self, text: &str) -> Result<(), ClipboardError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.write_text(text)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = text;
            Err(ClipboardError::Unavailable(
                "no clipboard adapter for this OS yet".into(),
            ))
        }
    }
}

impl Default for SystemInserter {
    fn default() -> Self {
        Self::new()
    }
}

impl TextInserter for SystemInserter {
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        #[cfg(target_os = "macos")]
        {
            self.inner.insert(text)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = text;
            Err(InsertError::Failed(
                "no paste adapter for this OS yet".into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use dictation_ports::{ClipboardMarkers, ClipboardSnapshot};

    use super::*;
    use crate::fakes::MemClipboard;

    /// Records pastes; can fail, and can run a hook (the user copying
    /// something) while "the app" reads the clipboard.
    #[derive(Clone, Default)]
    struct FakeKeys {
        pastes: Arc<Mutex<Vec<Option<String>>>>,
        clipboard: Option<MemClipboard>,
        fail: Option<InsertError>,
    }

    impl PasteKeys for FakeKeys {
        fn paste(&self) -> Result<(), InsertError> {
            // What the focused app would paste: the clipboard's text now.
            let seen = self
                .clipboard
                .as_ref()
                .and_then(|c| c.read_text().ok().flatten());
            self.pastes.lock().unwrap().push(seen);
            self.fail.clone().map_or(Ok(()), Err)
        }
    }

    /// Records each settle and runs an optional hook during it.
    #[derive(Clone, Default)]
    struct FakeSettle {
        waits: Arc<Mutex<Vec<Duration>>>,
        during: Option<Arc<dyn Fn() + Send + Sync>>,
    }

    impl Settle for FakeSettle {
        fn settle(&self, delay: Duration) {
            self.waits.lock().unwrap().push(delay);
            if let Some(hook) = &self.during {
                hook();
            }
        }
    }

    fn inserter(
        clipboard: &MemClipboard,
        keys: FakeKeys,
        settle: FakeSettle,
    ) -> PasteInserter<MemClipboard, FakeKeys, FakeSettle> {
        PasteInserter::new(clipboard.clone(), keys, settle, DEFAULT_SETTLE)
    }

    fn keys_for(clipboard: &MemClipboard) -> FakeKeys {
        FakeKeys {
            clipboard: Some(clipboard.clone()),
            ..FakeKeys::default()
        }
    }

    // Criterion 1, unchanged: the user's clipboard comes back.
    #[test]
    fn restores_the_clipboard_when_unchanged() {
        let clipboard = MemClipboard::with_text("the user's text");
        let keys = keys_for(&clipboard);
        let settle = FakeSettle::default();
        inserter(&clipboard, keys.clone(), settle.clone())
            .insert("dictated words")
            .unwrap();

        assert_eq!(
            *keys.pastes.lock().unwrap(),
            [Some("dictated words".to_owned())]
        );
        assert_eq!(*settle.waits.lock().unwrap(), [DEFAULT_SETTLE]);
        assert_eq!(
            clipboard.read_text().unwrap().as_deref(),
            Some("the user's text")
        );
        assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
        assert_eq!(clipboard.session(), None);
    }

    // M4.2 criterion 1: Secure Input on means nothing is pasted and the
    // clipboard is never touched.
    #[test]
    fn secure_input_on_pastes_nothing_and_leaves_the_clipboard_alone() {
        let clipboard = MemClipboard::with_text("the user's text");
        let keys = keys_for(&clipboard);
        let settle = FakeSettle::default();
        let result = inserter(&clipboard, keys.clone(), settle.clone())
            .with_secure_input(|| true)
            .insert("dictated words");

        assert_eq!(result, Err(InsertError::SecureInput));
        assert!(keys.pastes.lock().unwrap().is_empty());
        assert!(settle.waits.lock().unwrap().is_empty());
        assert_eq!(
            clipboard.read_text().unwrap().as_deref(),
            Some("the user's text")
        );
        assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
        assert_eq!(clipboard.session(), None);
    }

    #[test]
    fn secure_input_is_checked_before_the_clipboard_is_used() {
        let clipboard = MemClipboard::with_text("old");
        clipboard.set_unavailable(true);
        let result = inserter(&clipboard, keys_for(&clipboard), FakeSettle::default())
            .with_secure_input(|| true)
            .insert("words");
        // Not "could not save the clipboard".
        assert_eq!(result, Err(InsertError::SecureInput));
    }

    #[test]
    fn secure_input_off_pastes_as_usual_and_is_asked_on_every_insert() {
        let clipboard = MemClipboard::with_text("old");
        let keys = keys_for(&clipboard);
        let on = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let probe = Arc::clone(&on);
        let flow = inserter(&clipboard, keys.clone(), FakeSettle::default())
            .with_secure_input(move || probe.load(std::sync::atomic::Ordering::SeqCst));

        assert_eq!(flow.insert("first"), Err(InsertError::SecureInput));
        on.store(false, std::sync::atomic::Ordering::SeqCst);
        flow.insert("second").unwrap();

        assert_eq!(*keys.pastes.lock().unwrap(), [Some("second".to_owned())]);
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("old"));
    }

    #[test]
    fn an_empty_clipboard_is_left_empty() {
        let clipboard = MemClipboard::new();
        inserter(&clipboard, keys_for(&clipboard), FakeSettle::default())
            .insert("words")
            .unwrap();
        assert_eq!(clipboard.read_text().unwrap(), None);
    }

    // Criterion 1, changed: a copy made during the paste is kept.
    #[test]
    fn leaves_the_clipboard_alone_when_something_else_changed_it() {
        let clipboard = MemClipboard::with_text("old");
        let during = clipboard.clone();
        let settle = FakeSettle {
            during: Some(Arc::new(move || during.user_copies("copied meanwhile"))),
            ..FakeSettle::default()
        };
        inserter(&clipboard, keys_for(&clipboard), settle)
            .insert("dictated")
            .unwrap();
        assert_eq!(
            clipboard.read_text().unwrap().as_deref(),
            Some("copied meanwhile")
        );
    }

    #[test]
    fn even_the_same_text_copied_meanwhile_counts_as_a_change() {
        let clipboard = MemClipboard::with_text("old");
        let during = clipboard.clone();
        let settle = FakeSettle {
            during: Some(Arc::new(move || during.user_copies("dictated"))),
            ..FakeSettle::default()
        };
        inserter(&clipboard, keys_for(&clipboard), settle)
            .insert("dictated")
            .unwrap();
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("dictated"));
        assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
    }

    // Criterion 2.
    #[test]
    fn the_dictation_write_carries_all_three_markers_and_a_session() {
        let clipboard = MemClipboard::with_text("old");
        let seen = Arc::new(Mutex::new(None));
        let (probe, record) = (clipboard.clone(), Arc::clone(&seen));
        let settle = FakeSettle {
            during: Some(Arc::new(move || {
                *record.lock().unwrap() = Some((probe.markers(), probe.session()));
            })),
            ..FakeSettle::default()
        };
        inserter(&clipboard, keys_for(&clipboard), settle)
            .insert("dictated")
            .unwrap();
        let (markers, session) = seen.lock().unwrap().clone().unwrap();
        assert_eq!(markers, ClipboardMarkers::DICTATION);
        assert!(markers.transient && markers.concealed && markers.auto_generated);
        assert!(session.is_some());
    }

    #[test]
    fn a_failed_paste_restores_at_once_and_reports_the_error() {
        let clipboard = MemClipboard::with_text("old");
        let keys = FakeKeys {
            fail: Some(InsertError::PermissionDenied),
            ..keys_for(&clipboard)
        };
        let settle = FakeSettle::default();
        let result = inserter(&clipboard, keys, settle.clone()).insert("dictated");
        assert_eq!(result, Err(InsertError::PermissionDenied));
        assert!(
            settle.waits.lock().unwrap().is_empty(),
            "no settle without a paste"
        );
        assert_eq!(clipboard.read_text().unwrap().as_deref(), Some("old"));
    }

    #[test]
    fn an_unavailable_clipboard_fails_before_pasting() {
        let clipboard = MemClipboard::with_text("old");
        clipboard.set_unavailable(true);
        let keys = keys_for(&clipboard);
        let result = inserter(&clipboard, keys.clone(), FakeSettle::default()).insert("dictated");
        let Err(InsertError::Failed(message)) = result else {
            panic!("{result:?}")
        };
        assert!(message.starts_with("could not save the clipboard"));
        assert!(!message.contains("dictated"));
        assert!(keys.pastes.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failed_restore_still_reports_the_paste() {
        let clipboard = MemClipboard::with_text("old");
        let during = clipboard.clone();
        let settle = FakeSettle {
            during: Some(Arc::new(move || during.set_unavailable(true))),
            ..FakeSettle::default()
        };
        assert_eq!(
            inserter(&clipboard, keys_for(&clipboard), settle).insert("dictated"),
            Ok(())
        );
    }

    /// A [`MemClipboard`] that runs a hook after a snapshot or a dictation
    /// write: the user moving focus to a password field mid-flow.
    #[derive(Clone, Default)]
    struct HookClipboard {
        inner: MemClipboard,
        after_snapshot: Option<Arc<dyn Fn() + Send + Sync>>,
        after_write: Option<Arc<dyn Fn() + Send + Sync>>,
    }

    impl Clipboard for HookClipboard {
        fn read_text(&self) -> Result<Option<String>, ClipboardError> {
            self.inner.read_text()
        }
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            self.inner.write_text(text)
        }
        fn write_dictation(
            &self,
            text: &str,
        ) -> Result<dictation_ports::SessionMarker, ClipboardError> {
            let session = self.inner.write_dictation(text);
            if let Some(hook) = &self.after_write {
                hook();
            }
            session
        }
        fn restore_if_unchanged(
            &self,
            session: &dictation_ports::SessionMarker,
            previous: Option<&str>,
        ) -> Result<bool, ClipboardError> {
            self.inner.restore_if_unchanged(session, previous)
        }
        fn snapshot(&self) -> Result<ClipboardSnapshot, ClipboardError> {
            let snapshot = self.inner.snapshot();
            if let Some(hook) = &self.after_snapshot {
                hook();
            }
            snapshot
        }
        fn restore_snapshot_if_unchanged(
            &self,
            session: &dictation_ports::SessionMarker,
            previous: &ClipboardSnapshot,
        ) -> Result<bool, ClipboardError> {
            self.inner.restore_snapshot_if_unchanged(session, previous)
        }
    }

    /// A key sender whose preparation (permission check, layout lookup)
    /// runs a hook before it asks `blocked`, like [`MacPasteKeys`].
    #[derive(Clone, Default)]
    struct PreparingKeys {
        prepare: Option<Arc<dyn Fn() + Send + Sync>>,
        sent: Arc<Mutex<u32>>,
    }

    impl PasteKeys for PreparingKeys {
        fn paste(&self) -> Result<(), InsertError> {
            self.paste_unless(&|| false)
        }
        fn paste_unless(&self, blocked: &dyn Fn() -> bool) -> Result<(), InsertError> {
            if let Some(prepare) = &self.prepare {
                prepare();
            }
            if blocked() {
                return Err(InsertError::SecureInput);
            }
            *self.sent.lock().unwrap() += 1;
            Ok(())
        }
    }

    fn flag() -> Arc<std::sync::atomic::AtomicBool> {
        Arc::new(std::sync::atomic::AtomicBool::new(false))
    }

    fn turn_on(flag: &Arc<std::sync::atomic::AtomicBool>) -> Arc<dyn Fn() + Send + Sync> {
        let flag = Arc::clone(flag);
        Arc::new(move || flag.store(true, std::sync::atomic::Ordering::SeqCst))
    }

    fn reads(flag: &Arc<std::sync::atomic::AtomicBool>) -> impl Fn() -> bool + Send + Sync + use<> {
        let flag = Arc::clone(flag);
        move || flag.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn assert_untouched(clipboard: &MemClipboard) {
        assert_eq!(
            clipboard.read_text().unwrap().as_deref(),
            Some("the user's text")
        );
        assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
        assert_eq!(clipboard.session(), None);
    }

    // M4.2 round 1, M1: focus moves to a password field during the snapshot.
    #[test]
    fn secure_input_turning_on_during_the_snapshot_writes_and_pastes_nothing() {
        let inner = MemClipboard::with_text("the user's text");
        let on = flag();
        let clipboard = HookClipboard {
            inner: inner.clone(),
            after_snapshot: Some(turn_on(&on)),
            ..HookClipboard::default()
        };
        let keys = keys_for(&inner);
        let settle = FakeSettle::default();
        let result = PasteInserter::new(clipboard, keys.clone(), settle.clone(), DEFAULT_SETTLE)
            .with_secure_input(reads(&on))
            .insert("dictated words");

        assert_eq!(result, Err(InsertError::SecureInput));
        assert!(keys.pastes.lock().unwrap().is_empty());
        assert!(settle.waits.lock().unwrap().is_empty());
        assert_untouched(&inner);
    }

    // ... after the dictation is on the clipboard.
    #[test]
    fn secure_input_turning_on_after_the_write_sends_no_chord_and_restores() {
        let inner = MemClipboard::with_text("the user's text");
        let on = flag();
        let clipboard = HookClipboard {
            inner: inner.clone(),
            after_write: Some(turn_on(&on)),
            ..HookClipboard::default()
        };
        let keys = keys_for(&inner);
        let settle = FakeSettle::default();
        let result = PasteInserter::new(clipboard, keys.clone(), settle.clone(), DEFAULT_SETTLE)
            .with_secure_input(reads(&on))
            .insert("dictated words");

        assert_eq!(result, Err(InsertError::SecureInput));
        assert!(keys.pastes.lock().unwrap().is_empty(), "no paste");
        assert!(settle.waits.lock().unwrap().is_empty());
        assert_untouched(&inner);
    }

    // ... while the key sender prepares (the layout lookup), the last gap.
    #[test]
    fn secure_input_turning_on_before_the_chord_sends_nothing_and_restores() {
        let inner = MemClipboard::with_text("the user's text");
        let on = flag();
        let keys = PreparingKeys {
            prepare: Some(turn_on(&on)),
            ..PreparingKeys::default()
        };
        let settle = FakeSettle::default();
        let result =
            PasteInserter::new(inner.clone(), keys.clone(), settle.clone(), DEFAULT_SETTLE)
                .with_secure_input(reads(&on))
                .insert("dictated words");

        assert_eq!(result, Err(InsertError::SecureInput));
        assert_eq!(*keys.sent.lock().unwrap(), 0, "no chord");
        assert!(settle.waits.lock().unwrap().is_empty());
        assert_untouched(&inner);
    }

    // The restore after an abort still never overwrites a newer user copy.
    #[test]
    fn an_abort_after_the_write_keeps_what_the_user_copied_since() {
        let inner = MemClipboard::with_text("the user's text");
        let on = flag();
        let (copy, flip) = (inner.clone(), turn_on(&on));
        let clipboard = HookClipboard {
            inner: inner.clone(),
            after_write: Some(Arc::new(move || {
                copy.user_copies("copied meanwhile");
                flip();
            })),
            ..HookClipboard::default()
        };
        let keys = keys_for(&inner);
        let result = PasteInserter::new(
            clipboard,
            keys.clone(),
            FakeSettle::default(),
            DEFAULT_SETTLE,
        )
        .with_secure_input(reads(&on))
        .insert("dictated words");

        assert_eq!(result, Err(InsertError::SecureInput));
        assert!(keys.pastes.lock().unwrap().is_empty());
        assert_eq!(
            inner.read_text().unwrap().as_deref(),
            Some("copied meanwhile")
        );
    }

    // The next attempt, with the password field gone, pastes as usual.
    #[test]
    fn the_next_insert_after_an_abort_pastes_normally() {
        let inner = MemClipboard::with_text("the user's text");
        let on = flag();
        let clipboard = HookClipboard {
            inner: inner.clone(),
            after_write: Some(turn_on(&on)),
            ..HookClipboard::default()
        };
        let keys = keys_for(&inner);
        let flow = PasteInserter::new(
            clipboard,
            keys.clone(),
            FakeSettle::default(),
            DEFAULT_SETTLE,
        )
        .with_secure_input(reads(&on));
        assert_eq!(flow.insert("first"), Err(InsertError::SecureInput));

        on.store(false, std::sync::atomic::Ordering::SeqCst);
        // A fresh flow over the same clipboard, without the hook.
        let flow = PasteInserter::new(
            inner.clone(),
            keys.clone(),
            FakeSettle::default(),
            DEFAULT_SETTLE,
        )
        .with_secure_input(reads(&on));
        flow.insert("second").unwrap();

        assert_eq!(*keys.pastes.lock().unwrap(), [Some("second".to_owned())]);
        assert_untouched(&inner);
    }

    /// A clipboard that saves and restores whole items, like the macOS one.
    #[derive(Default)]
    struct ItemsClipboard {
        inner: MemClipboard,
        restored: Mutex<Option<ClipboardSnapshot>>,
    }

    impl Clipboard for ItemsClipboard {
        fn read_text(&self) -> Result<Option<String>, ClipboardError> {
            self.inner.read_text()
        }
        fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
            self.inner.write_text(text)
        }
        fn write_dictation(
            &self,
            text: &str,
        ) -> Result<dictation_ports::SessionMarker, ClipboardError> {
            self.inner.write_dictation(text)
        }
        fn restore_if_unchanged(
            &self,
            session: &dictation_ports::SessionMarker,
            previous: Option<&str>,
        ) -> Result<bool, ClipboardError> {
            self.inner.restore_if_unchanged(session, previous)
        }
        fn snapshot(&self) -> Result<ClipboardSnapshot, ClipboardError> {
            Ok(ClipboardSnapshot::Items(vec![
                dictation_ports::ClipboardItem {
                    representations: vec![("public.png".into(), vec![0x89, b'P'])],
                },
            ]))
        }
        fn restore_snapshot_if_unchanged(
            &self,
            _session: &dictation_ports::SessionMarker,
            previous: &ClipboardSnapshot,
        ) -> Result<bool, ClipboardError> {
            *self.restored.lock().unwrap() = Some(previous.clone());
            Ok(true)
        }
    }

    #[test]
    fn whole_snapshots_go_back_through_the_snapshot_restore() {
        let clipboard = ItemsClipboard::default();
        let inserter = PasteInserter::new(
            clipboard,
            FakeKeys::default(),
            FakeSettle::default(),
            DEFAULT_SETTLE,
        );
        inserter.insert("dictated").unwrap();
        let restored = inserter.clipboard.restored.lock().unwrap().clone().unwrap();
        assert_eq!(
            restored,
            ClipboardSnapshot::Items(vec![dictation_ports::ClipboardItem {
                representations: vec![("public.png".into(), vec![0x89, b'P'])],
            }])
        );
    }

    fn layout(keys: &[(u16, char)]) -> impl Fn(u16) -> Option<char> {
        let map: HashMap<u16, char> = keys.iter().copied().collect();
        move |code| map.get(&code).copied()
    }

    // Criterion 3: the chord for the OS and layout.
    #[test]
    fn macos_uses_command_and_the_key_that_types_v() {
        // US QWERTY (and AZERTY, QWERTZ): V is kVK_ANSI_V.
        let qwerty = layout(&[(0, 'a'), (8, 'c'), (9, 'v'), (47, '.')]);
        assert_eq!(
            paste_chord(Os::MacOs, qwerty),
            PasteChord {
                modifier: ChordModifier::Command,
                key_code: ANSI_V_KEY_CODE
            }
        );
        // AZERTY keeps V where QWERTY has it, but A/Q and Z/W move.
        let azerty = layout(&[(0, 'q'), (12, 'a'), (6, 'w'), (13, 'z'), (9, 'v')]);
        assert_eq!(paste_chord(Os::MacOs, azerty).key_code, 9);
        // Dvorak: V is on the ANSI period key (kVK_ANSI_Period = 47), and
        // the ANSI V key types k.
        let dvorak = layout(&[(9, 'k'), (47, 'v'), (34, 'c')]);
        assert_eq!(paste_chord(Os::MacOs, dvorak).key_code, 47);
        // Uppercase counts too (a layout table may report it with ⌘).
        assert_eq!(paste_chord(Os::MacOs, layout(&[(30, 'V')])).key_code, 30);
    }

    #[test]
    fn macos_falls_back_to_ansi_v_when_no_key_types_v() {
        // e.g. a non-Latin layout whose table has no Latin letters.
        let russian = layout(&[(9, 'м'), (8, 'с')]);
        assert_eq!(paste_chord(Os::MacOs, russian).key_code, ANSI_V_KEY_CODE);
        assert_eq!(paste_chord(Os::MacOs, |_| None).key_code, ANSI_V_KEY_CODE);
    }

    #[test]
    fn windows_uses_control_and_vk_v_without_a_layout() {
        let chord = paste_chord(Os::Windows, |_| panic!("not consulted on Windows"));
        assert_eq!(
            chord,
            PasteChord {
                modifier: ChordModifier::Control,
                key_code: WINDOWS_V_KEY
            }
        );
    }

    #[test]
    fn current_os_has_a_convention_on_mac_and_windows() {
        let expected = if cfg!(target_os = "macos") {
            Some(Os::MacOs)
        } else if cfg!(target_os = "windows") {
            Some(Os::Windows)
        } else {
            None
        };
        assert_eq!(Os::current(), expected);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn system_inserter_fails_without_an_adapter() {
        assert!(matches!(
            SystemInserter::new().insert("x"),
            Err(InsertError::Failed(_))
        ));
    }

    #[test]
    fn thread_settle_waits_at_least_the_delay() {
        let started = std::time::Instant::now();
        ThreadSettle.settle(Duration::from_millis(1));
        assert!(started.elapsed() >= Duration::from_millis(1));
    }
}
