//! The queue of [`UiEvent`]s from the pipeline to the UI (#84).
//!
//! Bounded, so a UI that stops reading (a frozen webview, a debugger) can't
//! make the app grow. The policy, which [`Pipeline::spawn`](crate::pipeline::Pipeline::spawn)
//! callers can rely on:
//!
//! - **Order.** Events arrive in the order they happened while the queue has
//!   room ([`UI_EVENTS_QUEUED`] in all), which is always, for a UI that reads.
//! - **Phases coalesce.** When the queue is full, the older
//!   [`UiEvent::StateChanged`]s in it are dropped and only the newest
//!   stays, so the UI never ends on a stale phase: the last phase it reads
//!   is always the pipeline's current one.
//! - **Errors are kept, up to a limit.** At most [`UI_ERRORS_QUEUED`]
//!   [`UiEvent::Error`]s wait unread. Past that the newest ones are dropped,
//!   counted ([`UiReceiver::dropped_errors`]) and logged (the first, and
//!   then once per [`DROPPED_LOG_EVERY`]); the oldest are the ones the user
//!   most needs to see, and every one is also in the log.
//! - **The hotkey's return is kept.** The newest
//!   [`UiEvent::HotkeyRestored`] stays too, so a UI that showed the hotkey
//!   as down learns it works again.
//! - **So is its newest outage (#154).** A
//!   [`DictationError::HotkeyFailed`] that arrives with the error limit
//!   reached is queued anyway, and the hotkey transitions queued before it
//!   (outages and returns, which it supersedes) are dropped, so the last
//!   transition a UI reads is always the listener's real state. The older
//!   outages dropped this way count as dropped errors. The limit is then
//!   exceeded by at most this one error.
//! - **Recovery.** Reading again picks up at once: what's queued, then the
//!   live stream.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dictation_core::error::DictationError;
use tokio::sync::Notify;
use tokio::sync::mpsc::error::TryRecvError;

use crate::pipeline::UiEvent;

/// The most events that may wait unread.
pub const UI_EVENTS_QUEUED: usize = 64;

/// The most errors among them.
pub const UI_ERRORS_QUEUED: usize = 16;

/// While errors are being dropped, one warning in this many.
const DROPPED_LOG_EVERY: u64 = 100;

#[derive(Default)]
struct Queue {
    events: VecDeque<UiEvent>,
    closed: bool,
}

impl Queue {
    fn errors(&self) -> usize {
        self.events
            .iter()
            .filter(|event| matches!(event, UiEvent::Error(_)))
            .count()
    }

    /// Drops every queued hotkey transition ([`DictationError::HotkeyFailed`]
    /// and [`UiEvent::HotkeyRestored`]); returns how many outages (errors)
    /// that dropped.
    fn drop_hotkey_transitions(&mut self) -> u64 {
        let before = self.errors();
        self.events.retain(|event| !is_hotkey_transition(event));
        // At most UI_EVENTS_QUEUED, so the conversion can't fail.
        u64::try_from(before - self.errors()).unwrap_or(u64::MAX)
    }

    /// Keeps every error, the newest phase and the newest
    /// [`UiEvent::HotkeyRestored`], in their order. Dropping an older
    /// `HotkeyRestored` loses nothing: a later one says the same.
    fn coalesce(&mut self) {
        let newest = |kind: fn(&UiEvent) -> bool| self.events.iter().rposition(kind);
        let newest_phase = newest(|event| matches!(event, UiEvent::StateChanged(_)));
        let newest_restored = newest(|event| matches!(event, UiEvent::HotkeyRestored));
        let newest_no_sound = newest(|event| matches!(event, UiEvent::NoSound));
        let mut index = 0;
        self.events.retain(|event| {
            let keep = match event {
                UiEvent::Error(_) => true,
                UiEvent::StateChanged(_) => Some(index) == newest_phase,
                UiEvent::HotkeyRestored => Some(index) == newest_restored,
                UiEvent::NoSound => Some(index) == newest_no_sound,
            };
            index += 1;
            keep
        });
    }
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    ready: Notify,
    dropped_errors: AtomicU64,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        // Nothing panics while holding it; recover the queue if it did.
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A bounded channel for [`UiEvent`]s: the sending side for the pipeline task
/// and the [`UiReceiver`] for the UI.
#[must_use]
pub fn ui_channel() -> (UiSender, UiReceiver) {
    let shared = Arc::new(Shared::default());
    (
        UiSender {
            shared: Arc::clone(&shared),
        },
        UiReceiver { shared },
    )
}

/// The pipeline's end. Never blocks and never fails: see the module docs for
/// what it drops. Dropping it closes the channel.
pub struct UiSender {
    shared: Arc<Shared>,
}

impl UiSender {
    /// Queues `event` under the module's policy.
    pub fn send(&self, event: UiEvent) {
        {
            let mut queue = self.shared.lock();
            if queue.events.len() >= UI_EVENTS_QUEUED {
                queue.coalesce();
            }
            if is_hotkey_outage(&event) && queue.errors() >= UI_ERRORS_QUEUED {
                // #154: the newest outage is the listener's real state; the
                // transitions before it are history it supersedes.
                let superseded = queue.drop_hotkey_transitions();
                queue.events.push_back(event);
                drop(queue);
                if superseded > 0 {
                    self.count_dropped_errors(superseded);
                }
                self.shared.ready.notify_one();
                return;
            }
            if matches!(event, UiEvent::Error(_)) && queue.errors() >= UI_ERRORS_QUEUED {
                drop(queue);
                let dropped = self.count_dropped_error();
                if log_this_drop(dropped) {
                    tracing::warn!(dropped, "the UI isn't reading; dropping errors");
                }
                return;
            }
            queue.events.push_back(event);
        }
        self.shared.ready.notify_one();
    }
}

impl UiSender {
    /// Counts one more dropped error and returns the total so far.
    fn count_dropped_error(&self) -> u64 {
        self.count_dropped_errors(1)
    }

