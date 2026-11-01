# handy-keys 0.3.4, patched for dictation-app

Upstream: https://github.com/handy-computer/handy-keys, version 0.3.4 as
published on crates.io (MIT, `LICENSE`). Vendored in M1.6 because no release
fixes the macOS defects found in the M1.4 retrospective review (#80). The
commit that added this directory holds the crate unchanged (minus
`Cargo.lock`), so `git diff` against it shows the whole patch. Wired in with
`[patch.crates-io]` in the root `Cargo.toml`. Drop the vendored copy once an
upstream release has equivalent fixes.

Every change is marked `Patched (dictation-app)` in the code.

## Changes

All in `src/platform/macos/listener.rs` unless noted.

1. **Modifier direction from the event's flags** (#80 B1). On `FlagsChanged`
   the listener used to toggle a remembered bit for the changed key. A key
   already held when the listener started then came out as a press when let
   go, and every later press and release of it was inverted. Now
   `after_flags_changed` decides from the event's flags, which already
   include the change: a modifier group absent from the flags is up; with
   the device-dependent side bits (IOKit's `NX_DEVICE*KEYMASK`, set on
   hardware events) each side is exactly what they say. Only when the group
   is held and the flags carry no side bits (some synthetic events) does it
   still toggle the changed key, as before. Fn follows its own flag.
2. **Side-aware reconciliation.** `reconcile_modifiers` is replaced by
   `modifiers_from_flags`, which also reads the side bits instead of always
   guessing the left key.
3. **Snapshot at start** (#80 B1). `run_event_tap` sets the tracked
   modifiers from `CGEventSourceFlagsState` before enabling the tap, instead
   of starting empty.
4. **Resync notice** (#80 B2). When the tap is disabled
   (`TapDisabledByTimeout` / `TapDisabledByUserInput`) the callback still
   re-enables it and reconciles, and now also sends `KeyEvent::resync` with
   the corrected modifiers. It sends one too, ahead of the event, when a
   key or mouse event's flags correct the tracked modifiers (a change was
   missed). Presses and releases may have been lost in both cases, and with
   a held hotkey no further event may ever arrive to show it.
   `FlagsChanged` events are reconciled too (#85 review round 1, B1): their
   flags describe every modifier, so a change to any tracked modifier other
   than the event's own key (another group, the other side of the same
   group, or anything at all on a Caps Lock or Fn event) is a missed press
   or release. The callback sends a resync with just the missed changes,
   then the event itself. Before, a missed Right Option release followed by
   Shift cleared Option silently and the Shift event read as the hotkey's
   release, so the recording was transcribed instead of discarded. Normal
   typing explains its own change and sends none.
   **Synthetic flags** (#89). A key or mouse event whose own flags disagree
   with the tracked modifiers is no longer taken as a missed release on its
   own: software that posts events (remote desktop, automation, the app's
   own paste) often gives them flags with no Option bit and no side bits,
   and that used to cancel a recording while Option was still held. The
   callback now asks the session's own flags
   (`CGEventSourceFlagsState`, the same source as the snapshot at start)
   and reconciles to those; only if they show the change too is it a missed
   release and a resync.
5. `src/types/hotkey.rs`: `KeyEvent::resync(modifiers)` and
   `KeyEvent::is_resync()`. A resync has no key and no changed modifier,
   which no other macOS event has. `HotkeyManager` needs no change: it
   treats a resync as a modifier release, so a modifier-only hotkey whose
   keys are no longer held is released.
6. Tests: `dictation_patch_tests` at the end of `listener.rs` drive the real
   callback with local `CGEvent`s (no tap, nothing posted, no permission
   needed). Run them with
   `CARGO_TARGET_DIR=target/handy-keys cargo test --manifest-path third_party/handy-keys/Cargo.toml --lib dictation_patch`.
7. `rustfmt.toml` (new): keeps the repository's pre-commit
   `rustfmt --edition 2024` from restyling the 2021-edition upstream code.

Not changed: Windows and Linux backends, blocking hotkeys, the tap thread's
run loop and teardown, the unreachable `keycode == 0x3F` branch.

## Upstream report (draft)

> **macOS: modifier held when the listener starts is decoded inverted; tap
> disable recovery is invisible to consumers**
>
> handy-keys 0.3.4, macOS.
>
> 1. `event_tap_callback` decides `FlagsChanged` up/down by toggling the
>    tracked bit for the keycode (`listener.rs` ~291–318) and skips
>    reconciliation on `FlagsChanged`. `ListenerState` starts with no
>    modifiers. If Right Option is held when `KeyboardListener::new()`
>    returns, its release arrives with flags lacking `MaskAlternate` but is
>    reported as `is_key_down: true`; the next press is reported as a
>    release. A missed press (tap disabled) inverts it the same way.
>    Suggested fix: derive direction from the event's flags (group mask,
>    plus the device-dependent `NX_DEVICE*KEYMASK` bits for the side), and
>    seed the state from `CGEventSourceFlagsState` at start.
> 2. On `TapDisabledByTimeout`/`TapDisabledByUserInput` the callback
>    re-enables the tap and reconciles `current_modifiers` but sends nothing.
>    A consumer tracking a held modifier-only hotkey never learns that its
>    release may have been lost; with no further key event it stays "held"
>    indefinitely. Suggested fix: send an event (as the Windows and Linux
>    backends do with synthetic releases), e.g. a key-less resync event or
>    releases for modifiers that reconciliation cleared.
>
>    The same holds when a modifier's release is missed and the next event
>    is another modifier's `FlagsChanged`: its flags show the first one up,
>    but `FlagsChanged` is never reconciled, so the stale bit stays set (and,
>    once direction comes from the flags as in 1, it is cleared with no
>    event of its own). A consumer can't tell that from a real release.
>    Suggested fix: on `FlagsChanged`, reconcile every modifier other than
>    the event's own key and report any correction before the event.
>
> Reproduction without a tap: call the callback with local `CGEvent`s (see
> the tests in this fork). We have a patch with tests and can open a PR.
