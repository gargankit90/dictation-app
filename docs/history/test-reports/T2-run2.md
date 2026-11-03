# T2 test report, run 2

PR: #137 at 44b5451 (re-check of b04cc30 and 44b5451 since run 1 at 080ab94)
Result: PASS (VM not used by me; the Developer's fresh-VM run: 18 passed, 0 failed, 1 skipped)

| Check | How | Result |
| ----- | --- | ------ |
| Four new handover tests fail on the old handover | Temporarily restored `crates/app/src/hotkey_setting.rs` from 080ab94 (new tests kept): `the_old_key_is_not_heard_once_the_new_one_has_taken_over`, `a_new_key_that_fails_as_it_starts_reports_it_at_the_handoff`, `a_press_of_the_new_key_as_it_starts_is_heard_at_the_handoff`, `a_press_of_the_new_key_while_the_old_one_stops_is_heard_once` all FAIL (13 others pass). Revert not committed. | PASS |
| Same four pass on the fix, 100x under load | 8 `yes` busy loops; `--stress-count 100`, `--retries 0`: 100/100 iterations passed (43 s). Plain run: 17/17. | PASS |
| Run-1 tests still pass | Run-1 checks (hotkey_settings, Settings.test.tsx, ACL test, runner and key-handling tests) are all in the gate: green. | PASS |
| Gate | `scripts/gate.sh`: all checks passed (includes test-guards 63, key-handling 39, runner 62). CI not run (billing). | PASS |

## Post-Quit missed paste (44b5451)
I read the commit and the Developer's probe table in the PR comment. The evidence is consistent with a VM activation artifact: after a tray Quit, pastes missed 0/11 and 0/6 with a single AppleScript `activate`, including cases with no copy of the app running; they landed with a second activation of TextEdit once frontmost (6/6), and 8/8 and 3/3 with no tray Quit or on a fresh VM. The app isn't implicated. I did not re-probe (no VM).

## Scenario change: not loosened
`e2e_error_bad_key` only moves `groq_textedit_new` from before `launch` to after the "app restarts with the mock" log wait. All assertions are unchanged: the dictation ends Idle, the mock's text ("This is a test dictation") must be in TextEdit, and the phases include Inserting. `textedit_fresh` adds one more `activate` and a 1 s sleep once TextEdit is frontmost; it still returns failure if TextEdit never becomes frontmost. A paste that missed would still fail the scenario.

Note: the "second activate fixes it" claim rests on the Developer's VM probes; the sample sizes are small but the split is stark.

## Verdict
PASS. Nothing blocking, no new minors.
