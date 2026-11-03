# T8 test report, run 2

PR: #227 at c8e52b9
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Lost-paste diagnostics are saved | forced loss: temporary local edit made Finder frontmost after the TextEdit wait (reverted, not committed) | PASS | Scenario failed as intended; saved e2e_error_bad_key_mock.app.log (17 lines) and .png, and logged frontmost app (Finder), phases after restart, tray state and insert lines |
| 2 | Saved files hold no key or transcript | grep of the saved log for key patterns (gsk_, sk-, api key, bearer) and the mock text | PASS | 0 matches for either |
| 3 | e2e_error_bad_key stable | 3 normal runs on dictation-vm-2 via the lock | PASS | 3 of 3 PASS |
| 4 | No new regressions | diff vs the run-1 head: only 6 lines in the scenario's failure branch | PASS | Passing path unchanged |

Notes: the first attempt found dictation-vm-2 locked by another full e2e run
(pid 70846, about 25 min); I waited for it. The #203 flake stays open as a
watch item; run 1's single failure is still unexplained.
