//! macOS keyboard listener using CGEventTap

use std::cell::Cell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventSource, CGEventSourceStateID,
    CGEventTapCallBack, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType,
};

use crate::error::{Error, Result};
use crate::platform::state::{BlockingHotkeys, ListenerState};
use crate::types::{Key, KeyEvent, Modifiers};

use super::keycode::{flags_have_alpha_shift, flags_have_fn, keycode_to_key, keycode_to_modifier};
use super::permissions::check_accessibility;

/// The tap thread's run loop, handed back to the public listener so Drop can
/// stop it. CFRunLoop is one of the documented thread-safe CF types; objc2
/// doesn't mark it Send, hence the wrapper.
pub(crate) struct TapRunLoop(CFRetained<CFRunLoop>);

unsafe impl Send for TapRunLoop {}

impl TapRunLoop {
    /// Stop the tap thread's run loop. Callable from any thread.
    ///
    /// A bare CFRunLoopStop only affects a loop that is currently between
    /// entry and exit — the stop flag lives in per-run data that is reset on
    /// every entry — so a stop landing in the gap between the `running`
    /// check and CFRunLoopRun entering would be silently lost and the tap
    /// thread would park forever. Enqueue the stop as a run-loop block
    /// instead: the block queue persists across entries, so the request
    /// executes either right away (after the wake-up) or at the next entry.
    /// Both behaviors verified empirically on macOS 26.5: a pre-entry
    /// CFRunLoopStop is a no-op, a pre-entry enqueued block stops the loop
    /// the moment it is entered.
    pub(crate) fn stop(&self) {
        let block = block2::StackBlock::new(|| {
            if let Some(rl) = CFRunLoop::current() {
                rl.stop();
            }
        });
        unsafe {
            self.0.perform_block(
                objc2_core_foundation::kCFRunLoopCommonModes.map(|m| m.as_ref()),
                Some(&block),
            );
        }
        self.0.wake_up();
    }
}

/// Internal listener state returned to KeyboardListener
pub(crate) struct MacOSListenerState {
    pub event_receiver: Receiver<KeyEvent>,
    pub thread_handle: Option<JoinHandle<()>>,
    pub running: Arc<AtomicBool>,
    pub blocking_hotkeys: Option<BlockingHotkeys>,
    pub run_loop: TapRunLoop,
}

/// Spawn a macOS keyboard listener using CGEventTap
pub(crate) fn spawn(blocking_hotkeys: Option<BlockingHotkeys>) -> Result<MacOSListenerState> {
    if !check_accessibility() {
        return Err(Error::AccessibilityNotGranted);
    }

    let (tx, rx) = mpsc::channel();
    let state = Arc::new(Mutex::new(ListenerState::new(tx, blocking_hotkeys.clone())));
    let running = Arc::new(AtomicBool::new(true));

    // Channel reporting event tap creation: the tap thread's run loop on
    // success (needed to stop it later), or an error message.
    let (init_tx, init_rx) = mpsc::channel::<std::result::Result<TapRunLoop, String>>();

    let thread_state = Arc::clone(&state);
    let thread_running = Arc::clone(&running);

    let handle = thread::spawn(move || {
        run_event_tap(thread_state, thread_running, init_tx);
    });

    // Wait for the event tap to be created
    let run_loop = match init_rx.recv() {
        Ok(Ok(run_loop)) => run_loop,
        Ok(Err(msg)) => {
            return Err(Error::EventTapCreationFailed(msg));
        }
        Err(_) => {
            return Err(Error::EventTapCreationFailed(
                "Event tap thread terminated unexpectedly".to_string(),
            ));
        }
    };

    Ok(MacOSListenerState {
        event_receiver: rx,
        thread_handle: Some(handle),
        running,
        blocking_hotkeys,
        run_loop,
    })
}

/// Patched (dictation-app): one modifier group of CGEventFlags with the
/// device-dependent bits (IOKit's `NX_DEVICE*KEYMASK`) that tell its left
/// and right keys apart. Hardware keyboard events carry them; events posted
/// by other apps may not.
struct ModifierGroup {
    mask: CGEventFlags,
    left_device: u64,
    right_device: u64,
    left: Modifiers,
    right: Modifiers,
}

const MODIFIER_GROUPS: [ModifierGroup; 4] = [
    ModifierGroup {
        mask: CGEventFlags::MaskControl,
        left_device: 0x0000_0001,
        right_device: 0x0000_2000,
        left: Modifiers::CTRL_LEFT,
        right: Modifiers::CTRL_RIGHT,
    },
    ModifierGroup {
        mask: CGEventFlags::MaskShift,
        left_device: 0x0000_0002,
        right_device: 0x0000_0004,
        left: Modifiers::SHIFT_LEFT,
        right: Modifiers::SHIFT_RIGHT,
    },
    ModifierGroup {
        mask: CGEventFlags::MaskCommand,
        left_device: 0x0000_0008,
        right_device: 0x0000_0010,
        left: Modifiers::CMD_LEFT,
        right: Modifiers::CMD_RIGHT,
    },
    ModifierGroup {
        mask: CGEventFlags::MaskAlternate,
        left_device: 0x0000_0020,
        right_device: 0x0000_0040,
        left: Modifiers::OPT_LEFT,
        right: Modifiers::OPT_RIGHT,
    },
];

