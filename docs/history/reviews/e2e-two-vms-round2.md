Reviewer: CHANGES REQUESTED

# Two test VMs review, round 2

PR: #146, `e2e-two-vms`, reviewed at `f7d6f15a9f2250fc17c074cfaeeb593e807310c5`.

Scope: only the fixes in `f7d6f15`, the round 1 findings R1-R4, and the associated regression coverage. Reviewed the Developer's fix summary and the round 1 report. All probes ran on macOS/Darwin arm64 against the actual checked-out shell libraries, with temporary lock directories and stubbed VM operations. No real VM was queried, started, changed, or deleted; the golden image and its backup were untouched. No merge performed.

## Findings

| # | Severity | Location | Finding | Suggested fix |
| --- | --- | --- | --- | --- |
| R1 | blocker, still open | `tests/e2e/lib_vm.sh:123`, `:127-128` | Removing an abandoned reclaim lock is a check-then-unlink operation without exclusion. Two contenders can both validate the same dead reclaim owner; a delayed contender then unlinks the other contender's new, live reclaim lock. Both can subsequently pass the main-lock comparison before either removes it. One then removes the other's newly acquired main lock. A deterministic probe produced two simultaneous live owners, both returning success. Atomic initial symlink creation fixes the original publication window but does not make stale recovery safe. | Make recovery of the reclaim mutex itself safe under competing recoverers. A safe minimal alternative is to fail closed on an abandoned reclaim lock and require quiescent manual recovery; an automatic protocol needs mutual exclusion that cannot itself be removed by a delayed stale observer. Re-reading a symlink before `rm` alone does not provide that exclusion. Add a deterministic concurrent recovery test with both the main and reclaim locks stale. |
| R4 | major, partially fixed | `tests/e2e/lib_vm.sh:296-297`, `:279-280` | The start lock is released even when the new VM never becomes visible in `lume ls`: the visibility timeout logs a warning and returns success. A second reset can then count the same one unrelated VM and issue another start. With two concurrent resets, successful stubbed `lume run`, delayed visibility, and `E2E_START_VISIBLE_WAIT=1`, both resets returned 0 and both started a VM: three including the unrelated VM. The default 30-second wait has the same fail-open branch. | Preserve capacity accounting until a successful start is visible, or safely cancel/stop the pending start and verify its state before freeing its reservation. On visibility timeout or repeated listing failure, do not permit another run to forget the pending start; merely exiting and releasing all locks is insufficient if that VM can still start. Add a timeout regression and a listing-failure-after-start regression. |

No new minor findings.

## Round 1 disposition

| Finding | Result |
| --- | --- |
| R1: two live owners | Original slow-publication probe now passes: one acquisition, loser exits 1. Native macOS `ln -s` refuses an existing dangling symlink with exit 1 and leaves its target unchanged. Live-owner checks work in the ordinary acquisition path. **Still open** because abandoned reclaim-lock recovery permits two owners; detailed schedule below. |
| R2: scenario trap replaces runner cleanup | **Fixed for the reviewed paths.** Re-ran the actual `arm_key_cleanup` failure through `run_scenario`, stubbing only log collection and key cleanup actions to avoid VM access. Scenario cleanup ran, runner exited 1, and the VM lock was absent. The focused suite also passes scenario TERM, runner TERM, nonzero scenario exit, and flag propagation. |
| R3: signal during acquisition leaks locks | **Fixed for the reviewed paths.** Injected TERM immediately after real `ln -s` publishes the VM lock and, separately, the golden-clone lock. Both runs exited 143; neither lock remained. The suite also passes before-acquisition, INT/HUP, reclaim-acquisition, and idempotent-release checks. Traps are armed before acquisition and ownership is registered before publication. |
| R4: concurrent capacity checks | **Ordinary successful-visibility case fixed; timeout case still open.** The suite's concurrent resets with one existing VM allow only one additional start; two resets with no existing VM both complete. The new probe demonstrates the limit can still be bypassed after visibility timeout. |

## R1 deterministic reproduction

Two independent Bash processes, A and B, source the real `tests/e2e/lib.sh`, share a fresh temporary `E2E_LOCK_DIR`, and call `arm_run_traps; vm_lock_take`. Seed `dictation-vm.lock` and `dictation-vm.reclaim.lock` as symlinks naming a PID whose process has exited. A shell wrapper around `rm` delegates to `command rm` after marker-file barriers; it changes no return values or lock data. It pauses only after the library's preceding comparison has completed, simulating a scheduler pause between comparison and deletion:

1. B reaches line 123, verifies the reclaim symlink still contains the dead PID, and pauses immediately before its `rm -f`.
2. A clears that dead reclaim lock, retries, acquires the reclaim lock with its own live PID, verifies the stale main lock at line 127, and pauses before line 128's `rm -rf`.
3. Resume B's pending reclaim unlink. It removes A's live reclaim lock using its already-completed stale check. B retries, acquires its own reclaim lock, verifies the still-stale main lock, and pauses before removing it.
4. Resume A. A removes the stale main lock, creates its own main-lock symlink, returns success, writes an acquired marker, and remains alive. Its reclaim release correctly leaves B's reclaim lock alone, but cannot restore exclusion.
5. Resume B. B removes A's new main lock using its already-completed main-lock comparison, creates its own main lock, returns success, and remains alive.

Observed output from the actual probe:

```text
R1 reclaim race: both live True stored owner 29286 2026-11-01 16:24:50
A exit 0 A acquired pid=29310 stored=29310 2026-11-01 16:24:50
B exit 0 B acquired pid=29286 stored=29286 2026-11-01 16:24:50
```

Both processes were confirmed alive before either was allowed to exit. Both logged takeover of the same dead PID, 29284. This is a reproducible mutual-exclusion failure affecting the shared VM, clone, and start lock primitive. The existing four-contender stale-main test has no stale reclaim lock, and the abandoned-reclaim test has only one contender; neither tests this schedule.

## R4 visibility-timeout reproduction

Two independent Bash processes source the real library, select different allowed VM names, share a temporary lock directory, and run `arm_run_traps; vm_lock_take; reset_vm`. Set `E2E_START_VISIBLE_WAIT=1` to shorten the supported timeout. Stub all Lume operations; `lume run` succeeds and records its target, but the listing remains the snapshot from before the detached starts become visible:

```bash
lume() {
  case "$1" in
    ls) echo '[{"name":"unrelated","os":"macOS","status":"running"}]' ;;
    run) echo "$2" >>"$PROBE/started" ;;
  esac
}
wait_for_vm() { :; }
```

Observed:

```text
R4 visibility timeout exit 0
warning: lume ls doesn't list dictation-vm as running yet; continuing
R4 visibility timeout exit 0
warning: lume ls doesn't list dictation-vm-2 as running yet; continuing
R4 starts: ['dictation-vm', 'dictation-vm-2'] total including preexisting: 3
```

This tests the explicit timeout branch, not a claim about normal Lume visibility latency. The first start has no capacity reservation after that branch, even though the code explicitly allows its status to remain unknown. The existing visibility test uses a stub that reports each start immediately; it never exercises a delayed listing or the timeout warning. Most cases set the visibility wait to zero.

## Lock ordering and checks

- Lock order is consistent in the reviewed callers: per-VM lock, then golden-clone lock, release clone lock, then vm-start lock, release start lock. No start-lock holder acquires or waits for a per-VM lock; no clone/start inverse ordering exists. I found no lock-order cycle between these locks. Clone/start acquisition waits are bounded. This does not remove the stale-recovery race above.
- `bash tests/e2e/test-vm-lock.sh`: **156 passed, 0 failed**.
- `bash tests/e2e/test-key-handling.sh`: **39 passed, 0 failed**.
- `bash -n tests/e2e/lib_vm.sh tests/e2e/run.sh tests/e2e/test-vm-lock.sh`: passed.
- Additional isolated macOS probes: native dangling-symlink creation semantics, original R1 slow publication, actual R2 key-trap failure, R3 VM/clone acquisition interruption, competing abandoned-reclaim recovery, and R4 visibility timeout. Outcomes are recorded above.
- Probe scripts and raw output were kept outside the repository at `/tmp/e2e-two-vms-round2-probes.py` and `/tmp/e2e-two-vms-round2-probes.log`; this report contains the durable evidence and reproduction schedules.
- Product crate coverage and real-VM acceptance checks were not repeated for this shell-only re-review. Publishing this review must pass the normal push hook; it does not replace the Tester's verdict or the Lead's manual checks.

## Checklist

| Section | Round 2 result |
| --- | --- |
| 1. Scope | OK: fixes and regressions for R1-R4 only; no unrelated product changes in the reviewed commit. |
| 2. Correctness | R1 remains blocker; R4 remains major on unknown visibility. R2/R3 probes pass. No per-VM/start lock-order cycle found. |
| 3. Security and privacy | OK for this delta: no new secret, transcript, audio, analytics, clipboard, capability, or network handling. Golden-name protection remains in place. |
| 4. Architecture | OK: shell test infrastructure; no crate or Tauri dependency changes. |
| 5. Rust and TypeScript practice | Not applicable to this delta; shell syntax checks pass. |
| 6. Licences and reuse | OK: no dependencies or declared external code reuse added. |
| 7. Tests | Existing focused suites pass; missing concurrent abandoned-reclaim and visibility-timeout coverage allowed the remaining failures. |
| 8. Git and PR | Developer summary read; no product, guard, AGENTS.md, or milestone edits by reviewer. Only this review file is committed. No merge. |

R1 and R4 require further fixes and regression coverage before approval. R2 and R3 need no additional changes based on this re-check.
