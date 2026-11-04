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
//! 4. wait until the paste shows in the target (#239): the focused field's
//!    caret moved and now follows the text's last character
//!    ([`Caret::shows_paste_of`], read through a [`CaretProbe`]), checked
//!    every [`LANDING_POLL`] for at most [`LANDING_TIMEOUT`]. A clipboard
//!    read is no such sign: any app can read the clipboard, and macOS
//!    hands a promised text to its first reader only (#241 review);
//! 5. once it landed, put the saved contents back, only if our session
//!    marker is still on the clipboard
//!    ([`Clipboard::restore_snapshot_if_unchanged`]): if the user (or an
//!    app) copied something meanwhile, it's left alone. If the paste
//!    couldn't be confirmed (no readable caret, or nothing in time), the
//!    transcript stays on the clipboard, so a late paste still pastes it,
//!    and the user's clipboard is owed: it goes back the inserter's delay
//!    after the chord, in the background ([`Later`]), unless someone copied
//!    by then; and a dictation that starts first takes it over, so it goes
//!    back after that paste instead of being lost under a second
//!    transcript.
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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dictation_ports::{
    Clipboard, ClipboardError, ClipboardSnapshot, InsertError, SessionMarker, TextInserter,
};

#[cfg(target_os = "macos")]
pub use macos::{MacPasteKeys, MacPasteboard};

/// The old fixed wait between the chord and the restore, now only the
/// delay the unit tests give [`PasteInserter::new`] (the app uses
/// [`UNCONFIRMED_RESTORE`]).
pub const DEFAULT_SETTLE: Duration = Duration::from_millis(300);

/// How often the paste checks the target's caret for the paste.
pub const LANDING_POLL: Duration = Duration::from_millis(20);

/// How long the paste looks for it. A busy target pasted the first
/// dictation after a relaunch more than a second after the chord (#239).
pub const LANDING_TIMEOUT: Duration = Duration::from_secs(2);

/// When an unconfirmed paste gets the user's clipboard back: this long after
/// the chord. Pastes into fields without a readable caret (many web and
/// Electron apps) all take this path, so it is the residual risk: a target
/// busy for longer than this pastes the user's old clipboard.
pub const UNCONFIRMED_RESTORE: Duration = Duration::from_secs(3);

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

/// Where the caret is in the focused text field, as the paste's landing
/// check (step 4 in the module docs) sees it. `Debug` never shows the
/// character.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Caret {
    /// Identifies the focused field (on macOS, the `CFHash` of its
    /// accessibility element), so a focus change isn't taken for a paste.
    pub field: u64,
    /// The selection's start, in UTF-16 units.
    pub location: usize,
    /// The selection's length, in UTF-16 units (0: a plain caret).
    pub length: usize,
    /// The character just before the selection, if it could be read (the
    /// same bounded read as the smart space, `crate::cursor`).
    pub last: Option<char>,
}

impl std::fmt::Debug for Caret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Caret")
            .field("field", &self.field)
            .field("location", &self.location)
            .field("length", &self.length)
            .field("last", &self.last.map(|_| ".."))
            .finish()
    }
}

impl Caret {
    /// Whether the target pasted `text` between `before` (read just before
    /// the chord) and `self`: same field, the caret moved and is a plain
    /// caret again, and the character before it is the text's last one.
    /// This comes from the target app itself, unlike a clipboard read,
    /// which any app can make (#241 review). Keys the target handles
    /// before the paste (a combination hotkey's own keys) select text or
    /// move the caret, but don't end it after the text's last character.
    #[must_use]
    pub fn shows_paste_of(&self, before: &Self, text: &str) -> bool {
        self.field == before.field
            && self.length == 0
            && self.location != before.location
            && self.location >= text.encode_utf16().count()
            && self.last.is_some()
            && self.last == text.chars().last()
    }
}