    /// Counts `n` more dropped errors and returns the total so far.
    fn count_dropped_errors(&self, n: u64) -> u64 {
        self.shared.dropped_errors.fetch_add(n, Ordering::Relaxed) + n
    }
}

/// An outage of the hotkey listener.
fn is_hotkey_outage(event: &UiEvent) -> bool {
    matches!(event, UiEvent::Error(DictationError::HotkeyFailed))
}

/// A change in whether the hotkey works: an outage or its return.
fn is_hotkey_transition(event: &UiEvent) -> bool {
    is_hotkey_outage(event) || matches!(event, UiEvent::HotkeyRestored)
}

/// Whether the `dropped`th dropped error is logged: the first, then one in
/// [`DROPPED_LOG_EVERY`], so a UI that never reads can't flood the log.
fn log_this_drop(dropped: u64) -> bool {
    dropped % DROPPED_LOG_EVERY == 1
}

impl Drop for UiSender {
    fn drop(&mut self) {
        self.shared.lock().closed = true;
        self.shared.ready.notify_one();
    }
}

/// The UI's end: the events, oldest first, and `None` once the pipeline has
/// ended and the queue is empty.
pub struct UiReceiver {
    shared: Arc<Shared>,
}

impl UiReceiver {
    /// The next event, waiting for one; `None` when the pipeline has ended
    /// and everything was read.
    pub async fn recv(&mut self) -> Option<UiEvent> {
        loop {
            {
                let mut queue = self.shared.lock();
                if let Some(event) = queue.events.pop_front() {
                    return Some(event);
                }
                if queue.closed {
                    return None;
                }
            }
            // `notify_one` keeps a permit if nobody waits yet, so a send
            // between the check above and here isn't missed.
            self.shared.ready.notified().await;
        }
    }

    /// The next event if one is waiting.
    ///
    /// # Errors
    ///
    /// [`TryRecvError::Empty`] if none is, [`TryRecvError::Disconnected`] if
    /// none ever will be.
    pub fn try_recv(&mut self) -> Result<UiEvent, TryRecvError> {
        let mut queue = self.shared.lock();
        match queue.events.pop_front() {
            Some(event) => Ok(event),
            None if queue.closed => Err(TryRecvError::Disconnected),
            None => Err(TryRecvError::Empty),
        }
    }

    /// How many events wait unread; never more than [`UI_EVENTS_QUEUED`].
    #[must_use]
    pub fn queued(&self) -> usize {
        self.shared.lock().events.len()
    }

    /// How many errors were dropped because [`UI_ERRORS_QUEUED`] were
    /// already waiting, since the start.
    #[must_use]
    pub fn dropped_errors(&self) -> u64 {
        self.shared.dropped_errors.load(Ordering::Relaxed)
    }
}

impl std::fmt::Debug for UiReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiReceiver")
            .field("queued", &self.queued())
            .field("dropped_errors", &self.dropped_errors())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use dictation_core::coordinator::Phase;

    use super::*;
    use crate::tray::TrayStatus;
    use dictation_core::diagnostics::ErrorCode;

    fn phase(phase: Phase) -> UiEvent {
        UiEvent::StateChanged(phase)
    }

    fn error() -> UiEvent {
        UiEvent::Error(DictationError::SecureInput)
    }

