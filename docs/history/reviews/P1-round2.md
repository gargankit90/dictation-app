Reviewer: CHANGES REQUESTED

# P1 review, round 2

PR: #241 at `09c8e0cbaba2fb86b246e621c4e13f02ef36d6a1`.
Counts: 0 blockers, 4 majors, 0 minors. No merge performed.

## Prior findings

The two specific promise defects from round 1 are removed: text is eagerly
written, and another app's read can neither consume a receipt nor directly
confirm delivery. The new observer-before/after-chord tests cover those
scenarios. However, the replacement confirmation and pending restore have the
following defects; the overall guarantees from the earlier findings are not
yet satisfied. The earlier threads remain open pending these changes.

## Findings

### 1. Major: the unconfirmed fallback silently loses the transcript

Location: `crates/platform/src/insert/mod.rs:590` (scheduled at line 594).

If the target exposes no caret and ignores Cmd-V (Finder, a noneditable view,
or a disabled field), insert returns success, leaves the text temporarily,
then restores the original clipboard and drops the saved transaction. The
transcript has neither landed nor remained available to the user. This violates
P1 criterion 4, which requires the clipboard or E1 Copy recovery when a paste
cannot be confirmed. A busy target handling Cmd-V after the restore also pastes
the old clipboard; making the delay longer does not confirm consumption.
The PR acknowledges the latter residual race but still claims criterion 4.

Suggested fix: retain an unconfirmed transcript in a user-accessible recovery
path (for example E1 with Copy) before restoring the original, and do not
replace the clipboard under a still-unconfirmed queued paste solely because
a delay elapsed. Test a target that never pastes and a target that reads after
the delayed restore, checking both the actual pasted value and recovery.

### 2. Major: the late restore can lose the original during the next dictation

Location: `crates/platform/src/insert/mod.rs:585`, together with lines 522–537.

The late job takes `Pending` out of the shared slot and releases its mutex
before attempting restoration. Interleaving: the timer takes the first paste's
saved original, then pauses while the native adapter checks/builds the restore;
the next dictation sees no pending debt and snapshots the still-present first
transcript. `MacPasteboard::snapshot_items` deliberately filters out our session
items, so it saves an empty clipboard. The next dictation writes its own session;
the old restore now fails its session check and drops the only original snapshot.
When the second paste settles, it restores the empty snapshot. The original
image/text is lost. The pasteboard write lock protects final writes, not this
pending-state/snapshot interleaving. The current carry-over test runs the old
job only after the next paste has finished, so it misses this race.

Suggested fix: make claiming/settling the old debt and snapshotting/carrying it
into the next write one coordinated transaction. Keep the original discoverable
until its restore has completed or the next insertion has taken ownership.
Add a barrier-controlled regression that starts the next insert after the old
job takes its slot and before its conditional restore commits.

### 3. Major: an unrelated caret move can falsely confirm a whole transcript

Location: `crates/platform/src/insert/mod.rs:265`.

The location check compares the absolute offset to the transcript's length;
it does not check the insertion delta or the inserted text. For example, before
is field 7, offset 20, no selection; after is the same field, offset 21,
preceded by `.`. This is accepted as proof of `a long dictation.` even though
only one character was inserted. An earlier queued hotkey which moves to a
line ending in `.` (such as a configured end-of-line shortcut), or an independent
edit by the target, can satisfy it before the queued Cmd-V is handled. The app
then restores immediately, causing the delayed Cmd-V to paste the old clipboard.
The source comment asserting that a hotkey caret move cannot end after the same
last character is not generally true. Also, the native reader reads the selection
twice, so the character and final offsets need not represent the same instant.

Suggested fix: at minimum validate the expected UTF-16 selection-replacement
delta from the captured pre-paste range, and do not regard unrelated caret/text
changes as delivery confirmation. Read a consistent selection/text sample and
treat ambiguous observations as unconfirmed. Add a negative test for the offsets
above and for a pending shortcut moving across existing matching punctuation.

### 4. Major: slow AX calls extend the busy state far beyond the 2-second limit

Location: `crates/platform/src/insert/mod.rs:553` (also lines 555 and 562).