/// Patched (dictation-app): the side-specific modifiers held according to
/// `flags`, which are authoritative. Per group: not in the flags, neither
/// side; device bits present, exactly those sides; otherwise (no side
/// information) the sides in `tracked`, or the left key if none.
///
/// Corrects drift from missed events (tap disabled, Mission Control, screen
/// lock) and gives the snapshot when the listener starts. On FlagsChanged
/// the flags already include the change, so this is the state after it.
fn modifiers_from_flags(tracked: Modifiers, flags: CGEventFlags) -> Modifiers {
    let mut held = Modifiers::empty();
    for group in &MODIFIER_GROUPS {
        let both = group.left | group.right;
        if !flags.contains(group.mask) {
            continue;
        }
        let left = flags.0 & group.left_device != 0;
        let right = flags.0 & group.right_device != 0;
        if left || right {
            held.set(group.left, left);
            held.set(group.right, right);
        } else if tracked.intersects(both) {
            held |= tracked & both;
        } else {
            held |= group.left;
        }
    }
    held
}

/// Patched (dictation-app): the tracked modifiers after a FlagsChanged event
/// for the side-specific modifier key `changed`. The event's flags decide:
/// a group absent from them is up whatever was tracked, so a key held when
/// the listener started can't come out as a press when it is let go. Only
/// when the group is held and the flags carry no side bits (some synthetic
/// events) does the changed key toggle, as upstream always did.
fn after_flags_changed(tracked: Modifiers, changed: Modifiers, flags: CGEventFlags) -> Modifiers {
    let mut held = modifiers_from_flags(tracked, flags);
    if let Some(group) = MODIFIER_GROUPS
        .iter()
        .find(|g| changed == g.left || changed == g.right)
    {
        let both = group.left | group.right;
        let sided = flags.0 & (group.left_device | group.right_device) != 0;
        if flags.contains(group.mask) && !sided {
            held.remove(both);
            held |= tracked & both;
            held.toggle(changed);
        }
    }
    held
}

/// Context handed to the event tap callback through the refcon pointer.
///
/// `tap` is filled in right after `CGEventTapCreate` returns. The callback and
/// `run_event_tap` execute on the same thread (callbacks only fire while that
/// thread runs its run loop), so a plain `Cell` is sound.
struct CallbackCtx {
    state: Arc<Mutex<ListenerState>>,
    tap: Cell<Option<NonNull<CFMachPort>>>,
}