    fn drain(rx: &mut UiReceiver) -> Vec<UiEvent> {
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    #[test]
    fn events_arrive_in_order_while_there_is_room() {
        let (tx, mut rx) = ui_channel();
        let sent = [
            phase(Phase::Recording),
            error(),
            phase(Phase::Transcribing),
            phase(Phase::Idle),
        ];
        for event in sent.clone() {
            tx.send(event);
        }
        assert_eq!(drain(&mut rx), sent);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn a_full_queue_keeps_the_newest_phase_and_every_error() {
        let (tx, mut rx) = ui_channel();
        tx.send(error());
        for n in 0..10 * UI_EVENTS_QUEUED {
            tx.send(phase(if n % 2 == 0 {
                Phase::Recording
            } else {
                Phase::Idle
            }));
            assert!(rx.queued() <= UI_EVENTS_QUEUED, "after {n} phases");
        }
        // 640 phases ended on Idle; the error is still the first thing read.
        let read = drain(&mut rx);
        assert_eq!(read.first(), Some(&error()));
        assert_eq!(read.last(), Some(&phase(Phase::Idle)));
        assert!(read.len() <= UI_EVENTS_QUEUED);
    }

    #[test]
    fn errors_past_the_limit_are_counted_and_the_oldest_are_kept() {
        let (tx, mut rx) = ui_channel();
        let kinds = [
            DictationError::SecureInput,
            DictationError::TranscriptionFailed { reason: "x".into() },
        ];
        for n in 0..UI_ERRORS_QUEUED + 5 {
            tx.send(UiEvent::Error(kinds[n % 2].clone()));
        }
        assert_eq!(rx.dropped_errors(), 5);
        assert_eq!(rx.queued(), UI_ERRORS_QUEUED);
        let read = drain(&mut rx);
        assert_eq!(read.len(), UI_ERRORS_QUEUED);
        assert_eq!(
            read[0],
            UiEvent::Error(kinds[0].clone()),
            "the oldest first"
        );
        // Reading made room: errors flow again.
        tx.send(error());
        assert_eq!(drain(&mut rx), [error()]);
        assert_eq!(rx.dropped_errors(), 5);
    }

    /// The queue never exceeds its bound, whatever mix arrives.
    #[test]
    fn the_bound_holds_for_any_mix() {
        let (tx, rx) = ui_channel();
        let mut seed = 0x2545_F491_u64;
        for _ in 0..10_000 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            tx.send(if seed >> 60 < 5 {
                error()
            } else {
                phase(Phase::Idle)
            });
            assert!(rx.queued() <= UI_EVENTS_QUEUED);
        }
    }

    #[test]
    fn coalescing_keeps_every_error_and_only_the_newest_phase_in_order() {
        let mut queue = Queue::default();
        queue.events.extend([
            phase(Phase::Recording),
            error(),
            phase(Phase::Transcribing),
            UiEvent::Error(DictationError::EmptyTranscript),
            phase(Phase::Idle),
        ]);
        queue.coalesce();
        assert_eq!(
            Vec::from(queue.events),
            [
                error(),
                UiEvent::Error(DictationError::EmptyTranscript),
                phase(Phase::Idle)
            ]
        );

        let mut queue = Queue::default();
        queue.events.extend([error(), error()]);
        queue.coalesce();
        assert_eq!(queue.events.len(), 2, "no phase to keep, no error lost");

        let mut queue = Queue::default();
        queue.events.push_back(phase(Phase::Idle));
        queue.coalesce();
        assert_eq!(Vec::from(queue.events), [phase(Phase::Idle)]);
    }

    /// #88: a UI that showed the hotkey as down still learns it is back.
    #[test]
    fn coalescing_keeps_the_newest_hotkey_restored_in_order() {
        let mut queue = Queue::default();
        let failed = UiEvent::Error(DictationError::HotkeyFailed);
        queue.events.extend([
            failed.clone(),
            UiEvent::HotkeyRestored,
            phase(Phase::Recording),
            failed.clone(),
            UiEvent::HotkeyRestored,
            phase(Phase::Idle),
        ]);
        queue.coalesce();
        assert_eq!(
            Vec::from(queue.events),
            [
                failed.clone(),
                failed,
                UiEvent::HotkeyRestored,
                phase(Phase::Idle)
            ]
        );
    }

    /// The tray's title after reading everything queued.
    fn tray_title_after(rx: &mut UiReceiver) -> Option<ErrorCode> {
        let mut tray = TrayStatus::default();
        for event in drain(rx) {
            tray.on_event(&event);
        }
        tray.problem()
    }

    fn hotkey_failed() -> UiEvent {
        UiEvent::Error(DictationError::HotkeyFailed)
    }

    /// #154's reproduction: with the error limit nearly reached, an outage,
    /// its return and a newer outage. The tray must end on the newer
    /// outage, not clear.
    #[test]
    fn a_full_error_queue_keeps_the_newest_hotkey_outage() {
        let (tx, mut rx) = ui_channel();
        for _ in 0..UI_ERRORS_QUEUED - 1 {
            tx.send(error());
        }
        tx.send(hotkey_failed());
        tx.send(UiEvent::HotkeyRestored);
        tx.send(hotkey_failed());
        // The first outage was superseded (and counted); the newest is kept.
        assert_eq!(rx.dropped_errors(), 1);
        assert_eq!(rx.queued(), UI_ERRORS_QUEUED);
        assert_eq!(
            tray_title_after(&mut rx),
            Some(ErrorCode::HotkeyUnregistered)
        );
    }

    /// The reverse: the same sequence then one more return. The tray must
    /// end clear.
    #[test]
    fn a_full_error_queue_keeps_the_newest_hotkey_return() {
        let (tx, mut rx) = ui_channel();
        for _ in 0..UI_ERRORS_QUEUED - 1 {
            tx.send(error());
        }
        tx.send(hotkey_failed());
        tx.send(UiEvent::HotkeyRestored);
        tx.send(hotkey_failed());
        tx.send(UiEvent::HotkeyRestored);
        assert_eq!(tray_title_after(&mut rx), None);
    }

    /// An outage after its return was already read, with the limit
    /// reached by other errors: nothing queued to supersede, still kept.
    #[test]
    fn an_outage_is_kept_when_other_errors_fill_the_limit() {
        let (tx, mut rx) = ui_channel();
        tx.send(hotkey_failed());
        tx.send(UiEvent::HotkeyRestored);
        assert_eq!(tray_title_after(&mut rx), None);
        for _ in 0..UI_ERRORS_QUEUED {
            tx.send(error());
        }
        tx.send(hotkey_failed());
        assert_eq!(rx.dropped_errors(), 0);
        assert_eq!(rx.queued(), UI_ERRORS_QUEUED + 1);
        let read = drain(&mut rx);
        assert_eq!(read.last(), Some(&hotkey_failed()));
    }

    /// A listener that keeps failing and coming back while nothing reads
    /// keeps the queue bounded, and the last transition is the real one.
    #[test]
    fn a_flapping_hotkey_stays_bounded_and_ends_on_its_state() {
        for ends_down in [true, false] {
            let (tx, mut rx) = ui_channel();
            for _ in 0..UI_ERRORS_QUEUED {
                tx.send(error());
            }
            for _ in 0..1000 {
                tx.send(hotkey_failed());
                tx.send(phase(Phase::Idle));
                tx.send(UiEvent::HotkeyRestored);
                assert!(rx.queued() <= UI_EVENTS_QUEUED);
            }
            if ends_down {
                tx.send(hotkey_failed());
            }
            let expected = ends_down.then_some(ErrorCode::HotkeyUnregistered);
            assert_eq!(tray_title_after(&mut rx), expected, "ends_down={ends_down}");
        }
    }

    #[test]
    fn dropped_errors_are_counted_and_logged_sparsely() {
        let (tx, _rx) = ui_channel();
        assert_eq!((tx.count_dropped_error(), tx.count_dropped_error()), (1, 2));
        let logged: Vec<u64> = (1..=250).filter(|n| log_this_drop(*n)).collect();
        assert_eq!(logged, [1, 101, 201]);
    }

    #[test]
    fn the_receiver_debug_shows_its_counters() {
        let (tx, rx) = ui_channel();
        tx.send(phase(Phase::Idle));
        let shown = format!("{rx:?}");
        assert!(
            shown.contains("queued: 1") && shown.contains("dropped_errors: 0"),
            "{shown}"
        );
    }

    #[tokio::test]
    async fn recv_waits_for_an_event_and_ends_when_the_sender_is_gone() {
        let (tx, mut rx) = ui_channel();
        let reader = tokio::spawn(async move {
            let first = rx.recv().await;
            let end = rx.recv().await;
            (first, end)
        });
        tokio::task::yield_now().await;
        tx.send(phase(Phase::Recording));
        drop(tx);
        // Bounded: a sender that never closes the channel fails the test.
        let (first, end) = tokio::time::timeout(std::time::Duration::from_secs(5), reader)
            .await
            .expect("recv never ended after the sender was dropped")
            .unwrap();
        assert_eq!(first, Some(phase(Phase::Recording)));
        assert_eq!(end, None);
    }

    #[tokio::test]
    async fn events_sent_before_the_sender_closed_are_still_read() {
        let (tx, mut rx) = ui_channel();
        tx.send(phase(Phase::Idle));
        drop(tx);
        let bounded = std::time::Duration::from_secs(5);
        assert_eq!(
            tokio::time::timeout(bounded, rx.recv()).await,
            Ok(Some(phase(Phase::Idle)))
        );
        assert_eq!(tokio::time::timeout(bounded, rx.recv()).await, Ok(None));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    }
}
