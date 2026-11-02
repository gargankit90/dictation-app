# e2e-two-vms test report, run 2

PR: #146 at f7d6f15 (re-check of round 1 findings R1-R4 only)
Result: PASS

| # | Check | Result | Notes |
| --- | --- | --- | --- |
| 1 | `tests/e2e/test-vm-lock.sh` | PASS | 156 passed, 0 failed |
| 2 | R1-R4 tests fail on the old code | PASS | new test file run against the pre-fix `lib.sh`, `lib_vm.sh`, `run.sh` (HEAD^): 113 passed, 43 failed. Failures include R1 (slow first owner: exactly one gets the lock, live holder refused), R2 (scenario trap/failure/TERM/non-zero exit), R3 (TERM/INT/HUP during acquisition and takeover), R4 (two resets at once with one other VM running) |
| 3 | Parallel real run, both VMs | PASS | `E2E_VM=dictation-vm-2` and `E2E_VM=dictation-vm`, `e2e_startup e2e_mock_paste`, started 2 s apart with `--no-build`: run B both 2/2 pass. Run A: vm-2 2/2, vm 1/2 (see notes), rerun of the pair: both 2/2 |
| 4 | Third concurrent run is refused | PASS | on each VM: `VM busy: <vm> is in use by a run (pid ...)`, exit 1, holder unaffected (it finished normally) |

## Notes

- Building twice in parallel in one worktree (no `--no-build`) fails: both
  `tauri build`s sign the same `target/debug/bundle/.../dictation-app.app` and
  one hits "No such file or directory". This is the shared build directory, not
  the lock. Build once, then start the two runs with `--no-build`.
  (Both runs failed at build and released their locks cleanly; the VMs were free.)
- First parallel pair: `dictation-vm` `e2e_mock_paste` failed once ("TextEdit holds: []",
  phases Recording..Idle logged). Same pair, same app, rerun: passed on both VMs.
  Treated as a flaky paste into TextEdit, not related to the lock fixes.
  Evidence: `tests/e2e/out/20261101-162653-dictation-vm/`.
- Not touched: dictation-vm-golden and its backup.
- Done with both VMs (left running, free).
