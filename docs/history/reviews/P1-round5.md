Reviewer: APPROVED

# P1 review, round 5

PR: #241 at `fc0b73d9e65e77e2929ed89d7d8bc97d98e488b5`.
Counts: 0 blockers, 0 majors, 0 minors. No merge performed.

## R4-1 re-check

Fixed. The paste no longer calls `on_main` or `dispatch_sync_f`. Its chord
reads an atomic cached key code; the TIS/UCKeyTranslate work runs ahead of time
on the main thread. An insertion holding `busy` therefore no longer waits for
the main queue while the main thread awaits shutdown.

The pipeline also bounds its wait for `finish()` with `FINISH_BOUND = 2 s`.
The Tokio timeout runs on the existing multithreaded runtime and needs no
native-main-queue callback. If finishing remains stuck, cleanup logs the
timeout and continues, accepting the Lead's stated possibility of skipping
that restore. Dropping the blocking task's handle does not cancel that task;
the bound is on the pipeline's wait, as intended for process exit.

The earlier promise/caret mechanisms remain removed; the pending-restore
serialization and normal completed-paste shutdown restore remain intact.
The Lead's accepted delayed-target and no-text-field trade-offs are not
reopened. All eight of this Reviewer's open threads can be closed on this head.

## Requested cache and lifetime checks

- **Very first paste:** production constructs `SystemInserter::new()` in
  Tauri's main-thread setup (`src-tauri/src/lib.rs:434`), before spawning the
  pipeline or logging ready. `MacPasteKeys::new()` calls `watch()`, whose
  main-thread branch refreshes synchronously before returning. Thus the
  shipped startup path fills the cache before it accepts dictation. The
  asynchronous off-main constructor path has an explicit ANSI-V fallback
  until its queued refresh runs; it does not block a caller on the main queue.
- **Switching layouts:** the process registers the selected-input-source
  distributed notification once and refreshes on its main-thread callback.
  `DeliverImmediately` is the correct background-delivery behavior for a
  menu-bar app. Each notification reads the currently selected layout rather
  than restoring a layout captured by an earlier event. The cache uses an
  atomic release/store and acquire/load, so readers see a complete key code.
  The supplied `e2e_layout_paste` exercises the first dictation after switching
  among French, Dvorak, Russian, Dvorak Right-Handed and Turkish, including
  layouts whose paste key is not ANSI V.
- **Delivery limit:** distributed notification delivery is asynchronous;
  the cache reflects a switch once its callback runs. It is not a guarantee
  that a paste racing notification delivery observes the new layout with
  zero latency. Apple's [notification documentation](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CocoaFundamentals/CommunicatingWithObjects/CommunicateWithObjects.html)
  documents that delivery can be delayed. No concrete persistent stale-cache
  defect was found in the registered normal path, and the VM evidence covers
  ordinary switch-then-dictate use rather than zero-latency ordering.
- **Observer lifetime/thread:** the callback is a static function, the cache
  and `Once` are process-wide statics, and the notification-name NSString is
  intentionally retained for the process lifetime. There is no borrowed
  observer object to outlive or deallocate. Apple's
  [CF registration documentation](https://developer.apple.com/documentation/corefoundation/cfnotificationcenteraddobserver(_:_:_:_:_:_:))
  permits a null observer on supported macOS versions and specifies delivery
  registration for the main thread. SDK headers confirm the distributed TIS
  notification and `DeliverImmediately = 4`. The callback's TIS reads therefore
  meet their main-thread requirement without involving the paste worker.
- **Queued closure:** off-main registration owns its boxed closure until
  `dispatch_async_f` invokes the trampoline once. The main-thread branch runs
  it directly. Neither case borrows a caller's stack or synchronously waits
  for main-queue work.

## Validation

- `cargo nextest run -p dictation-platform --features testing insert::`:
  **56 passed**, 287 skipped by the filter. Includes cached-key lookup and
  watcher construction without a running main queue, normal due-time finish,
  newer-copy preservation and the existing Secure Input/native restore checks.
- `cargo nextest run -p dictation-pipeline shutdown_`:
  **10 passed**, 240 skipped. Includes the new stuck-finish test, the normal
  finish invocation and shutdown during recording, transcription and insertion.
  The stuck worker is explicitly released after the timeout assertion so the
  test does not leave a blocking worker hung.
- Source search finds no synchronous main-queue dispatch or old `on_main`
  invocation anywhere in the paste adapter.
- Hosted Quality gate (Linux), Platform crate (macOS) and DMG (macOS) checks
  are green on the reviewed head. The Developer's passing VM quit, first-paste,
  clipboard, copy-wins, transcription/relaunch and layout scenarios were
  inspected as supplied evidence; no reviewer GUI run was duplicated.
- No process was stopped during this re-review.

## Engineering wording and checklist

The proposed rule now matches the Lead's design: markers and full snapshots,
conditional delayed restore, synchronous settlement before a subsequent
snapshot, Secure Input exception, failed-chord fallback, due-time quit restore,
bounded finish wait and no main-thread dependency in the paste. Read the
two-second limit as the **finish phase's wait**, not an upper bound on all
application shutdown work: audio shutdown precedes it and the existing usage
flush follows pipeline shutdown. The documented timeout/restore-loss exception
is explicit and accepted by the Lead.

This re-review covers only round 5 and R4-1. No new correctness, privacy,
architecture, licence or dependency problem found. The new unsafe FFI remains
in platform with safety comments; no target text reads, secrets, transcript
logging or new network behavior are introduced. Tests cover the regression
and fallback. No guard changes, new minors or tech-debt issues; only this
review report is committed by the Reviewer. Approval does not perform or
authorize a merge in this task.