/// Reads the caret of the focused text field, for the landing check.
/// `None` if there is no readable caret (no text field, a password field,
/// an app that doesn't expose it, no Accessibility).
pub trait CaretProbe: Send + Sync {
    /// The caret now.
    fn caret(&self) -> Option<Caret>;
}

impl<F: Fn() -> Option<Caret> + Send + Sync> CaretProbe for F {
    fn caret(&self) -> Option<Caret> {
        self()
    }
}

/// The probe of a [`PasteInserter`] that can't see a caret: every paste is
/// unconfirmed, so the clipboard goes back after the inserter's delay.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCaret;

impl CaretProbe for NoCaret {
    fn caret(&self) -> Option<Caret> {
        None
    }
}

/// Runs a job later. [`PasteInserter`] uses it to put the user's clipboard
/// back after an unconfirmed paste without keeping the dictation busy.
pub trait Later: Send + Sync {
    /// Runs `job` after `delay`.
    fn after(&self, delay: Duration, job: Box<dyn FnOnce() + Send>);
}

/// [`Later`] on a thread of its own that sleeps first.
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
            // The original stays pending; the next dictation carries it.
            tracing::warn!(%error, "could not schedule the clipboard restore");
        }
    }
}

/// The user's clipboard, saved before a paste that couldn't be confirmed,
/// still owed to them while the transcript sits on the clipboard.
#[derive(Debug)]
struct Pending {
    id: u64,
    session: SessionMarker,
    saved: ClipboardSnapshot,
}

/// What the wait after the chord found out (step 4 in the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Landing {
    /// The target's caret shows the paste.
    Landed,
    /// The clipboard no longer holds our write: someone copied.
    Changed,
    /// No sign of the paste: no readable caret, or none in time.
    Unconfirmed,
}

/// The paste flow (see the module docs) over any clipboard and key sender.
pub struct PasteInserter<C, K, S = ThreadSettle, P = NeverSecure, Q = NoCaret> {
    clipboard: Arc<C>,
    keys: K,
    settle: S,
    delay: Duration,
    secure_input: P,
    caret: Q,
    /// `None`: run the late restore inline, after `settle` waits `delay`.
    later: Option<Arc<dyn Later>>,
    pending: Arc<Mutex<Option<Pending>>>,
    next_id: AtomicU64,
}

impl<C, K, S, P, Q> std::fmt::Debug for PasteInserter<C, K, S, P, Q> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasteInserter")
            .field("delay", &self.delay)
            .finish_non_exhaustive()
    }
}

impl<C, K, S> PasteInserter<C, K, S> {
    /// The flow over `clipboard` and `keys`. A paste that can't be
    /// confirmed gets the user's clipboard back `delay` after the chord,
    /// waited by `settle` before the insert returns. It doesn't check
    /// Secure Input or read the caret; see
    /// [`with_secure_input`](Self::with_secure_input),
    /// [`with_caret`](Self::with_caret) and [`with_later`](Self::with_later).
    pub fn new(clipboard: C, keys: K, settle: S, delay: Duration) -> Self {
        Self {
            clipboard: Arc::new(clipboard),
            keys,
            settle,
            delay,
            secure_input: NeverSecure,
            caret: NoCaret,
            later: None,
            pending: Arc::new(Mutex::new(None)),
            next_id: AtomicU64::new(0),
        }
    }
}

impl<C, K, S, P, Q> PasteInserter<C, K, S, P, Q> {
    /// The same flow, refusing to paste while `probe` says Secure Input is
    /// on.
    pub fn with_secure_input<R: SecureInputProbe>(self, probe: R) -> PasteInserter<C, K, S, R, Q> {
        PasteInserter {
            clipboard: self.clipboard,
            keys: self.keys,
            settle: self.settle,
            delay: self.delay,
            secure_input: probe,
            caret: self.caret,
            later: self.later,
            pending: self.pending,
            next_id: self.next_id,
        }
    }

