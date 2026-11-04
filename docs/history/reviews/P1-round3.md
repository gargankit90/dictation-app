Reviewer: CHANGES REQUESTED

# P1 review, round 3

PR: #241 at `858ec45c6aef09b473e0b376dfee100183c6b8a5`.
Counts: 0 blockers, 1 major, 0 minors. No merge performed.

## Previous findings and the Lead's decision

Reviewed against the explicitly accepted round 3 fixed-delay design. The
late-target trade-off (a target reading after 1500 ms can paste the original)
is acknowledged by the Lead; this review does not reopen that design decision.

- Round 1's two promise mechanisms are gone. Clipboard reads do not affect
  the eager write or the timer.
- Round 2 finding 1 is superseded by the Lead's definition of failed-chord
  recovery: `PermissionDenied`/`Failed` leave the transcript, without scheduling
  restoration, and the E1 hint tells the user to paste it manually.
- Round 2 finding 2 is fixed: both `finish_pending` and the timer hold the
  pending mutex throughout the conditional restore. A subsequent insertion
  cannot snapshot a stale session while the old timer has hidden its debt.
  An old timer also checks its transaction ID before taking a newer pending item.
- Round 2 findings 3 and 4 are removed with the caret confirmation and AX
  polling. The paste path no longer reads the target or waits on an AX budget.

The previous defects therefore do not block this round. The new shutdown
problem below does; the earlier threads are not marked resolved while the
overall review remains CHANGES REQUESTED.

## Finding

### 1. Major: normal quit before the timer fires permanently loses the original clipboard

Location: `crates/platform/src/insert/mod.rs:215` (detached timer at lines
215–224; pending restore at lines 417–432). Shutdown context:
`src-tauri/src/lib.rs:929` and `crates/app/src/pipeline.rs:286`.

Concrete scenario: a dictation pastes successfully and returns to Idle; the
user immediately chooses Quit from the tray, within the 1.5-second window.
The only saved original (possibly an image or rich clipboard item) is held in
memory by `Pending`. `ThreadLater` drops its `JoinHandle`, and neither pipeline
shutdown nor shell shutdown waits for this timer or completes the pending
restore. Tauri exits the process, killing the sleeping thread and its snapshot.
The eager transcript remains on the system clipboard; the original never comes
back and cannot be recovered by relaunching. The optional analytics flush is
not a minimum wait: with analytics disabled or no work it returns immediately.
The shell itself explicitly notes that Tauri exits without running destructors
(`src-tauri/src/lib.rs:936`), so a Drop-only fix would not be sufficient.

Suggested fix: give pending clipboard work an explicit shutdown lifecycle.
Stop accepting new inserts, await the remaining restore deadline and complete
the token/change-count-guarded restore before normal quit/relaunch exits.
Do not restore early under a queued paste merely because shutdown began.
Add a lifecycle regression for immediate normal quit after successful insertion,
plus a newer-copy variant; verify the original is recovered and a newer copy
is preserved. This does not require recovery after a crash or forced kill.

## Checks and evidence

- `cargo nextest run -p dictation-platform --features testing insert::`:
  **49 passed**, 287 skipped by the filter. Includes fake-clock cases (a)–(f),
  Secure Input, marker checks and native named-pasteboard item restores.
- `cargo nextest run -p dictation-platform --features testing --test paste_flow`:
  **11 passed**, including the updated failed-chord expectation.
- A standalone subprocess probe under `/tmp/p1-round3-quit` calls the actual
  public `PasteInserter`, `ThreadLater` and `RESTORE_DELAY`, with a synthetic
  file-backed clipboard and no GUI or general pasteboard access. After inserting
  and immediately exiting, a parent checks two seconds later: the synthetic
  transcript remains instead of the original. The control keeps the child alive
  for two seconds and the original is restored. This confirms the detached-work
  lifetime; the real app shutdown path was checked in source, not GUI-reproduced.
