Reviewer: CHANGES REQUESTED

# Two test VMs review, round 1

PR: #146, `e2e-two-vms`, reviewed at `57844a1115a02c1c0a9d9aa7eeb3c943d09c4d34`.

Scope: the complete six-file PR diff, with particular attention to golden-image protection, per-VM and clone locks, cleanup, the VM limit, and the stubbed tests. No real VM was queried, started, changed, or otherwise used. This task does not authorize merging.

## Findings

| # | Severity | Location | Finding | Suggested fix |
| --- | --- | --- | --- | --- |
| R1 | blocker | `tests/e2e/lib_vm.sh:65-78` | A missing PID is treated as proof that an owner is dead after just one second. A live process can still be between `mkdir` and publishing its PID. A contender removes its directory and acquires the same lock; the original process then finishes publishing and also reports success. Both runs can reset the same VM. The reclaim mutex only serializes reclaimers, not the initial acquirer. | Make acquisition and reclamation use one ownership protocol in which an incomplete publication cannot be mistaken for a dead owner. Check a verifiable owner before destructive takeover; do not use a timeout alone to establish death. Add an overlapping acquisition test with the first owner paused before PID publication. Account for interruption of the reclaim operation as well. |
| R2 | major | `tests/e2e/run.sh:104-105` | Re-arming traps after a scenario returns cannot release the VM lock when that scenario exits or receives a signal. `arm_key_cleanup`, `groq_guard_start`, and the login-item scenario replace the runner's EXIT trap. A scenario failure executes its own cleanup and exits before line 105, leaving the VM lock behind. | Keep the runner's lock cleanup active for the entire scenario, including failure and signals, while preserving scenario cleanup and exit status. Isolating scenarios with a parent retaining ownership is one option; composing cleanup is another. Test failure and signals while the real scenario cleanup trap is armed, rather than only after clearing and re-arming it. |
| R3 | major | `tests/e2e/lib_vm.sh:117-121` (also `lock_take:91-98`) | Lock ownership is registered for EXIT cleanup only after `lock_try` finishes. A signal after PID publication but before that registration leaves a lock despite the signal and EXIT traps having run. The same gap exists for the clone lock. | Ensure cleanup knows about an acquired directory throughout acquisition and publication, including signal interruption, without letting a losing contender remove someone else's lock. Add fault injection during acquisition for both lock types. |
| R4 | major | `tests/e2e/lib.sh:54-59`, `tests/e2e/lib_vm.sh:166-169` | The two-VM check and VM start are not in the same shared critical section. With one unrelated macOS VM running, concurrent resets of the two test VMs both see one other VM and pass. The clone mutex serializes only `lume clone`, so both later issue `lume run`, attempting to exceed the limit. The preflight guard does not enforce the advertised limit under the concurrency this PR introduces. | Serialize the count/check and start, or reserve capacity under a shared lock until start completes. Keep lock ordering consistent with the VM and clone locks. Add a concurrent two-reset test with one other VM already running and assert that only one additional start is allowed. |

## Reproduction evidence

The following probes ran against the checked-out libraries in temporary directories outside the repository. All VM operations were replaced with shell stubs. No product, test, specification, or guard file was changed.

### R1: two live owners acquire the same lock

Two independent Bash processes sourced the real `lib.sh` and shared a temporary `E2E_LOCK_DIR`. In the first process, a shell wrapper around `date` paused for three seconds when `lock_write` requested `+%F %T`, signaling a marker first. This pauses the actual acquisition after its successful `mkdir` and before PID publication. After that marker, the second process called `vm_lock_take` normally and stayed alive for four seconds. Both armed the real run traps.

Observed output:

```text
owner exit 0
owner acquired pid=13447 stored=13447
contender exit 0
contender acquired pid=13455 stored=13455
contender still running but lock removed
took over the stale 'dictation-vm' lock (pid unknown is gone)
```

The second process acquired after the one-second grace period while the first was still alive. The first subsequently completed acquisition and its EXIT cleanup removed the lock while the second was still running. This is a mutual-exclusion failure, not merely unnecessary waiting. The shared primitive is also used for `golden-clone`.

### R2: scenario failure bypasses lock cleanup

A fresh Bash process sourced the actual `lib.sh` and `lib_full.sh`, stubbed only `collect_log` and `key_cleanup_steps` to avoid VM access, and ran:

```bash
arm_run_traps
vm_lock_take
arm_key_cleanup e2e_hold_release
die 'simulated scenario failure'
arm_run_traps
```

Observed exit status: **1**. The VM lock directory and PID file remained. This uses the real scenario trap implementation; the final re-arm is unreachable, just as in the runner.

### R3: signal during acquisition bypasses lock cleanup

A fresh Bash process sourced the actual library and ran:

```bash
arm_run_traps
mv() { command mv "$@"; kill -TERM $$; }
vm_lock_take
```

The wrapper injects TERM immediately after the real PID-file rename. Observed exit status: **143**. The VM lock directory and PID file remained because `E2E_HELD_LOCKS` was still empty. Signals after acquisition completes pass the existing tests, but that does not cover this window.

### R4: concurrent resets pass the limit independently

Two Bash processes sourced the actual library, selected different allowed VMs, shared a temporary lock directory, and each ran `arm_run_traps; vm_lock_take; reset_vm`. The stubbed `lume ls` waited until both processes had reached it, then returned a snapshot containing one running unrelated macOS VM. The stub recorded `lume run`; `wait_for_vm` was a no-op. Both processes exited **0**, and both VM starts were recorded: one pre-existing VM plus two starts, **3** in the stub. No real Lume command ran. Enforcement by Lume or macOS itself does not make this preflight check atomic.

## Tests and limits

- `tests/e2e/test-vm-lock.sh`: **100 passed, 0 failed**.
- `tests/e2e/test-key-handling.sh`: **39 passed, 0 failed**.
- `bash -n tests/e2e/lib_vm.sh tests/e2e/run.sh tests/e2e/test-vm-lock.sh`: passed.
- The four additional probes above reproduced the findings without VMs.
- The new tests cover sequential happy paths, already-published live/dead PIDs, an already-empty directory, and signals after acquisition. They do not overlap acquisition with PID publication, fail inside an armed scenario, interrupt acquisition, or overlap two capacity checks. The re-arm test at `test-vm-lock.sh:193` only tests successful return to the runner.
- PID reuse currently fails closed: an unrelated live process with a reused PID keeps an old lock busy. This reduces availability but does not authorize unsafe takeover. The PR notes acknowledge it; no demand for a more elaborate process-identity system is made here.
- An abandoned `.reclaim` directory is not cleaned by the run traps and prevents later stale takeover while the old lock directory remains. The documented manual recovery is a limitation to consider when fixing R1; a recovered lock must never be removed from a live new owner.
- The current lock order is VM lock, then clone lock; there is no reverse acquisition in the reviewed code. R4's fix must retain a consistent order.
- No separate full gate was run during inspection; the mandatory push hook is allowed to run normally when publishing this review. The focused checks above do not substitute for the Tester's report or the Lead's manual checks.

## Checklist

| Section | Result |
| --- | --- |
| 1. Scope | In scope: second VM selection, clone resources, locking, limits, documentation, and gate integration. Required concurrent and interruption behavior needs the regressions described in R1-R4. |
| 2. Correctness | R1-R4. The sequential reset order and per-VM routing are otherwise consistent. |
| 3. Security and privacy | OK for this diff. Exact quoted name comparisons reject golden/backup names, spaces and path variants; incoming `VM`, `GOLDEN`, and allowed-name environment values are overwritten. Resource overrides are passed as single quoted arguments. Golden is used only as clone source on the normal runner path; memory/CPU settings validate the clone name again. No new secret, transcript, audio, analytics, or network handling. |
| 4. Architecture | OK; shell test infrastructure only, no crate dependency or Tauri boundary changes. |
| 5. Rust and TypeScript practice | Not applicable to this diff; shell syntax checks passed. |
| 6. Licences and reuse | OK; no dependencies or declared external code reuse added. |
| 7. Tests | Existing focused tests pass, but R1-R4 are untested and reproduced with isolated probes. Coverage thresholds for product crates are unchanged. |
| 8. Git and PR | PR description explains scope, local gate and VM evidence, known limitations, and follow-ups. No protected guard, AGENTS.md, or milestone specification edits in the diff. Only this review file is changed by the reviewer. No merge performed. |

No minor findings were raised. Fix R1-R4 and add the corresponding regression coverage before re-review.
