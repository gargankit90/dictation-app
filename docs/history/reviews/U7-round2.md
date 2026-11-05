Reviewer: APPROVED

# U7 review, round 2

PR: #267 at 054bda2405d100b48e887857d64b896151af10a3
Verdict: APPROVED

## Findings

No new blocker, major or minor findings in this round. Both round 1 majors are fixed.

1. **Unreadable-file preservation:** failed reads, malformed JSON and unsupported
   versions leave `loaded` false and the original file untouched. Recording and
   limit changes retry the read; successful recovery merges pending entries with
   stored entries, assigns noncolliding ids and enforces the limit. Delete affects
   only memory while unreadable; explicit Clear and Off still delete the file.
   Settings shows a safe notice and allows Clear even with no visible rows.
2. **Reads independent of disk:** mutations serialize under `persisting`, while
   the separate state lock is released before every store operation. Tray status
   and Settings reads take only that state lock; Copy last performs its lookup
   inside its worker. No inverted lock acquisition was found.

## Regression strength

- `a_file_that_cannot_be_read_is_never_saved_over` checks the original bytes
  after multiple dictations, Delete and a limit change, for malformed and
  newer-version files. Old code replaces those bytes on the first dictation.
- `a_failed_read_is_retried_and_nothing_is_lost` checks both preservation during
  failure and recovery of old plus pending entries. Old code marks the failed
  read loaded permanently and loses the old entries. Collision and limit-change
  tests additionally cover the merge path.
- Clear/Off tests verify intentional deletion still works; failed Clear retains
  the unreadable notice.
- `reading_the_history_never_waits_for_a_write` uses channels to pause the store
  write and verifies all read APIs finish before releasing it. With the old
  shared mutex the reader times out; the test releases and joins its threads
  before asserting, so the regression fails rather than hanging.
- UI tests cover the notice, pending entries and Clear/Off confirmations.
  The expanded VM scenario checks popup trimming/cancel/raising/Off and damaged
  file preservation plus explicit deletion. VM execution remains the Tester's
  evidence; this reviewer inspected the scenario and checked shell syntax.

## E2F / PR #265 integration note for the Lead

U7's `Recording::finish()` delegates directly to `self.inner.finish()` exactly
as #265's `Remembering::finish()` does. The production chain uses Recording;
boxed inserters also forward finish. `the_inserter_passes_finish_through` asserts
one call reaches the shared fake. Therefore #265 can drop its change and test in
`crates/app/src/last_dictation.rs` when merging after U7, retaining U7's Recording
and its test rather than resurrecting the deleted Remembering wrapper. This
confirms equivalent forwarding, not independent VM proof of quit-time restore.

## Checklist (round 2 scope only)

1. Scope: OK; only round 2 changes and the two earlier majors rechecked.
2. Correctness: OK; preservation, retry/merge, deletion and lock ordering checked.
3. Security and privacy: OK; no new sensitive logs or telemetry; notice exposes
   no parser messages. Explicit deletion remains available.
4. Architecture: OK; storage stays behind its port; no new dependency or unsafe.
5. Rust and TypeScript practice: OK; disk calls outside state lock, typed IPC
   regenerated consistently, safe UI notice.
6. Licences and reuse: OK; no new dependencies or external adapted code.
7. Tests: OK; regression reasoning above; validation below.
8. Git and PR: OK; focused Conventional Commits and updated verification limits.

## Validation

- History/App/Settings UI tests: 88 passed across three files.
- `bash -n tests/e2e/scenarios/e2e_history.sh`: passed.
- Rust history tests and mandatory push gate: results posted with the PR comment.
- No VM scenarios rerun by this reviewer; manual criterion 8 remains the Lead's
  responsibility. Approval does not authorize merge without the required Tester
  and Lead evidence.
