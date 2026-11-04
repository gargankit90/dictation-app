Reviewer: CHANGES REQUESTED

# P1 review, round 1

PR: #241, reviewed product head `33495a1`.
Counts: 0 blockers, 2 majors, 0 minors. No merge performed.

## Findings

### 1. Major: another reader can still make the target paste the old clipboard

Location: `crates/platform/src/insert/mod.rs:356` (also lines 360–375 and 317–319).

A data request after the chord is treated as confirmation from the target.
Concrete scenario: a clipboard observer that does not honor the transient markers
reads at 20 ms after Cmd-V, while TextEdit is busy and processes Cmd-V at 1070 ms
(the target delay measured in the PR). At approximately 170 ms the quiet period
ends and the saved clipboard is restored. TextEdit then pastes the user's old
clipboard, exactly the harm this PR addresses. The callback has no reader identity;
the markers are conventions, not access controls. A target which probes text and
later reads again has the same problem: after the first request AppKit caches the
text, so the later read does not restart the quiet timer.

Suggested fix: do not authorize restoration solely from an arbitrary data-provider
callback plus 150 ms of silence. Require confirmation tied to the actual target,
or keep the transcript available through a safe unconfirmed-paste fallback. Add a
regression with an observer reading after the chord and a target reading at
1070 ms, and assert the target receives the transcript. Increasing the quiet delay
alone would only move the race.

### 2. Major: a pre-chord reader consumes the only receipt and loses the original clipboard

Location: `crates/platform/src/insert/mod.rs:302` (also lines 307–315),
with the native behavior at `crates/platform/src/insert/macos.rs:113`.

`provideDataForType:` is lazy materialization, not a notification of every read.
After `setString_forType` supplies the text, subsequent readers get cached data
without invoking the owner again. Concrete scenario: a background reader fetches
the text between publication and the baseline at line 302. `before` is now 1.
The real target pastes successfully, but its read cannot increment the counter.
The loop times out and returns without restoring; the local `saved` snapshot is
dropped. The user's original image, rich text, or text is then unrecoverable from
the app. This is a successful paste incorrectly classified as unconfirmed, not
the deliberate fallback for a target which never reads. The next dictation also
filters out the stale session item, so it cannot recover that original snapshot.

Suggested fix: model a promise as a one-shot materialization and preserve the
original snapshot in a recoverable pending transaction when delivery is ambiguous.
Do not require another provider invocation after one has already supplied the
text; coordinate this with finding 1 so counting the earlier callback does not
instead trigger unsafe restoration. Add native or faithful one-shot-fake coverage
for a pre-chord observer followed by a successful target paste and recovery of
the original clipboard. The current `ReadsClipboard` fake increments on every
simulated read, which AppKit does not do.

## Evidence and validation

- `cargo nextest run -p dictation-platform insert::`: **47 passed**, 244 skipped
  by the filter, including Secure Input, conditional restores and the six new
  read-wait tests.
- A standalone Swift/AppKit probe in `/tmp/p1-promise-probe.swift` used a unique
  **named** pasteboard, never the general clipboard, with the same
  `declareTypes:owner:` / `setString:forType:` mechanism. Two successive
  `string(forType: .string)` calls both returned the supplied synthetic text;
  the owner callback count was **1 after each call**, and `changeCount` stayed
  unchanged. The named pasteboard was released afterwards. This directly checks
  the caching assumption behind both findings; it is not a full VM reproduction
  of the user-visible races.
- Apple's [provider callback documentation](https://developer.apple.com/documentation/appkit/nspasteboardtypeowner/pasteboard(_:providedatafortype:))
  describes a request to supply a data type; it provides no target identity or
  per-consumer receipt guarantee.
- `bash -n` passed for the new scenario and `tests/e2e/run.sh`.
- Hosted checks on the reviewed head: Quality gate (Linux), Platform crate
  (macOS), and DMG (macOS) all passed. VM execution belongs to the parallel Tester;
  the Developer's 10/10 per key and full-suite results were read as supplied
  evidence, not independently rerun here.

## Checklist

1. **Scope:** the diff addresses P1 and adds its regression scenario. The spec
   commit precedes the implementation; PR notes map all six criteria to evidence.
   Findings 1–2 limit the claimed clipboard guarantees.
2. **Correctness:** findings 1–2. No additional state-machine or locking defect
   found. Insertion runs in `spawn_blocking` (`crates/app/src/pipeline.rs:1023`),
   leaving the event loop and audio callback free. The polling budget is 2 s;
   presses while Inserting are ignored by the existing coordinator, so another
   dictation cannot concurrently replace this promise. A new press after Idle
   works. The wait adds up to 2 s of busy state, not an indefinite hotkey block.
3. **Security/privacy:** existing Secure Input checks remain before snapshot,
   before write, and immediately before the chord; existing tests pass. Markers
   and session tokens are written eagerly. Restore rechecks both change count
   and token after constructing all saved representations, preserving a user
   copy made during the wait. No new network destination, capability, key,
   transcript logging, telemetry payload, or audio persistence. Finding 1 still
   risks pasting clipboard contents into the wrong context.
4. **Architecture:** OK. New trait method defaults to unsupported; platform
   implementation and unsafe code remain in platform, with safety comments.
5. **Rust/TypeScript practice:** no new unchecked panics or suppressions.
   Promise publication runs on the main queue; its callback takes no mutex and
   the thread-local retains the owner for late reads until the next publication.
   Polling and sleep run on the blocking pool. Text is promised only as plain
   text; original RTF/images/files are still snapshotted and conditionally
   restored. No separate password-manager or Universal Clipboard compatibility
   claim is established: arbitrary readers are precisely finding 1's risk.
6. **Licences:** OK. Compared the adaptation with Handy's
   `src-tauri/src/paste_tx/macos.rs` and its MIT LICENSE in the adjacent reference
   checkout. NOTICE and the generated shipped notice credit CJ Pais and the
   source commit; the macOS module identifies the source. No new dependencies
   or evidence of GPL adaptation.
7. **Tests:** native fidelity gap in findings 1–2. The new e2e scenario checks
   actual TextEdit contents against the mock text and restoration against a
   distinct sentinel, so an old-clipboard paste fails. Default execution is
   six attempts (three per key), with bounded readiness, Idle and Terminal-job
   waits and cleanup for held modifiers. It is registered in ALL_SCENARIOS.
   The scenario covers normal first paste, not competing readers.
8. **Git/PR:** Conventional Commits, regression before fix, detailed PR notes,
   no guard edits. No new minors, hence no tech-debt issues or minor threads
   to resolve. Only this review file is changed by the Reviewer.

## Exact clipboard rule for the Lead

Replace the Clipboard bullet in `docs/ENGINEERING.md` with:

> **Clipboard:** mark every dictation write with
> `org.nspasteboard.TransientType`, `ConcealedType` and `AutoGeneratedType`.
> Save all original items and representations. Restore them only after the
> paste target has consumed the transcript, and only while both our session
> token and the write's change count still match. Never overwrite a newer copy.
> On macOS, wait at most 2 seconds for delivery confirmation; if delivery is
> unconfirmed, keep the transcript available for a late or manual paste and
> preserve a recoverable original snapshot. A pasteboard data-provider request
> by an unidentified reader is not proof that the paste target consumed it.
> Adapters without read confirmation currently use the 300 ms settle fallback.

The current implementation uses a post-baseline provider callback followed by
150 ms of silence as confirmation; findings 1–2 explain why that is weaker than
the rule above. This review does not edit the engineering policy or product code.
