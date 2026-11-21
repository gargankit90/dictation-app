# QT1 test report, run 1

PR: #340 at 01b3569 (plus one Tester commit, see Failures)
Result: FAIL

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | New unit/integration tests pass | `cargo nextest run --workspace` (the gate's runner) | PASS | 1520 passed, 0 failed |
| 1 | Same tests under plain `cargo test` | `cargo test -p dictation-pipeline --test cleanup_q2` | FAIL (minor) | `qt1_each_cleanup_logs_one_info_line...` fails 3/3 in parallel (0 lines captured); passes alone, with `--test-threads=1` and under nextest. See below. |
| 1 | Tests fail when behaviour breaks | 3 temporary mutations, reverted | PASS | guard-refused reply pasted -> `q2_criterion_2_a_reply_the_guard_refuses_pastes_the_raw_text` fails; `zdr` dropped -> `cleanup_sends_zdr_and_deny_with_the_attribution` fails; `DroppedNumber` accepted -> `the_guard_matrix` fails |
| 1 | Info log line has no text or key | code + test | PASS | `tracing::info!(outcome, duration_ms, "cleanup")`; only an enum name and a number; `e2e_cleanup_applied` also checks the log has no transcript text |
| 2 | `e2e_cleanup_applied` | VM `dictation-vm-2` | PASS | passed in every run (5/5) |
| 2 | `e2e_cleanup_deadline` | VM `dictation-vm-2` | FAIL (flaky) | passed 3 of 8 runs. App behaviour was right every time (`fallback_timeout` at 1.5-1.6 s, next dictation `applied`). |
| 2 | Full existing suite passes | `run.sh --no-build` | INCOMPLETE | the run was killed at the 30 min background limit after ~38 of ~50 scenarios (no results.txt); the VMs were busy with other runs for ~50 min before. The cleanup scenarios come late in the list, so I ran them separately. |

## Gate
CI on the PR was still running when I finished. Locally: nextest workspace run green. Coverage not measured.

## Untested behaviour found
None beyond the failures below.

## Failures
1. `e2e_cleanup_deadline` flaky, two causes, both in the test, not the app:
   - `within the bound` compared wall time from before the dictation (which takes 6-7 s on the loaded VM) with a bound of 8 s. Failed 3 times with the cleanup itself at 1.5-1.6 s. I changed it (commit `test(e2e): bound the deadline by the app's logged duration`) to assert the logged `duration_ms` <= 2500. Passed 1/1 after.
   - `the raw transcript was pasted` once found TextEdit empty at the check (run 2 after the change). The app log shows `paste_sent` 3 s after the `fallback_timeout` line, so the paste landed after the check; the VM was slow (another run was using the Mac, ssh banner timeouts followed). The check should wait for the text (poll up to a few seconds) instead of reading once. The Developer should fix and re-run the scenario 5 times.
2. Under plain `cargo test`, `qt1_each_cleanup_logs_one_info_line_with_the_outcome_and_no_text` (`crates/app/tests/cleanup_q2.rs:335`) captures no lines when run in parallel with its siblings. Likely tracing's callsite-interest cache with a scoped `set_default` subscriber. The gate uses nextest so it is green there; either use `tracing::subscriber::with_default` + `tracing::callsite::rebuild_interest_cache()` or note that the file needs nextest. Minor.
3. Not verified: the rest of the existing e2e suite (about 12 scenarios after `e2e_update_from_dictation_app` in `ALL_SCENARIOS`). Re-run once the VMs are free.