/// The callback function for the event tap
///
/// Returns NULL to block the event, or the event pointer to pass it through.
unsafe extern "C-unwind" fn event_tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    // Safety: user_info is the CallbackCtx owned by run_event_tap
    let ctx = &*(user_info as *const CallbackCtx);

    // Handle tap-disable pseudo-events before touching the state lock: macOS
    // stops delivering real events until the tap is re-enabled, so recovery
    // must not depend on the mutex being healthy. Re-enabling from the
    // callback (rather than polling CGEventTapIsEnabled from the run loop) is
    // the Apple-documented pattern and avoids recurring WindowServer RPCs —
    // those leak kernel IPC vouchers and eventually panic the whole machine
    // (cjpais/Handy#1827).
    if matches!(
        event_type,
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
    ) {
        if let Some(tap) = ctx.tap.get() {
            CGEvent::tap_enable(tap.as_ref(), true);
        }
        // Reconcile modifiers: we may have missed events while disabled.
        // Patched (dictation-app): and say so, since presses and releases
        // may have been lost and no further event may come to show it.
        if let Ok(mut state) = ctx.state.lock() {
            state.current_modifiers = modifiers_from_flags(
                state.current_modifiers,
                CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState),
            );
            let _ = state
                .event_sender
                .send(KeyEvent::resync(state.current_modifiers));
        }
        return event.as_ptr();
    }

    let cg_event = event.as_ref();
    let flags = CGEvent::flags(Some(cg_event));

    let mut should_block = false;

    if let Ok(mut state) = ctx.state.lock() {
        // Reconcile tracked modifiers against OS flags for non-FlagsChanged events.
        // On FlagsChanged, flags reflect the state *after* the current change, so
        // reconciling would fight with the toggle logic.
        // Patched (dictation-app): a correction means a modifier change was
        // missed; report it before the event itself.
        if event_type != CGEventType::FlagsChanged {
            let reconciled = modifiers_from_flags(state.current_modifiers, flags);
            if reconciled != state.current_modifiers {
                state.current_modifiers = reconciled;
                let _ = state.event_sender.send(KeyEvent::resync(reconciled));
            }
        }

        // Build side-specific modifiers from internally tracked state + FN from flags
        let modifiers = if flags_have_fn(flags) {
            state.current_modifiers | Modifiers::FN
        } else {
            state.current_modifiers & !Modifiers::FN
        };

        match event_type {
            CGEventType::KeyDown => {
                let keycode = CGEvent::integer_value_field(
                    Some(cg_event),
                    CGEventField::KeyboardEventKeycode,
                ) as u16;

                let key = keycode_to_key(keycode);

                // Skip keycodes we can't map (e.g. special function keys like
                // Mission Control's 0xA0, or the Globe key's configured action).
                // A key-less KeyEvent carries no usable information, and the
                // Windows/Linux backends already emit mapped keys only.
                if key.is_none() {
                    return event.as_ptr();
                }

                // Check if this should be blocked
                should_block = state.should_block(modifiers, key);

                let _ = state.event_sender.send(KeyEvent {
                    modifiers,
                    key,
                    is_key_down: true,
                    changed_modifier: None,
                });
            }
            CGEventType::KeyUp => {
                let keycode = CGEvent::integer_value_field(
                    Some(cg_event),
                    CGEventField::KeyboardEventKeycode,
                ) as u16;

                let key = keycode_to_key(keycode);

                // Skip unmapped keycodes (same as KeyDown)
                if key.is_none() {
                    return event.as_ptr();
                }

                // Block key up if we blocked key down (to be consistent)
                should_block = state.should_block(modifiers, key);

                let _ = state.event_sender.send(KeyEvent {
                    modifiers,
                    key,
                    is_key_down: false,
                    changed_modifier: None,
                });
            }
            CGEventType::FlagsChanged => {
                let keycode = CGEvent::integer_value_field(
                    Some(cg_event),
                    CGEventField::KeyboardEventKeycode,
                ) as u16;

                let changed_modifier = keycode_to_modifier(keycode);

                // Check if this is a lock key (e.g., Caps Lock) which comes through
                // as FlagsChanged but isn't a traditional modifier
                let lock_key = keycode_to_key(keycode);

                // Patched (dictation-app): the flags describe every
                // modifier, not just this event's key. Reconcile them all;
                // any change other than this key's own means a press or
                // release was missed, so report it before the event (#85,
                // review round 1, B1). Caps Lock and Fn change no tracked
                // modifier, so for them every change is unexplained.
                let explained = match changed_modifier {
                    Some(bit) if bit != Modifiers::FN => bit,
                    _ => Modifiers::empty(),
                };
                let before = state.current_modifiers;
                let after = if explained.is_empty() {
                    modifiers_from_flags(before, flags)
                } else {
                    after_flags_changed(before, explained, flags)
                };
                if (before ^ after) & !explained != Modifiers::empty() {
                    // The missed changes alone; this key's comes next.
                    let _ = state.event_sender.send(KeyEvent::resync(
                        (after & !explained) | (before & explained),
                    ));
                }
                state.current_modifiers = after;
                // As reconciled; Fn is never tracked.
                let modifiers = if flags_have_fn(flags) {
                    after | Modifiers::FN
                } else {
                    after
                };

                // Handle lock keys specially - they come through FlagsChanged
                // but don't change our tracked modifier state
                if let Some(key) = lock_key {
                    let is_key_down = flags_have_alpha_shift(flags);

                    should_block = state.should_block(modifiers, Some(key));

                    let _ = state.event_sender.send(KeyEvent {
                        modifiers,
                        key: Some(key),
                        is_key_down,
                        changed_modifier: None,
                    });
                } else if let Some(modifier_bit) = changed_modifier {
                    // Patched (dictation-app): up or down from the event's
                    // flags, not by toggling the remembered bit (#80, B1).
                    // FN has no side and is never tracked: its flag decides.
                    // The tracked state was updated above.
                    let is_key_down = if modifier_bit == Modifiers::FN {
                        flags_have_fn(flags)
                    } else {
                        state.current_modifiers.contains(modifier_bit)
                    };

                    // Re-derive modifiers after update (include FN from flags)
                    let new_modifiers = if flags_have_fn(flags) {
                        state.current_modifiers | Modifiers::FN
                    } else {
                        state.current_modifiers & !Modifiers::FN
                    };

                    // Check if this modifier-only combo should be blocked
                    if is_key_down {
                        should_block = state.should_block(new_modifiers, None);
                    }

                    let _ = state.event_sender.send(KeyEvent {
                        modifiers: new_modifiers,
                        key: None,
                        is_key_down,
                        changed_modifier,
                    });
                } else if keycode == 0x3F {
                    // FN key itself — tracked via flags, not keycode state
                    let had_fn = modifiers.contains(Modifiers::FN);
                    let has_fn = flags_have_fn(flags);
                    if had_fn != has_fn {
                        let new_modifiers = if has_fn {
                            state.current_modifiers | Modifiers::FN
                        } else {
                            state.current_modifiers & !Modifiers::FN
                        };

                        if has_fn {
                            should_block = state.should_block(new_modifiers, None);
                        }

                        let _ = state.event_sender.send(KeyEvent {
                            modifiers: new_modifiers,
                            key: None,
                            is_key_down: has_fn,
                            changed_modifier: Some(Modifiers::FN),
                        });
                    }
                }
            }
            // Mouse button events
            // Only report left/right clicks when modifiers are held (to avoid noise)
            CGEventType::LeftMouseDown if !modifiers.is_empty() => {
                let _ = state.event_sender.send(KeyEvent {
                    modifiers,
                    key: Some(Key::MouseLeft),
                    is_key_down: true,
                    changed_modifier: None,
                });
            }
            CGEventType::LeftMouseUp if !modifiers.is_empty() => {
                let _ = state.event_sender.send(KeyEvent {
                    modifiers,
                    key: Some(Key::MouseLeft),
                    is_key_down: false,
                    changed_modifier: None,
                });
            }
            CGEventType::RightMouseDown if !modifiers.is_empty() => {
                let _ = state.event_sender.send(KeyEvent {
                    modifiers,
                    key: Some(Key::MouseRight),
                    is_key_down: true,
                    changed_modifier: None,
                });
            }
            CGEventType::RightMouseUp if !modifiers.is_empty() => {
                let _ = state.event_sender.send(KeyEvent {
                    modifiers,
                    key: Some(Key::MouseRight),
                    is_key_down: false,
                    changed_modifier: None,
                });
            }
            // Pass through unmodified left/right clicks
            CGEventType::LeftMouseDown
            | CGEventType::LeftMouseUp
            | CGEventType::RightMouseDown
            | CGEventType::RightMouseUp => {}
            CGEventType::OtherMouseDown => {
                let button_number = CGEvent::integer_value_field(
                    Some(cg_event),
                    CGEventField::MouseEventButtonNumber,
                );
                let key = match button_number {
                    2 => Some(Key::MouseMiddle),
                    3 => Some(Key::MouseX1),
                    4 => Some(Key::MouseX2),
                    _ => None, // Unknown button
                };
                if let Some(key) = key {
                    let _ = state.event_sender.send(KeyEvent {
                        modifiers,
                        key: Some(key),
                        is_key_down: true,
                        changed_modifier: None,
                    });
                }
            }
            CGEventType::OtherMouseUp => {
                let button_number = CGEvent::integer_value_field(
                    Some(cg_event),
                    CGEventField::MouseEventButtonNumber,
                );
                let key = match button_number {
                    2 => Some(Key::MouseMiddle),
                    3 => Some(Key::MouseX1),
                    4 => Some(Key::MouseX2),
                    _ => None,
                };
                if let Some(key) = key {
                    let _ = state.event_sender.send(KeyEvent {
                        modifiers,
                        key: Some(key),
                        is_key_down: false,
                        changed_modifier: None,
                    });
                }
            }
            // TapDisabledByTimeout/TapDisabledByUserInput are handled (and
            // returned from) before the state lock above.
            _ => {}
        }
    }

    if should_block {
        // Block the event from reaching other applications
        std::ptr::null_mut()
    } else {
        // Pass the event through unchanged
        event.as_ptr()
    }
}

