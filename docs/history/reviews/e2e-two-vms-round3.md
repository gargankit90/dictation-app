Reviewer: CHANGES REQUESTED

# Two test VMs review, round 3

PR: #146, `e2e-two-vms`, reviewed at `5e449a5cb62ff1ab1600b3afb6e8430615a44b8d`.

Scope: only the round-2 lock fixes, their new recovery command, and associated regression tests. The merged T2 work was not re-reviewed. Probes used the actual shell libraries on macOS with temporary lock directories and stubbed Lume operations. No real VM was queried or used. No merge performed.

## Findings

| # | Severity | Location | Finding | Suggested fix |
| --- | --- | --- | --- | --- |
| R4 | major, still open on failure/interruption | `tests/e2e/lib_vm.sh:292-297`, `:196-211`, `:266-269` | `abort_start` releases `vm-start` even when `lume stop` fails and the VM may still be starting. A subsequent run can count the same old listing and issue another start. TERM after a successful detached start also runs generic EXIT cleanup, releasing the reservation without stopping the pending VM. Both paths reproduce two unconfirmed starts alongside one unrelated VM. The stated invariant, holding the lock until visible or stopped, does not hold. | Keep a durable reservation when cancellation cannot be confirmed; normal EXIT cleanup must not erase it. Handle interruption during the detached-start/visibility interval with the same rule. A fail-closed stale reservation requiring explicit recovery is consistent with the simpler design; recovery must require resolving the pending VM, not merely observing a dead PID. Add failed-stop and interruption regressions that assert a second run cannot start while the first remains unconfirmed. |

No new minor findings. R1's automatic recovery race is fixed. R4 still requires a change before approval.

## R1: automatic takeover removed

Re-ran the relevant abandoned-reclaim setup from round 2: seed both `dictation-vm.lock` and `dictation-vm.reclaim.lock` with an exited process's PID, then run two independent Bash contenders against the actual library. Both exited 1 with the stale-lock recovery command; neither acquired, and both symlinks retained their original values. The former comparison/deletion barriers are no longer reachable because acquisition performs no deletion or reclaim operation.

The ordinary current-format protocol has one atomic symlink creation and no incomplete PID publication. Without concurrent explicit clearing or external mutation, contenders cannot remove another owner's lock. The focused suite also passes slow acquisition, live-owner exclusion, stale-lock contention, and signal cleanup during acquisition. R2/R3 were already resolved in round 2 and were not independently re-reviewed.

## Manual clear: guarantees and limits

- The allowlist is exact and fixed to `dictation-vm`, `dictation-vm-2`, `vm-start`, and `golden-clone`. The suite rejects golden/backup names, reclaim names, path traversal, empty names, and other names.
- A lock containing an unrelated live process's PID was refused and preserved. PID reuse therefore fails closed; it does not authorize removal.
- A lock symlink pointing at a temporary directory outside the lock directory was removed without traversing it: the outside sentinel remained unchanged. The allowlist and quoted pathname prevent a supplied lock name from selecting another path. The configured `E2E_LOCK_DIR` remains a trusted override.
- There is no mid-write owner in the current atomic symlink format. The compatibility reader does accept old directory locks without a PID: a live process paused after creating its legacy directory had that directory removed by `clear_stale_lock`, which returned 0. This command cannot distinguish that process from an abandoned empty directory.
- The README explicitly requires clearing only when no run is starting on that VM. This quiescence requirement matters: the final comparison and `rm` are separate operations, so concurrent clearers followed by a new acquisition can also delete the new live lock. The command is a manual recovery procedure under that documented precondition, not a concurrency-safe takeover protocol. No separate finding is raised for violating that precondition. Its live-PID refusal does not establish a guarantee for unpublished legacy owners or overlapping manual recovery.

## R4 reproduction

Each process sources the real `tests/e2e/lib.sh`, selects one allowed VM, shares a temporary `E2E_LOCK_DIR`, and executes `arm_run_traps; vm_lock_take; start_vm`. `E2E_START_VISIBLE_WAIT=0` shortens the supported deadline; it still performs a visibility check. The stub is:

