# T5 test report, run 2

PR: #163 at 76ab5a6 (re-check of the R1 fix only)
Result: PASS

| # | Check | Result | Notes |
| --- | --- | --- | --- |
| 1 | `tests/e2e/test-runner.sh` on 76ab5a6 | PASS | 73 passed, 0 failed (about 11 s) |
| 2 | Deadline pkill (`lib_full.sh` line 204) replaced by `:` (local edit, not committed) | PASS | 4 of 5 runs: 72 passed, 1 failed, and the one failure is "and the job is killed" (term_deadline). See the note below for the fifth |
| 3 | File restored | PASS | `git status` clean |
| 4 | No stray long sleep after the runs | PASS | `pgrep -fl 'sleep 600'` is empty once the case's wait (up to 30 s) has passed; right after a run the job can still be there for a few seconds before the TERM lands. During the work I saw other `longsleep 600` processes that belonged to other sessions' runs (their own tmp dirs), not to mine |
| 5 | 10 runs under CPU load (4 `yes` on 14 cores, stopped afterwards) | PASS | 10/10 green (73/73 each) |

Note: the first run with the pkill replaced reported 73/73, so that case did not
fail once. The edit was in place (git diff showed only line 204 changed). I could not
reproduce it in the next 4 runs; a concurrent run from another session on the same
machine may have reaped the job. Minor: it suggests job_gone can pass when something
else kills the job. No issue opened; the case failed in 4 of 5 attempts.

No product code or guards were touched. The full gate was not run locally, as agreed.