/// Run the event tap in a dedicated thread
fn run_event_tap(
    state: Arc<Mutex<ListenerState>>,
    running: Arc<AtomicBool>,
    init_tx: Sender<std::result::Result<TapRunLoop, String>>,
) {
    // Event types we want to monitor
    let event_mask: CGEventMask = (1 << CGEventType::KeyDown.0)
        | (1 << CGEventType::KeyUp.0)
        | (1 << CGEventType::FlagsChanged.0)
        // Mouse buttons
        | (1 << CGEventType::LeftMouseDown.0)
        | (1 << CGEventType::LeftMouseUp.0)
        | (1 << CGEventType::RightMouseDown.0)
        | (1 << CGEventType::RightMouseUp.0)
        | (1 << CGEventType::OtherMouseDown.0)
        | (1 << CGEventType::OtherMouseUp.0);

    // Callback context, handed to the tap as a raw refcon pointer
    let ctx_ptr = Box::into_raw(Box::new(CallbackCtx {
        state: Arc::clone(&state),
        tap: Cell::new(None),
    }));

    let callback: CGEventTapCallBack = Some(event_tap_callback);

    // Use Default mode (not ListenOnly) to enable optional event blocking
    let tap: Option<CFRetained<CFMachPort>> = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::SessionEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            event_mask,
            callback,
            ctx_ptr as *mut c_void,
        )
    };

    let tap = match tap {
        Some(t) => t,
        None => {
            // Cleanup
            unsafe {
                drop(Box::from_raw(ctx_ptr));
            }
            let _ = init_tx.send(Err(
                "Failed to create event tap. Your terminal app may need accessibility permission in System Settings > Privacy & Security > Accessibility".to_string()
            ));
            return;
        }
    };

    // Give the callback the tap port so it can re-enable on TapDisabledBy*
    // pseudo-events. No race: callbacks only run on this thread's run loop,
    // which hasn't started yet.
    unsafe { (*ctx_ptr).tap.set(Some(NonNull::from(&*tap))) };

    // Create run loop source
    let source: Option<CFRetained<CFRunLoopSource>> =
        CFMachPort::new_run_loop_source(None, Some(&tap), 0);

    let source = match source {
        Some(s) => s,
        None => {
            unsafe {
                CFMachPort::invalidate(&tap);
                drop(Box::from_raw(ctx_ptr));
            }
            let _ = init_tx.send(Err("Failed to create run loop source".to_string()));
            return;
        }
    };

    // Get the current run loop and add the source
    let run_loop = CFRunLoop::current();

    // Unwrap the Option<CFRetained<CFRunLoop>> - current() should always succeed on a valid thread
    let run_loop = match run_loop {
        Some(rl) => rl,
        None => {
            unsafe {
                CFMachPort::invalidate(&tap);
                drop(Box::from_raw(ctx_ptr));
            }
            let _ = init_tx.send(Err("Failed to get current run loop".to_string()));
            return;
        }
    };

    run_loop.add_source(Some(&source), unsafe {
        objc2_core_foundation::kCFRunLoopCommonModes
    });
    // Patched (dictation-app): start from the modifiers already held, so
    // letting go of one isn't read as a press (#80, B1).
    if let Ok(mut state) = state.lock() {
        state.current_modifiers = modifiers_from_flags(
            Modifiers::empty(),
            CGEventSource::flags_state(CGEventSourceStateID::CombinedSessionState),
        );
    }
    CGEvent::tap_enable(&tap, true);

    // Signal successful initialization, handing back this thread's run loop
    // so KeyboardListener::drop can stop it.
    let _ = init_tx.send(Ok(TapRunLoop(run_loop.clone())));

    // Park in the run loop: no periodic wakeups, no watchdog. v0.3.2 cycled
    // CFRunLoopRunInMode every 100ms and polled CGEventTapIsEnabled each
    // pass; that RPC leaked kernel IPC vouchers until macOS panicked with
    // "Cannot grow ipc space beyond IVAC_ENTRIES_MAX" (cjpais/Handy#1827),
    // and even the bare 100ms cycling cost ~10 mach messages/sec. Tap
    // recovery is event-driven in event_tap_callback; shutdown goes through
    // TapRunLoop::stop. Guarded by `running` in case the loop returns
    // without a stop request; bail out if the tap's port died (e.g. the
    // accessibility permission was revoked) so we don't spin on a dead
    // source.
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        CFRunLoop::run();

        if !CFMachPort::is_valid(&tap) {
            break;
        }
    }

    // Cleanup
    run_loop.remove_source(Some(&source), unsafe {
        objc2_core_foundation::kCFRunLoopCommonModes
    });
    CGEvent::tap_enable(&tap, false);
    CFMachPort::invalidate(&tap);
    unsafe {
        drop(Box::from_raw(ctx_ptr));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    use objc2_core_foundation::CFRunLoopTimer;

    unsafe extern "C-unwind" fn noop_timer(_timer: *mut CFRunLoopTimer, _info: *mut c_void) {}

    /// The stop request must survive being issued before the run loop has
    /// been entered. A bare CFRunLoopStop is lost in that window (the stop
    /// flag lives in per-run data reset on entry), which would park the tap
    /// thread forever and hang KeyboardListener::drop. Needs no
    /// accessibility permission, so it runs everywhere including CI.
    #[test]
    fn stop_requested_before_run_entry_is_not_lost() {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let rl = CFRunLoop::current().expect("thread has a run loop");

            // A far-future timer keeps the mode non-empty so run_in_mode
            // actually blocks instead of returning Finished immediately.
            let timer = unsafe {
                CFRunLoopTimer::new(
                    None,
                    objc2_core_foundation::CFAbsoluteTimeGetCurrent() + 3600.0,
                    0.0,
                    0,
                    0,
                    Some(noop_timer),
                    std::ptr::null_mut(),
                )
            }
            .expect("create timer");
            rl.add_timer(Some(&timer), unsafe {
                objc2_core_foundation::kCFRunLoopDefaultMode
            });

            tx.send(TapRunLoop(rl.clone())).unwrap();

            // Guarantee the main thread's stop lands while this thread is
            // provably outside the run loop.
            thread::sleep(Duration::from_millis(200));

            let started = Instant::now();
            // Bounded run: if the stop request was lost this returns at the
            // 5s timeout and the elapsed assertion below fails, instead of
            // hanging the test.
            CFRunLoop::run_in_mode(
                unsafe { objc2_core_foundation::kCFRunLoopDefaultMode },
                5.0,
                false,
            );
            started.elapsed()
        });

        let run_loop = rx.recv().expect("run loop handle");
        run_loop.stop(); // lands before the thread enters the run loop

        let elapsed = handle.join().expect("tap thread panicked");
        assert!(
            elapsed < Duration::from_secs(2),
            "stop issued before run-loop entry was lost; loop ran for {elapsed:?}"
        );
    }

    unsafe extern "C-unwind" fn noop_callback(
        _proxy: CGEventTapProxy,
        _event_type: CGEventType,
        event: NonNull<CGEvent>,
        _user_info: *mut c_void,
    ) -> *mut CGEvent {
        event.as_ptr()
    }

    /// A real (listen-only) event tap, or None when the test runner has no
    /// accessibility permission (CI, unprivileged terminals) — tests skip.
    fn create_test_tap() -> Option<CFRetained<CFMachPort>> {
        if !check_accessibility() {
            eprintln!("SKIPPED: accessibility permission not granted to the test runner");
            return None;
        }
        unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                1 << CGEventType::KeyDown.0,
                Some(noop_callback),
                std::ptr::null_mut(),
            )
        }
    }

    fn make_ctx(tap: &CFRetained<CFMachPort>) -> CallbackCtx {
        let (tx, _rx) = mpsc::channel();
        CallbackCtx {
            state: Arc::new(Mutex::new(ListenerState::new(tx, None))),
            tap: Cell::new(Some(NonNull::from(&**tap))),
        }
    }

    fn deliver_tap_disabled(ctx: &CallbackCtx, event_type: CGEventType) {
        let event = CGEvent::new(None).expect("failed to create CGEvent");
        let returned = unsafe {
            event_tap_callback(
                std::ptr::null_mut(),
                event_type,
                NonNull::from(&*event),
                ctx as *const CallbackCtx as *mut c_void,
            )
        };
        assert_eq!(
            returned,
            NonNull::from(&*event).as_ptr(),
            "pseudo-events must be passed through, not blocked"
        );
    }

    /// Recovery is event-driven now (no polling watchdog): receiving a
    /// TapDisabledBy* pseudo-event must re-enable the tap from the callback.
    #[test]
    fn tap_disabled_pseudo_event_reenables_tap() {
        let Some(tap) = create_test_tap() else { return };

        for event_type in [
            CGEventType::TapDisabledByTimeout,
            CGEventType::TapDisabledByUserInput,
        ] {
            let ctx = make_ctx(&tap);
            CGEvent::tap_enable(&tap, false);
            assert!(!CGEvent::tap_is_enabled(&tap));

            deliver_tap_disabled(&ctx, event_type);

            assert!(
                CGEvent::tap_is_enabled(&tap),
                "callback must re-enable the tap after {event_type:?}"
            );
        }
    }

    /// Re-enabling must not depend on the state mutex being healthy: a
    /// poisoned mutex (a panic on some other thread holding it) must not
    /// leave the tap permanently dead.
    #[test]
    fn tap_reenabled_even_with_poisoned_state_mutex() {
        let Some(tap) = create_test_tap() else { return };

        let ctx = make_ctx(&tap);
        let state = Arc::clone(&ctx.state);
        let _ = thread::spawn(move || {
            let _guard = state.lock().unwrap();
            panic!("poison the listener state mutex");
        })
        .join();
        assert!(ctx.state.lock().is_err(), "mutex should be poisoned");

        CGEvent::tap_enable(&tap, false);
        assert!(!CGEvent::tap_is_enabled(&tap));

        deliver_tap_disabled(&ctx, CGEventType::TapDisabledByTimeout);

        assert!(
            CGEvent::tap_is_enabled(&tap),
            "re-enable must happen even when the state lock is unusable"
        );
    }

    /// The pseudo-event handler must reconcile modifier state (the original
    /// purpose of PR #4): stale tracked modifiers with no physically-held
    /// keys must be cleared.
    #[test]
    fn tap_disabled_pseudo_event_reconciles_modifiers() {
        let Some(tap) = create_test_tap() else { return };

        let ctx = make_ctx(&tap);
        ctx.state.lock().unwrap().current_modifiers = Modifiers::CTRL_LEFT | Modifiers::SHIFT_RIGHT;

        deliver_tap_disabled(&ctx, CGEventType::TapDisabledByTimeout);

        // No modifier keys are physically held while tests run, so
        // reconciliation against OS flags must clear the stale state.
        assert_eq!(
            ctx.state.lock().unwrap().current_modifiers,
            Modifiers::empty(),
            "stale modifiers must be reconciled away on tap-disable"
        );
    }
}