```bash
lume() {
  echo "$VM $*" >>"$PROBE/events"
  case "$1" in
    ls) echo '[{"name":"unrelated","os":"macOS","status":"running"}]' ;;
    run) touch "$PROBE/pending-$VM" ;;
    stop)
      [ "${FAIL_STOP:-0}" != 1 ] || return 1
      command rm -f "$PROBE/pending-$VM"
      ;;
  esac
}
```

The pending files represent starts that remain absent from the listing. Unlike the suite's call-only stop stub, a successful stop removes that pending state; a failed stop leaves it intact.

Successful-stop control: run each VM in sequence with `FAIL_STOP=0`. Both time out and exit 1, each cancellation removes its pending start, and the lock is released only after that stop returns. No pending starts remain. This fixes round 2's ordinary visibility-timeout path when stopping succeeds.

Failed-stop probe: run each VM in sequence with `FAIL_STOP=1`. Observed:

```text
dictation-vm   exit=1 start_lock=False pending=1
dictation-vm-2 exit=1 start_lock=False pending=2
peak including unrelated: 3
```

Both failures print `lume stop ... failed; it may still be starting`, but both release the start lock. A diagnostic asking an operator to stop the first VM does not protect a second run that is already waiting or starts independently. This reproduction needs no concurrent-clear command or lock-reclaim race.

Interruption probe: after the first stubbed `lume run` records its pending start, send TERM to its owning shell before visibility confirmation. It exits 143, the start lock disappears, and its pending start remains; no stop is called for that VM. A second run can then issue its own start from the same old listing. This is ordinary handled TERM, not uncatchable SIGKILL.

The suite currently explicitly asserts that the start lock is released after a failed stop (`test-vm-lock.sh`, `stop_fails` case). That expectation permits the remaining failure. Its timeout ordering test covers a successful stop call, and its listing-failure test covers the same successful cancellation path; neither proves safety when cancellation fails or is interrupted.

## Validation and limits

- `bash tests/e2e/test-vm-lock.sh`: **239 passed, 0 failed**.
- `bash tests/e2e/test-key-handling.sh`: **39 passed, 0 failed**.
- `bash -n tests/e2e/lib_vm.sh tests/e2e/run.sh tests/e2e/test-vm-lock.sh`: passed.
- Additional isolated probes: competing stale/reclaim owners, live PID reuse, outside symlink target, unpublished legacy owner, successful cancellation control, failed cancellation, and TERM after detached start. Results above.
- Probe source and raw output are outside the repository at `/tmp/e2e-two-vms-round3-probes.py` and `/tmp/e2e-two-vms-round3-probes.log`; this report contains the durable reproduction details.
- The full gate was started before publication; the normal push hook must also run without bypass. This review does not substitute for the Tester's verdict or the Lead's manual checks. Real backend cancellation behavior was not tested because VM use was prohibited.

## Checklist for this delta

| Section | Result |
| --- | --- |
| 1. Scope | OK: stale-lock simplification, explicit recovery, start visibility, and their tests only. Merged T2 changes excluded. |
| 2. Correctness | R1 fixed for automatic acquisition. R4 remains major on failed cancellation and handled interruption. VM then clone/start lock order remains consistent. Manual clear requires documented quiescence. |
| 3. Security and privacy | OK for this delta: exact recovery-name allowlist, no traversal of lock symlink targets, live PID refusal; no secret, transcript, audio, network, analytics, or capability changes. |
| 4. Architecture | OK: shell test infrastructure only. |
| 5. Rust and TypeScript practice | Not applicable to this delta; shell syntax checks pass. |
| 6. Licences and reuse | OK: no new dependencies or declared external code reuse. |
| 7. Tests | Focused suites pass; failed-stop expectation needs correction and interruption coverage is missing. Product coverage and real-VM acceptance not repeated. |
| 8. Git and PR | Only this review file is committed by the reviewer. No product, test, specification, or guard edits; no merge. |