    /// The same flow, confirming each paste with `probe` (step 4).
    pub fn with_caret<R: CaretProbe>(self, probe: R) -> PasteInserter<C, K, S, P, R> {
        PasteInserter {
            clipboard: self.clipboard,
            keys: self.keys,
            settle: self.settle,
            delay: self.delay,
            secure_input: self.secure_input,
            caret: probe,
            later: self.later,
            pending: self.pending,
            next_id: self.next_id,
        }
    }

    /// The same flow, with the late restore of an unconfirmed paste run by
    /// `later` instead of inline, so the dictation doesn't wait for it.
    #[must_use]
    pub fn with_later(mut self, later: impl Later + 'static) -> Self {
        self.later = Some(Arc::new(later));
        self
    }
}

fn failed(what: &str, error: &ClipboardError) -> InsertError {
    InsertError::Failed(format!("{what}: {error}"))
}

fn lock_pending(pending: &Mutex<Option<Pending>>) -> std::sync::MutexGuard<'_, Option<Pending>> {
    // A panic elsewhere can't leave an Option half written.
    pending
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

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

impl<C, K, S, P, Q> TextInserter for PasteInserter<C, K, S, P, Q>
where
    C: Clipboard + 'static,
    K: PasteKeys,
    S: Settle,
    P: SecureInputProbe,
    Q: CaretProbe,
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
        let (saved, carried) = self.saved_clipboard()?;
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
        // This paste owes the carried-over clipboard now.
        if let Some(id) = carried {
            let mut slot = lock_pending(&self.pending);
            if slot.as_ref().is_some_and(|owed| owed.id == id) {
                *slot = None;
            }
        }
        let before = self.caret.caret();
        // The last check is inside `paste_unless`, after the key sender's
        // own preparation. If it trips, nothing is sent and the clipboard
        // goes back at once (unless the user copied since).
        let pasted = self.keys.paste_unless(&blocked);
        if pasted.is_err() {
            restore(&*self.clipboard, &session, &saved);
            return pasted;
        }
        match self.wait_for_landing(&session, before, text) {
            Landing::Landed | Landing::Changed => restore(&*self.clipboard, &session, &saved),
            Landing::Unconfirmed => self.restore_later(session, saved),
        }
        pasted
    }
}

impl<C, K, S, P, Q> PasteInserter<C, K, S, P, Q>
where
    C: Clipboard + 'static,
    S: Settle,
    Q: CaretProbe,
{
    /// Step 1: what to give back after this paste. If an earlier paste is
    /// still owed the user's clipboard and nobody copied since (our write
    /// is still on it), that saved clipboard is the user's, not the stale
    /// transcript; else a new snapshot.
    ///
    /// The owed clipboard stays in its slot (a copy is returned with its
    /// id) until this paste's own write, so an insert that stops before
    /// writing (Secure Input) still owes it.
    fn saved_clipboard(&self) -> Result<(ClipboardSnapshot, Option<u64>), InsertError> {
        {
            let mut slot = lock_pending(&self.pending);
            if let Some(owed) = slot.as_ref() {
                if self.clipboard.holds_dictation(&owed.session) {
                    tracing::debug!("carrying the user's clipboard over from the last paste");
                    return Ok((owed.saved.clone(), Some(owed.id)));
                }
                // The user copied since: that copy is theirs now.
                *slot = None;
            }
        }
        self.clipboard
            .snapshot()
            .map(|saved| (saved, None))
            .map_err(|e| failed("could not save the clipboard", &e))
    }

    /// Step 4: waits up to [`LANDING_TIMEOUT`] for the target's caret to
    /// show the paste (see [`Caret::shows_paste_of`]).
    fn wait_for_landing(
        &self,
        session: &SessionMarker,
        before: Option<Caret>,
        text: &str,
    ) -> Landing {
        let Some(before) = before else {
            tracing::info!("no readable caret; the paste can't be confirmed");
            return Landing::Unconfirmed;
        };
        let mut waited = Duration::ZERO;
        while waited < LANDING_TIMEOUT {
            self.settle.settle(LANDING_POLL);
            waited += LANDING_POLL;
            if !self.clipboard.holds_dictation(session) {
                tracing::info!("the clipboard changed while waiting for the paste");
                return Landing::Changed;
            }
            if self
                .caret
                .caret()
                .is_some_and(|now| now.shows_paste_of(&before, text))
            {
                // Timing only, never the text.
                tracing::info!(landed_ms = waited.as_millis(), "the paste landed");
                return Landing::Landed;
            }
        }
        tracing::info!(timeout = ?LANDING_TIMEOUT, "the paste wasn't seen landing");
        Landing::Unconfirmed
    }

    /// Step 5 for an unconfirmed paste: the transcript stays on the
    /// clipboard, and the user's clipboard is owed back `delay` after the
    /// chord, if nobody copied by then and no later dictation took it over.
    fn restore_later(&self, session: SessionMarker, saved: ClipboardSnapshot) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        *lock_pending(&self.pending) = Some(Pending { id, session, saved });
        let (clipboard, pending) = (Arc::clone(&self.clipboard), Arc::clone(&self.pending));
        let job = move || {
            let owed = {
                let mut slot = lock_pending(&pending);
                match slot.as_ref() {
                    Some(owed) if owed.id == id => slot.take(),
                    _ => None,
                }
            };
            if let Some(owed) = owed {
                restore(&*clipboard, &owed.session, &owed.saved);
            }
        };
        match &self.later {
            Some(later) => later.after(self.delay, Box::new(job)),
            None => {
                self.settle.settle(self.delay);
                job();
            }
        }
    }
}

