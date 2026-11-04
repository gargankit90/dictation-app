Reviewer: CHANGES REQUESTED

# P1 review, round 4

PR: #241 at `a6933c2cef1bf29b8e67360a2f5949dece6eff73`.
Counts: 0 blockers, 1 major, 0 minors. No merge performed.

## R3-1 re-check

The completed-paste case is fixed: `TextInserter::finish()` is wired through
`SystemInserter`, called during pipeline cleanup, and awaited by the shell's
normal shutdown. The pending item records its due time; finishing waits only
the remaining delay, restores under the existing token/change-count rule,
and consumes the slot so the later timer cannot overwrite a newer copy.
Closing also rejects subsequent inserts. The supplied VM quit evidence and
the new tests support this path.

However, waiting for an insertion still in flight introduces the following
deadlock, so the shutdown guarantee cannot yet be approved. Existing review
threads remain open; no `Fixed`/resolution is recorded in this round.

## Finding

### 1. Major: Quit/Relaunch can deadlock waiting for insertion work queued to the blocked main thread

Location: `crates/platform/src/insert/mod.rs:422`; integration at
`crates/app/src/pipeline.rs:738`.

Concrete interleaving:

1. An insertion has acquired `busy` (line 361), taken its clipboard snapshot
   and is preparing the paste chord on Tokio's blocking pool.
2. The main event loop begins normal Quit or Relaunch. `RunEvent::Exit` calls
   `shut_down`, which synchronously blocks the main thread on
   `pipeline.shutdown()` (`src-tauri/src/lib.rs:929`).
3. The insertion requests the native keyboard-layout lookup.
   `MacPasteKeys::paste_unless` calls `layout::command_char_lookup`
   (`crates/platform/src/insert/macos.rs:492`); `command_chars` dispatches
   synchronously to the main queue (line 559, `dispatch_sync_f` at line 657).
   This needs the same main thread that is now blocked in shutdown.
4. Pipeline cleanup awaits `inserter.finish()`. Finish waits for `busy`, held
   by the insertion waiting for the main queue. No participant can progress.

The result is an indefinitely hung Quit/Relaunch, not a bounded 1.5-second
wait. `abort_all()` does not interrupt an already-running blocking insertion.
The existing slow-key test just sleeps and eventually returns; it does not
model the actual sender's main-thread dependency. Tauri's locally installed
`async_runtime::block_on` delegates to Tokio's `block_on`; it does not pump
the native dispatch queue while waiting.

Suggested fix: coordinate shutdown before blocking or exiting the main event
loop, so an insertion can finish required main-queue work while shutdown awaits
it. Alternatively remove the insertion's synchronous main-thread dependency
without violating the TIS APIs' thread requirements. Merely moving `finish()`
to a blocking task (as here) or adding a timeout around its await does not close
the cycle or guarantee clipboard restoration. Add a deterministic regression
that pauses a real/main-queue-dependent sender during preparation, starts
shutdown, and verifies both termination and conditional clipboard restoration.
Use a channel/barrier for the in-flight boundary instead of timing sleeps.

## Validation

- `cargo nextest run -p dictation-platform --features testing insert::`:
  **54 passed**, 287 skipped by the filter, including all five platform quit
  tests and existing ownership/Secure Input tests.
- `cargo nextest run -p dictation-pipeline shutdown_has_the_inserter_finish_its_clipboard_work`:
  **1 passed**, 248 skipped. It verifies that normal pipeline cleanup invokes
  the new port method.
- A standalone probe under `/tmp/p1-round4-quit` uses the actual public
  `PasteInserter`, `finish`, `ThreadLater`, `MemClipboard` and native
  `dispatch_sync_f` to the main queue. A channel establishes that insertion
  owns `busy` and has entered key preparation before the main thread waits
  on `finish`. That child hangs beyond four seconds; the control sender with
  no main-queue dependency completes and restores normally. The Python parent
  terminates only the exact hung probe PID it started, then reaps it. No other
  process is stopped, and no GUI, real clipboard or focused text is accessed.
  This reproduces the wait-cycle mechanism; the full app quit race was checked
  in source rather than reproduced through VM GUI timing.
- Checked the installed Tauri source: the Exit callback runs before its exit
  cleanup, and `async_runtime::block_on` uses Tokio rather than a native
  run-loop pump.
- `bash -n` passes for the new scenario and runner. Hosted Quality gate
  (Linux), Platform crate (macOS) and DMG (macOS) are green on this head.
  The Developer's 3/3 VM scenario result is supplied evidence, not an additional
  reviewer VM run.

## Requested checks

- **Wait bound:** for an already-posted chord and responsive clipboard, the
  remaining timer sleep is at most `RESTORE_DELAY`; the code does not reset
  the deadline at quit. An insertion still in preparation has no such bound,
  and finding 1 makes it infinite in a concrete native path. Ordinary snapshot,
  pasteboard and audio shutdown work also remains outside the timer sleep;
  the PR should not describe total shutdown latency as universally 1.5 seconds.
- **Pipeline stuck before cleanup:** `finish()` runs only after the run loop
  ends and `audio.shut_down().await` completes. It is not a watchdog or a
  general recovery mechanism for a stuck/panicked pipeline. No new separate
  defect was established beyond the main-thread cycle above.
- **Relaunch:** tray relaunch and onboarding relaunch use `request_restart`,
  and updater restart uses the same exit mechanism. They share RunEvent::Exit
  and the pipeline cleanup; no ordinary relaunch route skipping the call was
  found. They share the deadlock risk when insertion is in flight.
- **E2e meaning:** the scenario arms Quit before dictation, waits for Inserting,
  asserts the app exits, checks the actual target text and reads `pbpaste`
  directly after exit against a distinct sentinel. It also requires the new
  finish-only log message, so a timer restoring normally after a late Quit does
  not silently satisfy the test. It is registered in ALL_SCENARIOS. The timing
  gap is logged rather than asserted; this scenario covers the ordinary quit
  path but cannot reliably select the main-queue interleaving in finding 1.
- **Ownership and errors:** the finish path reuses the existing conditional
  restore. It consumes the pending item once; newer copies and a timer running
  after finish are covered. Failed chords leave no debt, so finish does not
  unexpectedly clear their fallback transcript. No new AX target reads,
  transcript logging, permissions, network destinations or dependencies.

## Engineering wording and remaining checklist

The proposed rule correctly records the Lead's accepted fixed-delay and
no-text-field trade-offs, Secure Input exception, synchronous next-insertion
restore, due-time shutdown restore and U5/U7 recovery. Those design choices are
not reopened here. Its shutdown timing must distinguish the **remaining restore
delay after a posted chord** from total shutdown time, and it must not promise
the in-flight case until finding 1 is fixed. At present that case can hang.

This re-review is limited to round 4. Scope, inward dependencies, unsafe-code
placement, licence treatment and unchanged privacy rules remain consistent
with the earlier review. Tests cover normal completion but miss the native
main-queue dependency. No new minors or tech-debt issues; only this report is
committed by the Reviewer. No merge or review-thread resolution is performed.
