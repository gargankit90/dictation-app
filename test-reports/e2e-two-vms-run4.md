# e2e-two-vms test report, run 4

PR: #146 at bbc41b2 (R4 fix in 5282be4)
Result: PASS

| # | Check | Result | Notes |
| --- | --- | --- | --- |
| 1 | `tests/e2e/test-vm-lock.sh` | PASS | 271 passed, 0 failed |
| 2 | Revert `lib_vm.sh` to 5269348 (pre-fix), rerun | PASS | 248 passed, 23 failed (the three new tests); restored, worktree clean |
| 3 | Live: both VMs running, hand-made stale `vm-start` lock (pid 99999, dictation-vm), `run.sh --clear-stale-lock vm-start` | PASS | exit 1, "not removing the 'vm-start' lock: dictation-vm ... is still running ... lume stop dictation-vm"; lock left in place, then removed by hand |

No VM was started, stopped or modified. Full gate not run locally (CI green, push hook runs it).