/// The macOS flow: Secure Input and caret probes as plain functions.
#[cfg(target_os = "macos")]
type MacInserter =
    PasteInserter<MacPasteboard, MacPasteKeys, ThreadSettle, fn() -> bool, fn() -> Option<Caret>>;

/// This OS's inserter: [`PasteInserter`] over [`MacPasteboard`] and
/// [`MacPasteKeys`] on macOS. Elsewhere (until M-W.2) every insert fails
/// with [`InsertError::Failed`].
pub struct SystemInserter {
    #[cfg(target_os = "macos")]
    inner: MacInserter,
}

impl SystemInserter {
    /// The inserter: Secure Input checks, the caret as the landing check,
    /// and [`UNCONFIRMED_RESTORE`] in the background for unconfirmed pastes.
    #[must_use]
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            inner: PasteInserter::new(
                MacPasteboard::new(),
                MacPasteKeys::new(),
                ThreadSettle,
                UNCONFIRMED_RESTORE,
            )
            .with_secure_input(crate::secure_input::is_secure_input_enabled as fn() -> bool)
            .with_caret(crate::cursor::caret as fn() -> Option<Caret>)
            .with_later(ThreadLater),
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
        fn holds_dictation(&self, session: &dictation_ports::SessionMarker) -> bool {
            self.inner.holds_dictation(session)
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
        fn holds_dictation(&self, session: &dictation_ports::SessionMarker) -> bool {
            self.inner.holds_dictation(session)
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

    // --- P1 round 2: the paste is confirmed by the target's caret ----------

    /// The target app's text field: its caret, which the inserter's probe
    /// reads, and what it pasted.
    #[derive(Clone)]
    struct Target {
        caret: Arc<Mutex<Option<Caret>>>,
        pasted: Arc<Mutex<Vec<Option<String>>>>,
    }

    const FIELD: u64 = 7;

    impl Target {
        /// A plain caret at `location` after `last`.
        fn at(location: usize, last: Option<char>) -> Self {
            Self {
                caret: Arc::new(Mutex::new(Some(Caret {
                    field: FIELD,
                    location,
                    length: 0,
                    last,
                }))),
                pasted: Arc::default(),
            }
        }

        fn unreadable() -> Self {
            Self {
                caret: Arc::new(Mutex::new(None)),
                pasted: Arc::default(),
            }
        }

        fn probe(&self) -> impl Fn() -> Option<Caret> + Send + Sync + use<> {
            let caret = Arc::clone(&self.caret);
            move || *caret.lock().unwrap()
        }

        fn set(&self, caret: Option<Caret>) {
            *self.caret.lock().unwrap() = caret;
        }

        /// The target handles ⌘V now: it reads the clipboard (whatever is
        /// on it at this moment) and pastes it at the caret.
        fn handles_paste(&self, clipboard: &MemClipboard) {
            let text = clipboard.read_text().unwrap();
            let mut caret = self.caret.lock().unwrap();
            if let (Some(c), Some(t)) = (caret.as_mut(), &text) {
                c.location += t.encode_utf16().count();
                c.length = 0;
                c.last = t.chars().last();
            }
            self.pasted.lock().unwrap().push(text);
        }

        fn pasted(&self) -> Vec<Option<String>> {
            self.pasted.lock().unwrap().clone()
        }
    }

    /// Another app reading the clipboard (a clipboard manager, Universal
    /// Clipboard): it learns nothing for the inserter, whenever it reads.
    fn observer_reads(clipboard: &MemClipboard) {
        let _ = clipboard.read_text().unwrap();
    }

    type LaterJob = (Duration, Box<dyn FnOnce() + Send>);

    /// Records [`Later`] jobs; the test runs them when it likes.
    #[derive(Clone, Default)]
    struct ManualLater {
        jobs: Arc<Mutex<Vec<LaterJob>>>,
    }

    impl Later for ManualLater {
        fn after(&self, delay: Duration, job: Box<dyn FnOnce() + Send>) {
            self.jobs.lock().unwrap().push((delay, job));
        }
    }

    impl ManualLater {
        fn delays(&self) -> Vec<Duration> {
            self.jobs.lock().unwrap().iter().map(|(d, _)| *d).collect()
        }

        fn run_all(&self) {
            let jobs: Vec<_> = self.jobs.lock().unwrap().drain(..).collect();
            for (_, job) in jobs {
                job();
            }
        }
    }

    /// The flow over `clipboard` (observed by another app right after the
    /// dictation write), confirming with `target`'s caret, with late
    /// restores in `later`.
    fn caret_flow(
        clipboard: &MemClipboard,
        target: &Target,
        settle: FakeSettle,
        later: &ManualLater,
    ) -> PasteInserter<HookClipboard, FakeKeys, FakeSettle, NeverSecure, impl CaretProbe> {
        let observed = clipboard.clone();
        let hooked = HookClipboard {
            inner: clipboard.clone(),
            after_write: Some(Arc::new(move || observer_reads(&observed))),
            ..HookClipboard::default()
        };
        PasteInserter::new(hooked, keys_for(clipboard), settle, UNCONFIRMED_RESTORE)
            .with_caret(target.probe())
            .with_later(later.clone())
    }

    /// A settle that runs `at(poll)` on every wait, counting from 1.
    fn polls(at: impl Fn(u32) + Send + Sync + 'static) -> FakeSettle {
        let count = Arc::new(Mutex::new(0_u32));
        FakeSettle {
            during: Some(Arc::new(move || {
                let mut n = count.lock().unwrap();
                *n += 1;
                at(*n);
            })),
            ..FakeSettle::default()
        }
    }

    fn waited(settle: &FakeSettle) -> Duration {
        settle.waits.lock().unwrap().iter().sum()
    }

    fn text_of(clipboard: &MemClipboard) -> Option<String> {
        clipboard.read_text().unwrap()
    }

    const USER: &str = "the user's text";
    const DICTATED: &str = "dictated words";

    // Review finding 1 (#241): other apps read the clipboard before and
    // after the chord while the target is busy and handles ⌘V at about
    // 1080 ms (the delay measured in the VM). The clipboard goes back only
    // once the target pasted, and the target pastes the transcript.
    #[test]
    fn a_busy_target_gets_the_transcript_whatever_other_apps_read() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::at(0, None);
        let (c, t) = (clipboard.clone(), target.clone());
        let settle = polls(move |n| {
            if n == 1 || n == 30 {
                observer_reads(&c);
            }
            if n == 54 {
                t.handles_paste(&c);
            }
        });
        let later = ManualLater::default();
        let result = caret_flow(&clipboard, &target, settle.clone(), &later).insert(DICTATED);

        assert_eq!(result, Ok(()));
        assert_eq!(target.pasted(), [Some(DICTATED.to_owned())]);
        assert_eq!(waited(&settle), LANDING_POLL * 54);
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
        assert_eq!(clipboard.session(), None);
        assert!(later.delays().is_empty(), "nothing owed");
    }

    // Review finding 2: an app reads the text before the chord; the target
    // still pastes at once, and the user's clipboard comes back.
    #[test]
    fn a_read_before_the_chord_doesnt_keep_the_original_from_coming_back() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::at(3, Some('x'));
        let (c, t) = (clipboard.clone(), target.clone());
        let settle = polls(move |n| {
            if n == 1 {
                t.handles_paste(&c);
            }
        });
        let later = ManualLater::default();
        let result = caret_flow(&clipboard, &target, settle.clone(), &later).insert(DICTATED);

        assert_eq!(result, Ok(()));
        assert_eq!(target.pasted(), [Some(DICTATED.to_owned())]);
        assert_eq!(waited(&settle), LANDING_POLL);
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
        assert!(later.delays().is_empty());
    }

