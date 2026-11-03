Reviewer: APPROVED

PR: #107 at `6839b60f536c2d61d44575b8e8ad14da29924cce` (product fixes at `ddbbf23`).
Targeted round 2: fixes since round 1 and merge of main. No open blocker,
major, or minor findings. No VM used; no merge performed.

## Findings resolved

- **M1 resolved:** `crates/app/tests/overflow_outcome.rs:221` and
  `crates/app/tests/m2_3_edges.rs:312` await the fake microphone's discard
  notification, then assert not recording before shutdown or recovery.
  `notify_one` retains a permit if discard finishes before the waiter starts;
  the notification follows clearing the recording flag. Shutdown can no longer
  conceal the missing overflow discard.
- **N1 resolved:** `src-tauri/src/commands.rs:207` and
  `crates/ports/src/transcribe.rs:75` now advise checking the provider selected
  in Settings. That selector exists in `src/Settings.tsx:257`; Test key can be
  retried there. Exact-message tests cover both messages; issue #109 is closed.

## Independent validation

Archived this head into `/tmp/t1-round2-review-6839b60`, with a fresh target
at `/tmp/t1-round2-target-6839b60`, separate from earlier mutant builds.
Reapplied precisely the round-1 mutant: `Runner::keys_overflowed` executes
all cancellation commands except `DiscardRecording`, preserving queue draining,
state and UI transitions. No test modification was made.

`cargo nextest run -p dictation-pipeline -p dictation-ports`:

- Mutant: **150 passed, 2 failed, 1 skipped**. Both
  `overflow_fence_discards_all_untrusted_queued_keys` and
  `criterion_7_a_key_flood_behind_a_blocked_start_is_bounded_and_recovers`
  fail after the bounded 10-second wait with “overflow never discarded the
  recording”. This catches the user-visible regression: Idle while the
  microphone continues recording.
- Restored product code: **152 passed, 1 skipped**, in 8.350 seconds.

Reproduction script: `/tmp/t1-round2-mutant.py`; logs:
`/tmp/t1-round2-mutant.log` and `/tmp/t1-round2-control.log`.
All mutations stayed in the temporary archive.

The merge's single-instance files match its main parent exactly: no conflict
resolution changes to M1.7.1. Beyond that merge, the only product/test changes
since round 1 are the two overflow fences and redirect wording/tests; remaining
changes are test reports. GitHub checks on ddbbf23 show Linux Quality gate and
macOS Platform crate success. The current report-only head has no check runs;
Tester run 2 records a passing full gate and 20 repeated changed-test runs.

## Checklist (changed scope only)

| Section | Result |
| --- | --- |
| 1. Scope | OK: M1, N1 and main merge only; unchanged acceptance mapping retained. |
| 2. Correctness | OK: microphone completion checked before cleanup; recovery retained. |
| 3. Security and privacy | OK: no new secret, transcript, audio persistence, network or capability behavior. |
| 4. Architecture | OK: no dependency, OS conditional or unsafe changes in the fixes. |
| 5. Rust and TypeScript practice | OK: bounded notification waits; actionable messages; no new suppressions. |
| 6. Licences and reuse | OK: no new dependencies or copied code. |
| 7. Tests | OK: independent mutant now fails both repaired oracles; restored suite passes. |
| 8. Git and PR | OK: scoped fixes and reports; no guard/spec edits; #109 closed. |

Approval covers this re-review only; merging is explicitly outside this task.
