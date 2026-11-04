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
//!    write, the chord isn't sent and the clipboard goes back at once;
//! 4. leave the transcript on the clipboard for [`RESTORE_DELAY`] after the
//!    chord, on a timer ([`Later`]; the app's runs on a thread of its own,
//!    so nothing waits for it), because the target reads the clipboard
//!    whenever its main thread gets to the ⌘V (#239);
//! 5. then put the saved contents back only if our write is still on the
//!    clipboard ([`Clipboard::restore_snapshot_if_unchanged`]: its session
//!    token and change count). If the user (or an app) copied something
//!    meanwhile, it's left alone and the saved contents are dropped: the
//!    newer copy wins.
//!
//! A dictation that starts while a restore is still pending does that
//! restore first, under the same rule, and only then saves the clipboard,
//! so the user's contents are never lost and never replaced by a saved
//! transcript. If the chord can't be sent ([`InsertError::PermissionDenied`]
//! or [`InsertError::Failed`]), nothing is restored: the transcript stays on
//! the clipboard, and the error the user sees says so. Nothing in this flow
//! reads the target app (no Accessibility queries).
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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use dictation_ports::{
    Clipboard, ClipboardError, ClipboardSnapshot, InsertError, SessionMarker, TextInserter,
};

#[cfg(target_os = "macos")]
pub use macos::{MacPasteKeys, MacPasteboard};

/// How long the transcript stays on the clipboard after the paste chord
/// before the user's clipboard goes back. The target reads the clipboard
/// when its main thread gets to the ⌘V, not when it is posted: the slowest
/// read measured was about 1070 ms after the chord (TextEdit, the first
/// paste after a relaunch with a Ctrl+Shift+A hotkey, #239), where the old
/// 300 ms pasted the user's old clipboard instead. A target that reads
/// later than this still pastes the old clipboard; none did in testing.
pub const RESTORE_DELAY: Duration = Duration::from_millis(1500);

/// The old fixed settle delay, kept for callers that pass their own; the
/// app uses [`RESTORE_DELAY`].
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

/// Runs the clipboard restore later. Every [`Settle`] is one that waits
/// inline (tests, and callers that want the old blocking flow);
/// [`ThreadLater`] runs it on a thread of its own.
pub trait Later: Send + Sync {
    /// Runs `job` after `delay`.
    fn after(&self, delay: Duration, job: Box<dyn FnOnce() + Send>);
}

impl<S: Settle> Later for S {
    fn after(&self, delay: Duration, job: Box<dyn FnOnce() + Send>) {
        self.settle(delay);
        job();
    }
}

/// [`Later`] on a new thread that sleeps first, so the event loop, the
/// hotkey and the next dictation never wait for a restore.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadLater;

impl Later for ThreadLater {
    fn after(&self, delay: Duration, job: Box<dyn FnOnce() + Send>) {
        let spawned = std::thread::Builder::new()
            .name("clipboard-restore".into())
            .spawn(move || {
                std::thread::sleep(delay);
                job();
            });
        if let Err(error) = spawned {
            // The restore stays pending; the next dictation does it.
            tracing::warn!(%error, "could not schedule the clipboard restore");
        }
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

/// A restore that hasn't run yet: the user's saved clipboard, owed back
/// if the clipboard still holds the write that made `session`.
#[derive(Debug)]
struct Pending {
    id: u64,
    session: SessionMarker,
    saved: ClipboardSnapshot,
    /// When the timer is due: `RESTORE_DELAY` after the chord.
    due: Instant,
}

/// The paste flow (see the module docs) over any clipboard and key sender.
pub struct PasteInserter<C, K, S = ThreadSettle, P = NeverSecure> {
    clipboard: Arc<C>,
    keys: K,
    later: S,
    delay: Duration,
    secure_input: P,
    /// The restore still owed, if any. Taken and done under this lock, by
    /// the timer or by the next dictation, never both.
    pending: Arc<Mutex<Option<Pending>>>,
    next_id: AtomicU64,
    /// Held for the whole of an insert, so [`TextInserter::finish`] waits
    /// for one still pasting.
    busy: Mutex<()>,
    /// Set by [`TextInserter::finish`]: no insert after it.
    closed: AtomicBool,
}

impl<C, K, S, P> std::fmt::Debug for PasteInserter<C, K, S, P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasteInserter")
            .field("delay", &self.delay)
            .finish_non_exhaustive()
    }
}