/// Patched (dictation-app): the M1.6 fixes (#80) through the real callback,
/// with local `CGEvent`s only. No tap is created and nothing is posted, so
/// these need no permission and don't touch the desktop.
#[cfg(test)]
mod dictation_patch_tests {
    use super::*;

    const OPTION: i64 = 0x3A;
    const RIGHT_OPTION: i64 = 0x3D;
    const FUNCTION: i64 = 0x3F;
    const KEY_A: i64 = 0x00;
    const LEFT_OPT_DEVICE: u64 = 0x20;
    const RIGHT_OPT_DEVICE: u64 = 0x40;
    const NONE: CGEventFlags = CGEventFlags(0);

    fn alt(device: u64) -> CGEventFlags {
        CGEventFlags(CGEventFlags::MaskAlternate.0 | device)
    }

    /// The real callback with `tracked` modifiers and no tap, and the
    /// events it sends.
    struct Callback {
        ctx: CallbackCtx,
        rx: Receiver<KeyEvent>,
    }

    impl Callback {
        fn new(tracked: Modifiers) -> Self {
            let (tx, rx) = mpsc::channel();
            let mut state = ListenerState::new(tx, None);
            state.current_modifiers = tracked;
            let ctx = CallbackCtx {
                state: Arc::new(Mutex::new(state)),
                tap: Cell::new(None),
            };
            Self { ctx, rx }
        }

