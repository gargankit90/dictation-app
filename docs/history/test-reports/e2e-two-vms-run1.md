# e2e-two-vms test report, run 1

PR: #146 at 57844a1
Result: PASS

| # | Check | Result | Notes |
| --- | --- | --- | --- |
| 1 | `tests/e2e/test-vm-lock.sh` on the branch | PASS | 100 passed, 0 failed |
| 2 | Mutation: `validate_vm_name` accepts everything (golden/backup names refused) | caught | 23 failures |
| 3 | Mutation: `lume set` aimed at the golden image | caught | 7 failures |
| 4 | Mutation: EXIT trap does not release locks | caught | 8 failures |
| 5 | Mutation: `lock_release` does not remove the lock dir | caught | 12 failures |
| 6 | Mutation: stale lock never taken over (pid check always "alive") | caught | test script hangs (so it fails by timeout; no clean failure line) |
| 7 | Mutation: busy case no longer dies | caught | 6 failures |
| 8 | Mutation: remove the INT / TERM / HUP `trap` lines individually | NOT caught | 100 passed each time. bash runs the EXIT trap on a fatal signal anyway, so these three lines are redundant, not a test gap (they only fix exit codes 130/143/129). Not a blocker. |
| 9 | Real VM: `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_startup e2e_mock_paste` | PASS | 2 passed, 0 failed, 0 skipped; lock `dictation-vm-2.lock` held (pid 34801) during the run, gone afterwards |
| 10 | Second `E2E_VM=dictation-vm-2 run.sh --no-build e2e_startup` while 9 ran | PASS | stopped at once: `VM busy: dictation-vm-2 is in use by a run (pid 34801 since ...)`; first run's lock untouched, first run still passed |
| 11 | `lume get dictation-vm-golden` before and after | PASS | 4 CPU / 8.00G, stopped, unchanged; `dictation-vm` untouched; `dictation-vm-2` is 6.00G / 4 CPU |

Every mutation was a temporary local edit of `tests/e2e/lib_vm.sh`, restored with `git checkout`; the worktree was clean afterwards.

## Notes
- Stale takeover is covered by the test script (mutation 6 breaks it), but the failure mode is a hang rather than a readable FAIL line. Minor.
- Checks 9-10 ran one scenario subset only (not the full 13), per the brief.