impl<C, K, S> PasteInserter<C, K, S> {
    /// The flow with `later` running the restore `delay` after the chord
    /// (a [`Settle`] waits inline; [`ThreadLater`] doesn't). It doesn't
    /// check Secure Input; see [`with_secure_input`](Self::with_secure_input).
    pub fn new(clipboard: C, keys: K, later: S, delay: Duration) -> Self {
        Self {
            clipboard: Arc::new(clipboard),
            keys,
            later,
            delay,
            secure_input: NeverSecure,
            pending: Arc::new(Mutex::new(None)),
            next_id: AtomicU64::new(0),
            busy: Mutex::new(()),
            closed: AtomicBool::new(false),
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
            later: self.later,
            delay: self.delay,
            secure_input: probe,
            pending: self.pending,
            next_id: self.next_id,
            busy: self.busy,
            closed: self.closed,
        }
    }
}

fn lock(pending: &Mutex<Option<Pending>>) -> MutexGuard<'_, Option<Pending>> {
    // A panic elsewhere can't leave an Option half written.
    pending.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Puts `saved` back if the clipboard still holds the write that made
/// `session`; else leaves it alone (the newer copy wins).
fn restore<C: Clipboard + ?Sized>(
    clipboard: &C,
    session: &SessionMarker,
    saved: &ClipboardSnapshot,
) {
    match clipboard.restore_snapshot_if_unchanged(session, saved) {
        Ok(restored) => tracing::debug!(restored, "clipboard after paste"),
        // The text is pasted (or the paste error is reported); a failed
        // restore isn't worth failing the dictation for.
        Err(error) => tracing::warn!(%error, "could not restore the clipboard"),
    }
}

fn failed(what: &str, error: &ClipboardError) -> InsertError {
    InsertError::Failed(format!("{what}: {error}"))
}

impl<C, K, S, P> TextInserter for PasteInserter<C, K, S, P>
where
    C: Clipboard + 'static,
    K: PasteKeys,
    S: Later,
    P: SecureInputProbe,
{
    fn insert(&self, text: &str) -> Result<(), InsertError> {
        let _busy = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
        if self.closed.load(Ordering::SeqCst) {
            return Err(InsertError::Failed("the app is quitting".into()));
        }
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
        // An earlier paste's restore first, so what is saved next is the
        // user's clipboard, never that paste's transcript.
        self.finish_pending();
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
        // own preparation. If it trips, nothing is sent and the clipboard
        // goes back at once (unless the user copied since).
        match self.keys.paste_unless(&blocked) {
            Ok(()) => {
                self.restore_later(session, saved);
                Ok(())
            }
            Err(InsertError::SecureInput) => {
                restore(&*self.clipboard, &session, &saved);
                Err(InsertError::SecureInput)
            }
            Err(error) => {
                // The chord didn't go out: keep the transcript where the
                // user can paste it (the error shown says so).
                tracing::warn!("the paste failed; the transcript stays on the clipboard");
                Err(error)
            }
        }
    }

    /// At quit or relaunch: waits for an insert still pasting, refuses
    /// any later one, then does the restore still owed, if any, at its
    /// time (never earlier: the target may not have read the transcript
    /// yet), under the same rule as the timer. Blocks at most
    /// [`RESTORE_DELAY`] past the last chord.
    fn finish(&self) {
        self.closed.store(true, Ordering::SeqCst);
        let _busy = self.busy.lock().unwrap_or_else(PoisonError::into_inner);
        let mut slot = lock(&self.pending);
        if let Some(owed) = slot.take() {
            std::thread::sleep(owed.due.saturating_duration_since(Instant::now()));
            restore(&*self.clipboard, &owed.session, &owed.saved);
            tracing::info!("restored the clipboard before quitting");
        }
    }
}

impl<C, K, S, P> PasteInserter<C, K, S, P>
where
    C: Clipboard + 'static,
    S: Later,
{
    /// Does the restore still owed, if any, now: a new dictation is about
    /// to save the clipboard.
    fn finish_pending(&self) {
        let mut slot = lock(&self.pending);
        if let Some(owed) = slot.take() {
            restore(&*self.clipboard, &owed.session, &owed.saved);
        }
    }

    /// Steps 4 and 5: owes `saved` back and has `later` restore it after
    /// the delay, unless the next dictation did already.
    fn restore_later(&self, session: SessionMarker, saved: ClipboardSnapshot) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let due = Instant::now() + self.delay;
        *lock(&self.pending) = Some(Pending {
            id,
            session,
            saved,
            due,
        });
        let (clipboard, pending) = (Arc::clone(&self.clipboard), Arc::clone(&self.pending));
        self.later.after(
            self.delay,
            Box::new(move || {
                // Under the lock, so a dictation starting now waits for the
                // restore before it saves the clipboard.
                let mut slot = lock(&pending);
                if slot.as_ref().is_some_and(|owed| owed.id == id)
                    && let Some(owed) = slot.take()
                {
                    restore(&*clipboard, &owed.session, &owed.saved);
                }
            }),
        );
    }
}

