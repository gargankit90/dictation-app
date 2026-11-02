# e2e-two-vms test report, run 3

PR: #146 at 5e449a5
Result: PASS

| # | Check | Result | Notes |
| --- | --- | --- | --- |
| 1 | `tests/e2e/test-vm-lock.sh` on 5e449a5 | PASS | 239 passed, 0 failed |
| 2 | Same tests against round-2 `lib_vm.sh`/`run.sh` (temporary revert, restored) | PASS | 178 passed, 61 failed: the new tests catch the old code |
| 3 | Dead-pid lock for dictation-vm-2 (made by hand) | PASS | run fails: "stale lock ... remove it with: tests/e2e/run.sh --clear-stale-lock dictation-vm-2" |
| 4 | `--clear-stale-lock dictation-vm-2` on the dead lock | PASS | lock removed |
| 5 | Live-pid lock | PASS | run says "VM busy ... pid N"; `--clear-stale-lock` refuses, lock kept |
| 6 | `--clear-stale-lock dictation-vm-golden` / `foo` | PASS | refused; allowed names listed: dictation-vm, dictation-vm-2, golden-clone, vm-start |
| 7 | Real run `E2E_VM=dictation-vm-2 run.sh --no-build e2e_startup` | PASS | reset, boot, install, scenario passed in 29 s; no lock left in `~/.cache/dictation-e2e` |

dictation-vm and the golden images were not touched. Not exercised: the 120 s vm-start deadline path (covered by the unit tests only).