        /// Delivers one event and returns what the callback sent.
        fn deliver(&self, kind: CGEventType, flags: CGEventFlags, keycode: i64) -> Vec<KeyEvent> {
            let event = CGEvent::new_keyboard_event(None, keycode as u16, true).unwrap();
            CGEvent::set_type(Some(&event), kind);
            CGEvent::set_flags(Some(&event), flags);
            CGEvent::set_integer_value_field(
                Some(&event),
                CGEventField::KeyboardEventKeycode,
                keycode,
            );
            let returned = unsafe {
                event_tap_callback(
                    std::ptr::null_mut(),
                    kind,
                    NonNull::from(&*event),
                    &self.ctx as *const CallbackCtx as *mut c_void,
                )
            };
            assert!(!returned.is_null(), "a non-blocking listener never blocks");
            self.rx.try_iter().collect()
        }

        /// A FlagsChanged event: (down, modifiers after it).
        fn flags_changed(&self, flags: CGEventFlags, keycode: i64) -> (bool, Modifiers) {
            let events = self.deliver(CGEventType::FlagsChanged, flags, keycode);
            assert_eq!(events.len(), 1, "{events:?}");
            (events[0].is_key_down, events[0].modifiers)
        }
    }

    #[test]
    fn option_held_at_start_then_released_is_a_release() {
        // Tracked empty: the worst case, as if the start snapshot missed it.
        for (keycode, device, side) in [
            (RIGHT_OPTION, RIGHT_OPT_DEVICE, Modifiers::OPT_RIGHT),
            (OPTION, LEFT_OPT_DEVICE, Modifiers::OPT_LEFT),
        ] {
            let cb = Callback::new(Modifiers::empty());
            assert_eq!(cb.flags_changed(NONE, keycode), (false, Modifiers::empty()));
            // The next press and release pair is read correctly.
            assert_eq!(cb.flags_changed(alt(device), keycode), (true, side));
            assert_eq!(cb.flags_changed(NONE, keycode), (false, Modifiers::empty()));
        }
    }

    #[test]
    fn right_option_released_while_left_option_stays_held() {
        let cb = Callback::new(Modifiers::empty());
        // Both held at start and the snapshot missed them; Right goes up
        // first. Left's press was never seen: a resync, then the release.
        let events = cb.deliver(
            CGEventType::FlagsChanged,
            alt(LEFT_OPT_DEVICE),
            RIGHT_OPTION,
        );
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(events[0].is_resync());
        assert_eq!(events[0].modifiers, Modifiers::OPT_LEFT);
        assert!(!events[1].is_key_down);
        assert_eq!(events[1].modifiers, Modifiers::OPT_LEFT);
        assert_eq!(events[1].changed_modifier, Some(Modifiers::OPT_RIGHT));
        assert_eq!(
            cb.flags_changed(alt(LEFT_OPT_DEVICE | RIGHT_OPT_DEVICE), RIGHT_OPTION),
            (true, Modifiers::OPT_LEFT | Modifiers::OPT_RIGHT)
        );
        assert_eq!(
            cb.flags_changed(alt(RIGHT_OPT_DEVICE), OPTION),
            (false, Modifiers::OPT_RIGHT)
        );
        assert_eq!(
            cb.flags_changed(NONE, RIGHT_OPTION),
            (false, Modifiers::empty())
        );
    }