`waited` sums only `LANDING_POLL` sleeps. Each synchronous `caret()` call can
perform several AX requests; their time is not counted. If the baseline succeeds
and the target then stops responding, 100 polls each taking a single 100 ms AX
timeout plus a 20 ms sleep keep Inserting active for about 12 seconds, not 2.
Multiple slow successful requests make it longer. Failed messaging-timeout setup
can leave the OS default in force as well. The outer timeout used for smart space
in the pipeline does not wrap these new direct caret calls. The async event loop
is free, but the coordinator ignores new dictation presses while Inserting, so
the user cannot start the next dictation throughout that interval.

Suggested fix: use a monotonic deadline covering probe time as well as sleeps,
bound each complete caret read to the remaining budget, and use the same clock
for scheduling any delayed restore. Inject time/probe latency into a regression
so a slow or vanished target cannot extend the insertion beyond its budget.

## Validation and remaining risk checks

- `cargo nextest run -p dictation-platform insert::`: **56 passed**, 244 skipped
  by the filter. Includes the new observer, field-switch, carry-over, newer-copy,
  failed-chord and Secure Input tests.
- A separate harness under `/tmp/p1-round2-probe` exercises the actual public
  `PasteInserter` and `Caret` interfaces without editing the product or tests.
  Its clipboard delegates to `MemClipboard` and models the native snapshot's
  exclusion of stale session items. Manual jobs and barriers, with no timing
  sleeps, demonstrate findings 1–3: fallback replaces the unpasted transcript
  with the original; overlapping old restore and next snapshot ultimately clear
  the clipboard; a one-character edit is accepted as a whole paste. These are
  deterministic logic probes, not claims of VM reproduction.
- Shell syntax checks pass for both changed scenarios. Hosted Linux gate,
  macOS platform and DMG checks are green on the reviewed product head. VM results
  in the Developer's report were inspected; no VM GUI run was duplicated here.
- Insertion still runs on the blocking pool, and the background restore sleeps
  on its own thread. No new main-thread or audio-callback blocking was found.
  The busy-state latency defect is finding 4.
- Password subrole checks fail closed before text reads; Secure Input is checked
  before writes and immediately before the chord. The AX reader still requests
  at most two UTF-16 units and logs no characters. There are no new network,
  telemetry, key, capability or audio-persistence changes.
- Switching to a different field prevents confirmation when the hashes differ;
  unsupported browser/Electron/terminal AX text takes the unconfirmed path.
  Session token and change count checks continue to preserve newer copies and
  reject an old timer's write over a new dictation session. That write protection
  does not preserve the old snapshot in finding 2.
- The reworked user-copy e2e has a bounded clipboard watcher, requires Finder to
  be frontmost, verifies the unconfirmed path, and checks the user's copy and
  unchanged change count beyond the restore window. It meaningfully tests newer
  copy protection, but not transcript recovery or a concurrent next insertion.
- No new dependencies or GPL-derived code found. Promise code is removed;
  the revised Handy ideas credit is consistent with the replacement code.
- Scope, architecture, Rust practice, licences and Git/PR checklist otherwise
  unchanged from round 1. No guard/product/test edits by the Reviewer, no new
  minors or tech-debt issues. Only this report is committed.

## Proposed engineering wording: not accurate yet

The proposed Clipboard bullet in the PR body overstates three guarantees:

1. A moved caret plus one matching character is a heuristic, not proof of the
   whole paste (finding 3).
2. The two-second budget does not include AX calls (finding 4). Also, the late
   restore is scheduled **three seconds after `wait_for_landing` returns**, not
   three seconds after the chord. With a readable field that never matches,
   it is at least five seconds after the chord, plus AX time. No-caret reads
   return immediately and therefore are approximately the advertised three.
3. "Never paste the user's old clipboard" and "never drop the saved clipboard"
   are not true under findings 1–2.

The Lead should retain the marker/session/change-count requirements, require
recoverable transcript and original snapshots for ambiguous delivery, and apply
the final timeout/caret description only after the implementation is fixed.
Do not adopt the proposed absolute guarantees or its chord-relative timing as
a description of this head.
