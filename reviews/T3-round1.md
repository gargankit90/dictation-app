Reviewer: APPROVED

PR: #141 at `df84261a0f8e7103c06919d85f18b1d76378743b` (round 1).
No blocker or major findings; two minor follow-ups. This is a code-review
approval, not authorization to merge before the Tester and Lead checks.

## Findings by severity

| # | Severity | File:line | Finding and suggested fix |
| --- | --- | --- | --- |
| M1 | Minor | `crates/platform/src/single_instance/unix.rs:944` (also production lines 91, 114) | Both disclosed extra mutants survive: halve the stop budget instead of doubling it, or ignore the accept-loop stop flag. Independently confirmed against all 39 singleton module tests. The new tests catch the intended wake-up and lock-release regressions, but do not prove the stop budget or accept-loop exit. Add an injectable wait/clock seam and an exit fence in a follow-up; avoid sleeps as proof. No defect observed in the correct implementation. [Issue #142](https://github.com/gargankit90/dictation-app/issues/142), [inline comment](https://github.com/gargankit90/dictation-app/pull/141#discussion_r4437750216). |
| M2 | Minor | `src-tauri/src/commands.rs:1461` (fixture at line 1362) | Another successful ACL save uses a scratch directory without an owner to remove it. Passing runs leave `dictation-acl-<pid>-main-overlay/settings.json` behind; the older ACL test already has this omission. Retain an RAII scratch owner until app/windows/tasks finish and clean the root on normal return and unwind. Give each fixture a unique root. [Issue #143](https://github.com/gargankit90/dictation-app/issues/143), [inline comment](https://github.com/gargankit90/dictation-app/pull/141#discussion_r4437750264). |

## Product behavior checked

- **Bounded key saves:** reviewed `save_key`, `LiveEngine::write_key`,
  `KeyWrites`, and the Settings submit path together. Validation precedes
  queuing. Clones share the write slot; it stays occupied through the write
  and engine rebuild. A timed-out write can finish, but a later save waits
  behind it, so the earlier write cannot overwrite a completed newer save.
  Its outcome is not mistaken for the waiting save's outcome. Repeated
  stuck retries start no additional OS writes; the command returns an
  actionable uncertainty error and Settings unlocks without claiming
  success. Panic/thread-start errors release or leave the slot usable.
  Concurrent callers are ordered by lock acquisition, not a FIFO promise;
  ordinary Settings submissions are serialized by the disabled form.
- **Login-item transaction:** `settings.validate()` runs before native
  changes, including the structurally plausible `Ctrl+Banana` typo. A file
  failure restores the prior login state and preserves the original config
  error if restoration also fails. The existing Settings error path rereads
  the OS state. Restoration is necessarily best-effort if the OS refuses;
  the code logs that refusal rather than claiming the transaction succeeded.
  Checked Disabled/Enabled/RequiresApproval/unknown handling, native refusal
  and idempotence through the injected service, without registering a host
  login item.
- **Overlay bounds:** the shell now actually passes `ui_channel` to the
  overlay runner. Its established #84 policy keeps the newest phase,
  preserves order while there is room, and retains up to 16 oldest errors;
  excess errors are counted and sparsely logged. The stalled-consumer test
  proves bounded storage, the retained error count and a final Recording
  state. The existing model/timer tests cover Idle after success, an error
  surviving Idle, expiration and a recording superseding the error.
  This policy intentionally does not retain every error during an unlimited
  stall. Native placement conversion uses independent arithmetic oracles;
  actual mixed-display placement remains a manual check.
- **Panic privacy and diagnosis:** hiding the entire payload is appropriate:
  arbitrary transcript text cannot be made safe merely by key scrubbing.
  The location, thread, ERROR record and optional backtrace remain. The real
  child-process test checks worker/request/error records in both stderr and
  retained rotated files before checking old/new fake keys and transcript
  sentinels, and verifies rotation without pruning. It also proves recovery
  after the panicking job. An independent executable using copies of the
  actual logging modules retained controls/location while hiding fake
  sensitive panic text; restoring the default hook exposed that text.
  `RUST_BACKTRACE=1` retained a useful stack without either fake sentinel.
- **Other scope:** inspected real-HTTP pipeline shutdown and backoff
  cancellation fences, exact uncommon-rate clip counts and endpoint
  alignment, singleton directory selection and cleanup, unique pasteboard
  naming, and always-run CI evidence collection/redaction/14-day retention.
  No new production dependency, network destination or copied external code.

## Independent validation

- Full `scripts/gate.sh`: **PASS**, approximately **223.62 seconds** from
  first to last gate-log write, including builds. Workspace nextest:
  **850 passed, 2 skipped**; explicit platform/testing run: **272 passed**;
  frontend: **89 passed**. Formatting, lint, dependency/license checks,
  doctests, vendor tests, spelling, guard checks and key-handling checks
  passed. Coverage: **core 98.93%, transcribe 98.61%, app 97.81%**.
- With four `yes` processes providing CPU load, **50/50 iterations passed**
  for each selected group: shell **17 tests**, app **4 tests**, platform
  **3 tests** (1,200 test executions). Includes bounded writes, command
  recovery/ordering, login validation/rollback, main-window ACL, real HTTP
  cancellation/backoff, spawned privacy, overlay bounds, singleton wake/lock
  and exact microphone-rate alignment. These are reviewer spot checks;
  they do not claim every changed test was independently stress-run.
- Inspected the new synchronization: entered/release channels, condvars,
  stop/drain acknowledgments and bounded receives provide the outcomes.
  Paused Tokio time drives overlay expiry; new tests do not rely on wall
  clock sleeps as proof. The disclosed older engine-test sleeps are unchanged.
- Read the whole PR diff, T3 spec, PR notes, #103 follow-up audit, engineering
  rules, reviewer role/checklist and T2 overlap. `git diff --check` passed.

### Reviewer mutation spot checks (outside the repository)

| Area | Mutation | Observed result |
| --- | --- | --- |
| Key writes | Ignore an occupied write slot | Existing stuck-write/retry test fails at `retry.is_err()`; baseline 2/2 passes. |
| Key writes | Treat a write deadline as `Ok(())` | Existing stuck-write test fails because the first call falsely succeeds. |
| Singleton wake | `delivering == 0` to `!= 0` in `Delivery::drop` | New wake-up test fails with `Err(Timeout)` versus `Ok(true)`; its cleanup releases the waiter. |
| Singleton lock | Invert the `Server::stop` begin guard, skipping its wait | New lock/shutdown test fails; correct baseline 39/39 passes. |
| Singleton budget | `io_timeout * 2` to `/ 2` | 39/39 still pass; M1. |
| Singleton accept | `is_stopping()` guard to `false` | 39/39 still pass; M1. |
| Panic privacy | Restore default hook after logging initialization | Fake key/transcript appear on stderr; current hook hides them and retains positive controls/location. |

Singleton checks copy the three actual modules into a minimal tracing crate;
key checks copy actual `key_writes.rs` with minimal error-type stand-ins.
The panic probe copies the actual logging modules and uses the real core
redaction registry. These isolate the reviewed mechanisms; the complete
gate above exercises their real workspace integration. Sources/results are
under `/tmp/t3-review-mutants`, `/tmp/t3-key-review`, `/tmp/t3-panic-review`;
gate/stress logs are `/tmp/T3-review-gate.log` and
`/tmp/T3-review-stress-{shell,app,platform}.log` on this host.

## Checklist and limits

| Checklist section | Result |
| --- | --- |
| 1. Scope | OK: issue-to-test mapping is explicit; residual mutations are M1. |
| 2. Correctness | OK: bounded saves, ordering, recovery, rollback, queue policy and cancellation checked as above. |
| 3. Security/privacy | OK: fake sentinels/positive controls verified; keys remain write-only, no new audio persistence, capability/CSP expansion, analytics or network destination. |
| 4. Architecture | OK: dependency direction, platform-only OS/unsafe code, pure core and existing shell-policy boundaries preserved. |
| 5. Rust/TypeScript | OK: gate and suppression review; keychain work stays off the event loop; typed IPC preserved. |
| 6. Licenses/reuse | OK: no new external reuse; deny/machete passed. |
| 7. Tests | OK with nonblocking M1/M2 and the verification limits below. |
| 8. Git/PR | OK: scoped Conventional Commits and detailed PR evidence; no guard edits; minor issues linked above. |

T3 criterion 4 (`tests/e2e/run.sh`) was **not run by this reviewer**, per
the explicit no-VM instruction. The gate's key-handling shell test is not
that native suite. Tester/Lead must supply or explicitly account for native
e2e/manual evidence before merge; CI is unavailable because of billing.
Failed-job artifact upload cannot be demonstrated locally; #128 remains a
follow-up until CI runs. No owner-screen interaction or VM use occurred.

## What merging T2 (#137) will require

Reconcile `save_with_login` deliberately: keep T2's `HotkeyControl`, `Saved`
result, partial-save reporting and hotkey undo, while validating all Settings
before either OS side effect, remembering the prior login state and undoing
both changes on file-write failure. Preserve the original config error if
either undo fails. Update the combined command tests and Settings tests to
exercise a failed write after both changes, and invalid input before either.

Keep T2's `launch_without_focus` helper in `platform/overlay/mod.rs` alongside
T3's conversion/placement helpers and tests. Preserve both startup/hotkey
wiring and the bounded overlay sender in `src-tauri/src/lib.rs`. Rerun the
gate and focused T2/T3 regressions on the combined head. This review does not
approve the unbuilt merge result. **No merge performed.**