    #[test]
    fn flags_without_side_bits_fall_back_to_toggling() {
        // Events posted without device bits still pair up.
        let cb = Callback::new(Modifiers::empty());
        let bare = alt(0);
        assert_eq!(
            cb.flags_changed(bare, RIGHT_OPTION),
            (true, Modifiers::OPT_RIGHT)
        );
        assert_eq!(
            cb.flags_changed(bare, OPTION),
            (true, Modifiers::OPT_LEFT | Modifiers::OPT_RIGHT)
        );
        assert_eq!(
            cb.flags_changed(bare, RIGHT_OPTION),
            (false, Modifiers::OPT_LEFT)
        );
        // A group missing from the flags is up, without side bits too.
        assert_eq!(cb.flags_changed(NONE, OPTION), (false, Modifiers::empty()));
        assert_eq!(cb.flags_changed(NONE, OPTION), (false, Modifiers::empty()));
    }

    #[test]
    fn fn_follows_its_flag() {
        let cb = Callback::new(Modifiers::empty());
        let fn_flag = CGEventFlags::MaskSecondaryFn;
        assert_eq!(cb.flags_changed(fn_flag, FUNCTION), (true, Modifiers::FN));
        assert_eq!(
            cb.flags_changed(NONE, FUNCTION),
            (false, Modifiers::empty())
        );
        assert_eq!(
            cb.flags_changed(NONE, FUNCTION),
            (false, Modifiers::empty())
        );
    }

    #[test]
    fn tap_disabled_sends_a_resync_after_what_came_before() {
        for kind in [
            CGEventType::TapDisabledByTimeout,
            CGEventType::TapDisabledByUserInput,
        ] {
            let cb = Callback::new(Modifiers::empty());
            cb.flags_changed(alt(RIGHT_OPT_DEVICE), RIGHT_OPTION);
            // The release is lost while the tap is off; nothing else comes.
            let events = cb.deliver(kind, NONE, 0);
            assert_eq!(events.len(), 1, "{kind:?}: {events:?}");
            assert!(events[0].is_resync(), "{kind:?}: {events:?}");
            // Its modifiers come from the host's real keyboard state, which
            // this test doesn't control, so they aren't checked.
        }
    }

    #[test]
    fn a_missed_release_seen_on_a_key_event_sends_a_resync_first() {
        let cb = Callback::new(Modifiers::OPT_RIGHT);
        let events = cb.deliver(CGEventType::KeyDown, NONE, KEY_A);
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(events[0].is_resync());
        assert_eq!(events[0].modifiers, Modifiers::empty());
        assert_eq!(events[1].key, Some(Key::A));
        assert!(!events[1].is_resync());
    }

