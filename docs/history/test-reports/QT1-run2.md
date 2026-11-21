# QT1 test report, run 2

PR: #340 at d04c12b (Developer's fixes d04c12b)
Result: PASS

| # | Check | Result | Notes |
| - | ----- | ------ | ----- |
| 1 | Log test and `cleanup_q2` under plain `cargo test` | PASS | `cleanup_log` 1/1 and `cleanup_q2` 6/6; `cargo test -p dictation-pipeline` run 3 times, all green |
| 2 | `e2e_cleanup_deadline` x3 | PASS | 3/3 on dictation-vm (run 1 together with applied) |
| 2 | `e2e_cleanup_applied` | PASS | 1/1 |
| 2 | Full existing suite | PASS | 55 passed, 0 failed, 3 skipped (all optional: `e2e_real_openrouter`, `e2e_update_keychain_control`, `e2e_release_build_smoke`); covers the scenarios run 1 could not reach |

## Gate
Not re-run beyond the pre-push gate on my push; CI on the PR is the authority.

## Untested behaviour found
None new. `e2e_real_openrouter` (the cleanup outcome line against the real key) is the Lead's criterion 3.

## Failures
None. The run-1 flakes (wall-clock bound, single-shot paste check, parallel log test) are fixed.