/// This OS's inserter: [`PasteInserter`] over [`MacPasteboard`] and
/// [`MacPasteKeys`] on macOS. Elsewhere (until M-W.2) every insert fails
/// with [`InsertError::Failed`].
pub struct SystemInserter {
    #[cfg(target_os = "macos")]
    inner: PasteInserter<MacPasteboard, MacPasteKeys, ThreadLater, fn() -> bool>,
}

impl SystemInserter {
    /// The inserter: the restore [`RESTORE_DELAY`] after the chord, on a
    /// thread of its own.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            inner: PasteInserter::new(
                MacPasteboard::new(),
                MacPasteKeys::new(),
                ThreadLater,
                RESTORE_DELAY,
            )
            .with_secure_input(crate::secure_input::is_secure_input_enabled as fn() -> bool),
        }
    }
}

impl Default for SystemInserter {
    fn default() -> Self {
        Self::new()
    }
}

impl TextInserter for SystemInserter {
    fn finish(&self) {
        #[cfg(target_os = "macos")]
        self.inner.finish();
    }

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
    fn a_failed_paste_keeps_the_transcript_and_reports_the_error() {
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
        assert_eq!(
            clipboard.read_text().unwrap().as_deref(),
            Some("dictated"),
            "P1: the transcript stays"
        );
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

    // --- P1 round 3: a fixed RESTORE_DELAY on a timer ----------------------

    type Job = (Duration, Box<dyn FnOnce() + Send>);

    /// A fake clock with a timer: [`Later`] jobs wait until the test runs
    /// the clock past their time. Time starts at 0, when the chord is sent.
    #[derive(Clone, Default)]
    struct Timeline {
        now: Arc<Mutex<Duration>>,
        jobs: Arc<Mutex<Vec<Job>>>,
        delays: Arc<Mutex<Vec<Duration>>>,
    }

    impl Later for Timeline {
        fn after(&self, delay: Duration, job: Box<dyn FnOnce() + Send>) {
            let due = *self.now.lock().unwrap() + delay;
            self.delays.lock().unwrap().push(delay);
            self.jobs.lock().unwrap().push((due, job));
        }
    }

    impl Timeline {
        /// Runs every job due by `t`, in order, and moves the clock to `t`.
        fn run_until(&self, t: Duration) {
            loop {
                let next = {
                    let mut jobs = self.jobs.lock().unwrap();
                    jobs.sort_by_key(|(due, _)| *due);
                    if jobs.first().is_some_and(|(due, _)| *due <= t) {
                        Some(jobs.remove(0))
                    } else {
                        None
                    }
                };
                let Some((due, job)) = next else { break };
                *self.now.lock().unwrap() = due;
                job();
            }
            *self.now.lock().unwrap() = t;
        }

        fn ms(&self, ms: u64) {
            self.run_until(Duration::from_millis(ms));
        }
    }

    const USER: &str = "the user's text";
    const DICTATED: &str = "dictated words";

    fn timed(
        clipboard: &MemClipboard,
        clock: &Timeline,
    ) -> PasteInserter<MemClipboard, FakeKeys, Timeline> {
        PasteInserter::new(
            clipboard.clone(),
            keys_for(clipboard),
            clock.clone(),
            RESTORE_DELAY,
        )
    }

    fn text_of(clipboard: &MemClipboard) -> Option<String> {
        clipboard.read_text().unwrap()
    }

    // (a) #239: a busy target reads the clipboard 1.2 s after the chord
    // (later than the ~1070 ms measured): it gets the transcript.
    #[test]
    fn a_target_reading_at_1_2_s_gets_the_transcript() {
        let clipboard = MemClipboard::with_text(USER);
        let clock = Timeline::default();
        timed(&clipboard, &clock).insert(DICTATED).unwrap();
        clock.ms(1200);
        assert_eq!(text_of(&clipboard).as_deref(), Some(DICTATED));
    }

    // (b) The user's clipboard comes back RESTORE_DELAY after the chord.
    #[test]
    fn the_original_comes_back_at_1_5_s() {
        assert_eq!(RESTORE_DELAY, Duration::from_millis(1500));
        let clipboard = MemClipboard::with_text(USER);
        let clock = Timeline::default();
        timed(&clipboard, &clock).insert(DICTATED).unwrap();
        assert_eq!(*clock.delays.lock().unwrap(), [RESTORE_DELAY]);
        clock.ms(1499);
        assert_eq!(text_of(&clipboard).as_deref(), Some(DICTATED));
        clock.ms(1500);
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
        assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
        assert_eq!(clipboard.session(), None);
    }

    // (c) The user copies at 1.0 s: the restore at 1.5 s leaves it alone.
    #[test]
    fn a_copy_at_1_0_s_is_never_overwritten() {
        let clipboard = MemClipboard::with_text(USER);
        let clock = Timeline::default();
        timed(&clipboard, &clock).insert(DICTATED).unwrap();
        clock.ms(1000);
        clipboard.user_copies("copied at 1 s");
        clock.ms(5000);
        assert_eq!(text_of(&clipboard).as_deref(), Some("copied at 1 s"));
    }

    // (d) A second dictation at 0.8 s does the first restore before it
    // saves the clipboard: it saves the user's text, not the first
    // transcript, and gives that back after its own delay. The first
    // timer, at 1.5 s, finds nothing owed.
    #[test]
    fn a_second_dictation_at_0_8_s_finishes_the_first_restore_then_saves_the_original() {
        let clipboard = MemClipboard::with_text(USER);
        let clock = Timeline::default();
        let flow = timed(&clipboard, &clock);
        flow.insert("first words").unwrap();
        clock.ms(800);
        flow.insert(DICTATED).unwrap();
        assert_eq!(text_of(&clipboard).as_deref(), Some(DICTATED));
        clock.ms(1500);
        assert_eq!(
            text_of(&clipboard).as_deref(),
            Some(DICTATED),
            "first timer: nothing owed"
        );
        clock.ms(2300);
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
        assert_eq!(clipboard.session(), None);
    }

    // ... and a copy between the two dictations is what the second saves.
    #[test]
    fn a_copy_before_the_second_dictation_is_what_it_gives_back() {
        let clipboard = MemClipboard::with_text(USER);
        let clock = Timeline::default();
        let flow = timed(&clipboard, &clock);
        flow.insert("first words").unwrap();
        clock.ms(500);
        clipboard.user_copies("copied between");
        clock.ms(800);
        flow.insert(DICTATED).unwrap();
        clock.ms(5000);
        assert_eq!(text_of(&clipboard).as_deref(), Some("copied between"));
    }

    // (e) E1 paste_failed: the chord didn't go out, so the transcript stays
    // on the clipboard for the user to paste, and no restore is scheduled.
    #[test]
    fn a_failed_paste_keeps_the_transcript_on_the_clipboard() {
        let clipboard = MemClipboard::with_text(USER);
        let clock = Timeline::default();
        for error in [
            InsertError::PermissionDenied,
            InsertError::Failed("no keys".into()),
        ] {
            let flow = PasteInserter::new(
                clipboard.clone(),
                FakeKeys {
                    fail: Some(error.clone()),
                    ..keys_for(&clipboard)
                },
                clock.clone(),
                RESTORE_DELAY,
            );
            assert_eq!(flow.insert(DICTATED), Err(error));
            clock.ms(10_000);
            assert_eq!(text_of(&clipboard).as_deref(), Some(DICTATED));
        }
        assert!(clock.delays.lock().unwrap().is_empty());
    }

    // (f) Other apps reading the clipboard before and after the chord (a
    // clipboard manager, Universal Clipboard) change nothing: the target
    // still gets the transcript and the original still comes back.
    #[test]
    fn other_readers_before_and_after_the_chord_change_nothing() {
        let inner = MemClipboard::with_text(USER);
        let reader = inner.clone();
        let clipboard = HookClipboard {
            inner: inner.clone(),
            after_write: Some(Arc::new(move || {
                let _ = reader.read_text().unwrap();
            })),
            ..HookClipboard::default()
        };
        let clock = Timeline::default();
        let keys = keys_for(&inner);
        PasteInserter::new(clipboard, keys.clone(), clock.clone(), RESTORE_DELAY)
            .insert(DICTATED)
            .unwrap();
        clock.ms(10);
        let _ = inner.read_text().unwrap();
        clock.ms(1200);
        assert_eq!(
            text_of(&inner).as_deref(),
            Some(DICTATED),
            "the slow target's read"
        );
        let _ = inner.read_text().unwrap();
        clock.ms(1500);
        assert_eq!(text_of(&inner).as_deref(), Some(USER));
        assert_eq!(*keys.pastes.lock().unwrap(), [Some(DICTATED.to_owned())]);
    }

    #[test]
    fn thread_later_runs_the_job_after_the_delay() {
        let (tx, rx) = std::sync::mpsc::channel();
        let started = std::time::Instant::now();
        ThreadLater.after(
            Duration::from_millis(5),
            Box::new(move || tx.send(started.elapsed()).unwrap()),
        );
        let elapsed = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(elapsed >= Duration::from_millis(5));
    }

    // --- P1 round 4: quitting before the timer --------------------------

    const SHORT: Duration = Duration::from_millis(60);

    /// A flow whose timer never fires on its own (the clock isn't run), as
    /// when the app exits first, with a short real delay for `finish`.
    fn quitting(clipboard: &MemClipboard) -> PasteInserter<MemClipboard, FakeKeys, Timeline> {
        PasteInserter::new(
            clipboard.clone(),
            keys_for(clipboard),
            Timeline::default(),
            SHORT,
        )
    }

    // Codex round 3: Quit right after a paste. `finish` (the app's shutdown
    // path) waits out the rest of the delay, never restoring earlier, then
    // gives the user's clipboard back.
    #[test]
    fn a_quit_before_the_timer_still_restores_the_original_at_its_time() {
        let clipboard = MemClipboard::with_text(USER);
        let flow = quitting(&clipboard);
        let chord = std::time::Instant::now();
        flow.insert(DICTATED).unwrap();
        assert_eq!(text_of(&clipboard).as_deref(), Some(DICTATED));
        flow.finish();
        assert!(chord.elapsed() >= SHORT, "not before its time");
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
        assert_eq!(clipboard.session(), None);
    }

    // ... and a copy the user made before quitting is kept.
    #[test]
    fn a_quit_keeps_a_newer_copy() {
        let clipboard = MemClipboard::with_text(USER);
        let flow = quitting(&clipboard);
        flow.insert(DICTATED).unwrap();
        clipboard.user_copies("copied before quitting");
        flow.finish();
        assert_eq!(
            text_of(&clipboard).as_deref(),
            Some("copied before quitting")
        );
    }

    // Nothing owed: `finish` returns at once; and no insert after it.
    #[test]
    fn finish_with_nothing_owed_returns_at_once_and_closes() {
        let clipboard = MemClipboard::with_text(USER);
        let flow = PasteInserter::new(
            clipboard.clone(),
            keys_for(&clipboard),
            Timeline::default(),
            Duration::from_secs(60),
        );
        let started = std::time::Instant::now();
        flow.finish();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(matches!(flow.insert(DICTATED), Err(InsertError::Failed(_))));
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER), "untouched");
    }

    /// Keys that take a while to send, as a paste in flight at Quit.
    #[derive(Clone)]
    struct SlowKeys(FakeKeys);

    impl PasteKeys for SlowKeys {
        fn paste(&self) -> Result<(), InsertError> {
            std::thread::sleep(Duration::from_millis(80));
            self.0.paste()
        }
    }

    // A Quit while an insert is still pasting waits for it, then finishes
    // the restore it owes.
    #[test]
    fn a_quit_during_a_paste_waits_for_it_and_restores() {
        let clipboard = MemClipboard::with_text(USER);
        let flow = Arc::new(PasteInserter::new(
            clipboard.clone(),
            SlowKeys(keys_for(&clipboard)),
            Timeline::default(),
            SHORT,
        ));
        let pasting = {
            let flow = Arc::clone(&flow);
            std::thread::spawn(move || flow.insert(DICTATED))
        };
        std::thread::sleep(Duration::from_millis(20));
        flow.finish();
        assert_eq!(pasting.join().unwrap(), Ok(()));
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
    }

    // The timer, if it still runs after `finish`, finds nothing owed.
    #[test]
    fn the_timer_after_a_finish_changes_nothing() {
        let clipboard = MemClipboard::with_text(USER);
        let clock = Timeline::default();
        let flow = PasteInserter::new(
            clipboard.clone(),
            keys_for(&clipboard),
            clock.clone(),
            SHORT,
        );
        flow.insert(DICTATED).unwrap();
        flow.finish();
        clipboard.user_copies("copied after quitting");
        clock.ms(10_000);
        assert_eq!(
            text_of(&clipboard).as_deref(),
            Some("copied after quitting")
        );
    }
}