    // A combination hotkey's own keys reach the target first: Ctrl+Shift+A
    // selects text (the caret moves) before ⌘V is handled. That isn't the
    // paste.
    #[test]
    fn the_hotkeys_own_selection_is_not_the_paste() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::at(12, Some('y'));
        let (c, t) = (clipboard.clone(), target.clone());
        let settle = polls(move |n| {
            if n == 2 {
                t.set(Some(Caret {
                    field: FIELD,
                    location: 0,
                    length: 12,
                    last: None,
                }));
            }
            if n == 10 {
                t.handles_paste(&c);
            }
        });
        let later = ManualLater::default();
        caret_flow(&clipboard, &target, settle.clone(), &later)
            .insert(DICTATED)
            .unwrap();
        assert_eq!(waited(&settle), LANDING_POLL * 10);
        assert_eq!(target.pasted(), [Some(DICTATED.to_owned())]);
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
    }

    // The same words dictated twice: the character before the caret is
    // already the text's last one, but the caret hasn't moved yet.
    #[test]
    fn the_same_text_again_waits_for_the_caret_to_move() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::at(14, Some('s'));
        let (c, t) = (clipboard.clone(), target.clone());
        let settle = polls(move |n| {
            if n == 5 {
                t.handles_paste(&c);
            }
        });
        let later = ManualLater::default();
        caret_flow(&clipboard, &target, settle.clone(), &later)
            .insert(DICTATED)
            .unwrap();
        assert_eq!(waited(&settle), LANDING_POLL * 5);
        assert_eq!(target.pasted(), [Some(DICTATED.to_owned())]);
    }

    // P1 criterion 4: no readable caret (many web and Electron fields): the
    // dictation doesn't wait, the transcript stays on the clipboard so a
    // late ⌘V still pastes it, and the user's clipboard goes back later.
    #[test]
    fn without_a_caret_the_transcript_stays_and_the_original_comes_back_later() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::unreadable();
        let settle = FakeSettle::default();
        let later = ManualLater::default();
        let result = caret_flow(&clipboard, &target, settle.clone(), &later).insert(DICTATED);

        assert_eq!(result, Ok(()));
        assert!(settle.waits.lock().unwrap().is_empty(), "no busy wait");
        assert_eq!(later.delays(), [UNCONFIRMED_RESTORE]);
        // The busy target handles ⌘V now: it gets the transcript.
        target.handles_paste(&clipboard);
        assert_eq!(target.pasted(), [Some(DICTATED.to_owned())]);
        later.run_all();
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
        assert_eq!(clipboard.markers(), ClipboardMarkers::NONE);
    }

    // A readable caret that never shows the paste in time (the target
    // ignores ⌘V, or changes the text): the same as no caret, after the
    // landing timeout.
    #[test]
    fn a_paste_never_seen_landing_is_unconfirmed_after_the_timeout() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::at(0, None);
        let settle = FakeSettle::default();
        let later = ManualLater::default();
        caret_flow(&clipboard, &target, settle.clone(), &later)
            .insert(DICTATED)
            .unwrap();
        assert_eq!(waited(&settle), LANDING_TIMEOUT);
        assert_eq!(text_of(&clipboard).as_deref(), Some(DICTATED));
        assert_eq!(later.delays(), [UNCONFIRMED_RESTORE]);
        later.run_all();
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
    }

    // Focus moving to another field, even one whose caret looks pasted
    // into, isn't the paste.
    #[test]
    fn a_caret_in_another_field_is_not_the_paste() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::at(0, None);
        let t = target.clone();
        let settle = polls(move |n| {
            if n == 1 {
                t.set(Some(Caret {
                    field: FIELD + 1,
                    location: 14,
                    length: 0,
                    last: Some('s'),
                }));
            }
        });
        let later = ManualLater::default();
        caret_flow(&clipboard, &target, settle.clone(), &later)
            .insert(DICTATED)
            .unwrap();
        assert_eq!(waited(&settle), LANDING_TIMEOUT);
        assert_eq!(text_of(&clipboard).as_deref(), Some(DICTATED));
    }

    // A copy the user makes after an unconfirmed dictation is never
    // overwritten by the late restore.
    #[test]
    fn the_late_restore_never_overwrites_a_newer_copy() {
        let clipboard = MemClipboard::with_text(USER);
        let later = ManualLater::default();
        caret_flow(
            &clipboard,
            &Target::unreadable(),
            FakeSettle::default(),
            &later,
        )
        .insert(DICTATED)
        .unwrap();
        clipboard.user_copies("copied after");
        later.run_all();
        assert_eq!(text_of(&clipboard).as_deref(), Some("copied after"));
    }

    // Review finding 2, the other half: a second dictation before the late
    // restore takes the user's clipboard over, so it comes back after the
    // second paste, never lost under the first transcript.
    #[test]
    fn a_dictation_before_the_late_restore_carries_the_original_over() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::at(0, None);
        let later = ManualLater::default();
        *target.caret.lock().unwrap() = None;
        let flow = caret_flow(&clipboard, &target, FakeSettle::default(), &later);
        flow.insert("first words").unwrap();
        assert_eq!(text_of(&clipboard).as_deref(), Some("first words"));

        // The second paste lands; the first late restore hasn't run yet.
        target.set(Some(Caret {
            field: FIELD,
            location: 11,
            length: 0,
            last: Some('s'),
        }));
        let (c, t) = (clipboard.clone(), target.clone());
        let second = PasteInserter {
            settle: polls(move |n| {
                if n == 1 {
                    t.handles_paste(&c);
                }
            }),
            ..flow
        };
        second.insert(DICTATED).unwrap();
        assert_eq!(target.pasted(), [Some(DICTATED.to_owned())]);
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));

        // The first late restore finds nothing owed and changes nothing.
        clipboard.user_copies("copied later");
        later.run_all();
        assert_eq!(text_of(&clipboard).as_deref(), Some("copied later"));
    }

    // ... but if the user copied in between, their copy is what the next
    // dictation saves and gives back.
    #[test]
    fn a_copy_between_dictations_replaces_what_is_owed() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::unreadable();
        let later = ManualLater::default();
        let flow = caret_flow(&clipboard, &target, FakeSettle::default(), &later);
        flow.insert("first words").unwrap();
        clipboard.user_copies("copied between");
        flow.insert(DICTATED).unwrap();
        later.run_all();
        assert_eq!(text_of(&clipboard).as_deref(), Some("copied between"));
    }

    // A dictation stopped by Secure Input after its snapshot still owes
    // the user's clipboard from the one before.
    #[test]
    fn a_secure_input_stop_keeps_what_is_owed() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::unreadable();
        let later = ManualLater::default();
        let flow = caret_flow(&clipboard, &target, FakeSettle::default(), &later);
        flow.insert("first words").unwrap();
        let blocked = flow.with_secure_input(|| true);
        assert_eq!(blocked.insert(DICTATED), Err(InsertError::SecureInput));
        later.run_all();
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
    }

    // The user copies while the paste is being looked for: the wait ends,
    // and their copy stays.
    #[test]
    fn a_copy_while_waiting_for_the_paste_ends_the_wait_and_is_kept() {
        let clipboard = MemClipboard::with_text(USER);
        let target = Target::at(0, None);
        let c = clipboard.clone();
        let settle = polls(move |n| {
            if n == 3 {
                c.user_copies("copied meanwhile");
            }
        });
        let later = ManualLater::default();
        caret_flow(&clipboard, &target, settle.clone(), &later)
            .insert(DICTATED)
            .unwrap();
        assert_eq!(waited(&settle), LANDING_POLL * 3);
        assert_eq!(text_of(&clipboard).as_deref(), Some("copied meanwhile"));
        assert!(later.delays().is_empty());
    }

    // A failed chord waits for nothing: the clipboard goes back at once.
    #[test]
    fn a_failed_chord_restores_at_once_even_with_a_caret() {
        let clipboard = MemClipboard::with_text(USER);
        let later = ManualLater::default();
        let flow = PasteInserter::new(
            clipboard.clone(),
            FakeKeys {
                fail: Some(InsertError::PermissionDenied),
                ..keys_for(&clipboard)
            },
            FakeSettle::default(),
            UNCONFIRMED_RESTORE,
        )
        .with_caret(Target::at(0, None).probe())
        .with_later(later.clone());
        assert_eq!(flow.insert(DICTATED), Err(InsertError::PermissionDenied));
        assert_eq!(text_of(&clipboard).as_deref(), Some(USER));
        assert!(later.delays().is_empty());
    }

    fn caret(location: usize, length: usize, last: Option<char>) -> Caret {
        Caret {
            field: FIELD,
            location,
            length,
            last,
        }
    }

    #[test]
    fn shows_paste_of_needs_the_same_field_a_moved_plain_caret_and_the_last_char() {
        let before = caret(4, 0, Some('a'));
        assert!(caret(7, 0, Some('c')).shows_paste_of(&before, "abc"));
        // A selection replaced by the paste.
        assert!(caret(3, 0, Some('c')).shows_paste_of(&caret(0, 5, None), "abc"));
        assert!(
            !caret(4, 0, Some('c')).shows_paste_of(&before, "abc"),
            "not moved"
        );
        assert!(
            !caret(7, 2, Some('c')).shows_paste_of(&before, "abc"),
            "a selection"
        );
        assert!(
            !caret(7, 0, Some('x')).shows_paste_of(&before, "abc"),
            "other text"
        );
        assert!(!caret(7, 0, None).shows_paste_of(&before, "abc"), "unread");
        assert!(
            !caret(2, 0, Some('c')).shows_paste_of(&before, "abc"),
            "too short"
        );
        let other = Caret {
            field: FIELD + 1,
            ..caret(7, 0, Some('c'))
        };
        assert!(!other.shows_paste_of(&before, "abc"), "another field");
        // UTF-16 length: an emoji is two units.
        assert!(caret(2, 0, Some('😀')).shows_paste_of(&caret(0, 0, None), "😀"));
    }

    #[test]
    fn caret_debug_never_shows_the_character() {
        let shown = format!("{:?}", caret(1, 0, Some('Z')));
        assert!(!shown.contains('Z'), "{shown}");
    }
}