    #[test]
    fn a_key_event_that_agrees_with_the_tracked_state_sends_no_resync() {
        let cb = Callback::new(Modifiers::OPT_RIGHT);
        let events = cb.deliver(CGEventType::KeyDown, alt(RIGHT_OPT_DEVICE), KEY_A);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].modifiers, Modifiers::OPT_RIGHT);
        // Without side bits the tracked side stands.
        let events = cb.deliver(CGEventType::KeyUp, alt(0), KEY_A);
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].modifiers, Modifiers::OPT_RIGHT);
    }

    #[test]
    fn modifiers_from_flags_reads_sides() {
        let all_device = CGEventFlags(
            CGEventFlags::MaskControl.0
                | CGEventFlags::MaskShift.0
                | CGEventFlags::MaskCommand.0
                | 0x2000
                | 0x02
                | 0x10,
        );
        assert_eq!(
            modifiers_from_flags(Modifiers::all(), all_device),
            Modifiers::CTRL_RIGHT | Modifiers::SHIFT_LEFT | Modifiers::CMD_RIGHT
        );
        // No side bits: keep what was tracked, or guess left.
        let bare = CGEventFlags(CGEventFlags::MaskShift.0 | CGEventFlags::MaskCommand.0);
        assert_eq!(
            modifiers_from_flags(Modifiers::SHIFT_RIGHT, bare),
            Modifiers::SHIFT_RIGHT | Modifiers::CMD_LEFT
        );
        assert_eq!(
            modifiers_from_flags(Modifiers::all(), NONE),
            Modifiers::empty()
        );
    }

    const SHIFT: i64 = 0x38;
    const RIGHT_SHIFT: i64 = 0x3C;
    const COMMAND: i64 = 0x37;
    const RIGHT_COMMAND: i64 = 0x36;
    const CAPS_LOCK: i64 = 0x39;

    /// #85 review round 1, B1: Right Option's release is missed and the
    /// next event is another modifier key's, whose flags no longer have
    /// Option. That must come out as a resync, not as Option's release.
    #[test]
    fn a_missed_release_seen_on_another_modifier_sends_a_resync_first() {
        let shift = CGEventFlags::MaskShift.0;
        let cmd = CGEventFlags::MaskCommand.0;
        let caps = CGEventFlags::MaskAlphaShift.0;
        let fn_flag = CGEventFlags::MaskSecondaryFn.0;
        // (name, keycode, flags, the event that follows the resync)
        let cases = [
            (
                "left shift",
                SHIFT,
                shift | 0x02,
                Some(Modifiers::SHIFT_LEFT),
            ),
            (
                "right shift",
                RIGHT_SHIFT,
                shift | 0x04,
                Some(Modifiers::SHIFT_RIGHT),
            ),
            (
                "shift, no side bits",
                SHIFT,
                shift,
                Some(Modifiers::SHIFT_LEFT),
            ),
            ("left cmd", COMMAND, cmd | 0x08, Some(Modifiers::CMD_LEFT)),
            (
                "right cmd, no side bits",
                RIGHT_COMMAND,
                cmd,
                Some(Modifiers::CMD_RIGHT),
            ),
            ("caps lock", CAPS_LOCK, caps, None),
            ("fn", FUNCTION, fn_flag, Some(Modifiers::FN)),
        ];
        for (name, keycode, flags, changed) in cases {
            for opt in [alt(RIGHT_OPT_DEVICE), alt(0)] {
                let cb = Callback::new(Modifiers::empty());
                assert_eq!(
                    cb.flags_changed(opt, RIGHT_OPTION),
                    (true, Modifiers::OPT_RIGHT),
                    "{name}"
                );
                // Right Option's release never arrives.
                let events = cb.deliver(CGEventType::FlagsChanged, CGEventFlags(flags), keycode);
                assert_eq!(events.len(), 2, "{name}: {events:?}");
                assert!(events[0].is_resync(), "{name}: {events:?}");
                assert!(
                    !events[0].modifiers.contains(Modifiers::OPT_RIGHT),
                    "{name}: {events:?}"
                );
                assert!(!events[1].is_resync(), "{name}: {events:?}");
                assert!(events[1].is_key_down, "{name}: {events:?}");
                assert_eq!(events[1].changed_modifier, changed, "{name}: {events:?}");
                assert!(
                    !events[1].modifiers.contains(Modifiers::OPT_RIGHT),
                    "{name}: {events:?}"
                );
            }
        }
    }

    #[test]
    fn a_missed_other_side_change_on_the_same_group_sends_a_resync() {
        // Both Options tracked; Right's release says Left is up too.
        let cb = Callback::new(Modifiers::OPT_LEFT | Modifiers::OPT_RIGHT);
        let events = cb.deliver(CGEventType::FlagsChanged, NONE, RIGHT_OPTION);
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(events[0].is_resync());
        // The resync has only the missed change; Right's release follows.
        assert_eq!(events[0].modifiers, Modifiers::OPT_RIGHT);
        assert!(!events[1].is_key_down);
        assert_eq!(events[1].modifiers, Modifiers::empty());
    }

    /// Normal typing while Right Option is held, and while nothing is:
    /// every event explains its own change, so no resync and the hold
    /// stays intact until its own release.
    #[test]
    fn other_modifiers_and_keys_never_resync() {
        let shift = CGEventFlags::MaskShift.0;
        let cmd = CGEventFlags::MaskCommand.0;
        let caps = CGEventFlags::MaskAlphaShift.0;
        let fn_flag = CGEventFlags::MaskSecondaryFn.0;
        for sided in [true, false] {
            let side = |bits: u64| if sided { bits } else { 0 };
            for held in [true, false] {
                let opt = if held {
                    alt(side(RIGHT_OPT_DEVICE)).0
                } else {
                    0
                };
                let cb = Callback::new(Modifiers::empty());
                if held {
                    cb.flags_changed(CGEventFlags(opt), RIGHT_OPTION);
                }
                let sequence = [
                    (CGEventType::FlagsChanged, shift | side(0x02), SHIFT),
                    (CGEventType::KeyDown, shift | side(0x02), KEY_A),
                    (CGEventType::KeyUp, shift | side(0x02), KEY_A),
                    (CGEventType::FlagsChanged, 0, SHIFT),
                    (CGEventType::FlagsChanged, shift | side(0x04), RIGHT_SHIFT),
                    (CGEventType::FlagsChanged, 0, RIGHT_SHIFT),
                    (CGEventType::FlagsChanged, cmd | side(0x08), COMMAND),
                    (CGEventType::KeyDown, cmd | side(0x08), KEY_A),
                    (CGEventType::FlagsChanged, 0, COMMAND),
                    (CGEventType::FlagsChanged, cmd | side(0x10), RIGHT_COMMAND),
                    (CGEventType::FlagsChanged, 0, RIGHT_COMMAND),
                    (CGEventType::FlagsChanged, caps, CAPS_LOCK),
                    (CGEventType::KeyDown, caps, KEY_A),
                    (CGEventType::FlagsChanged, 0, CAPS_LOCK),
                    (CGEventType::FlagsChanged, fn_flag, FUNCTION),
                    (CGEventType::KeyDown, fn_flag, KEY_A),
                    (CGEventType::FlagsChanged, 0, FUNCTION),
                    (CGEventType::LeftMouseDown, 0, 0),
                    (CGEventType::LeftMouseUp, 0, 0),
                ];
                for (kind, flags, keycode) in sequence {
                    let events = cb.deliver(kind, CGEventFlags(flags | opt), keycode);
                    for event in &events {
                        assert!(
                            !event.is_resync(),
                            "sided={sided} held={held} {kind:?} {keycode}: {events:?}"
                        );
                        assert_eq!(
                            event.modifiers.contains(Modifiers::OPT_RIGHT),
                            held,
                            "sided={sided} held={held} {kind:?} {keycode}: {events:?}"
                        );
                    }
                }
                if held {
                    assert_eq!(
                        cb.flags_changed(NONE, RIGHT_OPTION),
                        (false, Modifiers::empty())
                    );
                }
                assert_eq!(
                    cb.ctx.state.lock().unwrap().current_modifiers,
                    Modifiers::empty()
                );
            }
        }
    }

    #[test]
    fn resync_is_distinct_from_other_events() {
        let resync = KeyEvent::resync(Modifiers::OPT_LEFT);
        assert!(resync.is_resync());
        assert!(!resync.is_key_down);
        let cb = Callback::new(Modifiers::empty());
        let (_, _) = cb.flags_changed(alt(RIGHT_OPT_DEVICE), RIGHT_OPTION);
        let events = cb.deliver(CGEventType::KeyDown, alt(RIGHT_OPT_DEVICE), KEY_A);
        assert!(events.iter().all(|e| !e.is_resync()));
    }
}