- `bash -n` passes for `tests/e2e/lib_full.sh` and the image scenario.
- Hosted Quality gate (Linux), Platform crate (macOS) and DMG (macOS) checks
  are green on the reviewed head. The supplied 20/20 VM loop and four reruns
  were inspected as Developer evidence; no VM GUI suite was duplicated here.

## Requested risk and test-edit assessment

- **Quit/sleep:** normal quit is finding 1. Sleep can delay the detached timer;
  its conditional checks still run when it resumes, so a copy made in the
  meantime prevents restoration. Exact timing during sleep is not established
  by the fake-clock tests; no separate sleep-specific corruption was found.
- **Locks/hotkeys:** the timer does not sleep while holding the pending mutex.
  The next insertion can wait for a restore already in progress, or execute it
  synchronously through `finish_pending`. This is on the pipeline's blocking
  pool, not the audio callback, event loop or hotkey listener. A large snapshot
  can therefore delay insertion even though it does not impose the 1.5-second
  sleep. No reversed pending/write-lock acquisition was found.
- **Ownership:** the unchanged macOS adapter checks both the unique session
  token and captured change count, then rechecks after constructing all restored
  items. A newer copy, including identical text or a copy retaining marker
  types, changes the count and is preserved. The already documented tiny
  cross-process check/write window remains; AppKit has no atomic compare/swap.
- **Images/rich content:** the native adapter is back to main and still saves
  every item and representation, preserving privacy markers on original items
  and excluding only our stale sessions. Named-pasteboard round-trip, stale-item
  and restore tests pass. The image e2e now waits before checking types and PNG
  bytes, as required by asynchronous restoration.
- **Secure Input:** guards before snapshot/write and immediately before the
  chord remain. If the final guard trips after writing, the original is restored
  immediately instead of taking the failed-chord fallback. Tests pass.
- **`crates/platform/tests/paste_flow.rs`:** the changed assertion is correct
  for design item 5: a failed chord on an initially empty clipboard now leaves
  the transcript. It still checks the error result; it does not weaken the
  successful-paste or newer-copy assertions. The Lead should approve this
  Tester-owned edit as requested.
- **`tests/e2e/lib_full.sh`:** waiting before a clipboard assertion is appropriate
  now that Idle precedes restoration. A missing/wrong restore still leaves the
  transcript or wrong value, so sentinel/image assertions fail. The helper can
  tolerate a late restore up to its polling window; it is not proof of the exact
  1500 ms deadline, which the fake-clock test separately checks. It makes an
  extra VM query per call, waits only while a session marker exists, and adds
  roughly the remaining restore window after a normal paste (up to 20 polling
  sleeps for a persistent failed-paste marker). VM-query time is additional to
  the nominal four seconds. The Lead should approve this shared-helper edit;
  it does not hide a permanently missing restore in the reviewed assertions.

## Engineering rule and checklist

The proposed PR-body rule accurately describes the accepted delay, eager
markers, ownership checks, pending-restore-before-next-snapshot behavior and
absence of target reads. Before applying it, qualify two sentences:

> The timer does not wait on the event loop or hotkey listener. A subsequent
> insertion completes any pending conditional restore synchronously before
> snapshotting, and may wait for a restore already in progress.

> A non-Secure-Input failure to send the chord leaves the transcript on the
> clipboard; a Secure Input refusal restores the original immediately.

The blanket claim that the next dictation never waits is inaccurate, as the
code's own lock comment at line 424 explains. The no-chord error hint also
presupposes a successful clipboard write; the PR already records the rarer
pre-write failure limitation. Add explicit shutdown semantics after fixing
finding 1. This review does not edit engineering policy.

Scope and correctness: one new lifecycle major above, with the earlier findings
removed or superseded by the Lead's decision. Security/privacy, inward
dependencies, unsafe placement/comments, Rust practices and licences: no new
problem found in the changed code. No new dependencies, target text reads,
network destinations, logging payloads or guard edits. Tests and PR notes cover
the accepted timed flow, but lack shutdown coverage. No new minors or tech-debt
issues; only this review report is committed by the Reviewer.
