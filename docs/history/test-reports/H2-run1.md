# H2 test report, run 1

PR: #307 at f869461
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Silent ≥ 1 s shows message + mic name, 6 s | e2e_silence, e2e_pill_states (dictation-vm-2); overlay/pipeline unit tests; h2_edges (non-ASCII, 5000-char, newline, HTML names render verbatim) | PASS | |
| 2 | No message for tap/click/empty/<1 s | pipeline test 15 999 vs 16 000 samples, clicks, empty; e2e_pill_states 0.5 s tap | PASS | 1 s boundary exact (≥) |
| 3 | Nothing sent/pasted, not in History | e2e_silence ("never inserts"), pipeline test (no transcriber call) | PASS | |
| 4 | One-off; next press records; Esc/cancel silent; tray unchanged | coordinator h2_* tests; h2_edges (press while shown, silent hold then dictation ends on Done, Esc); e2e_cancel_and_busy_presses | PASS | |
| 5 | Analytics: no event, no device name | usage.rs ignores NoSound; src-tauri/lib.rs logs a fixed line; overlay logs only the state name | PASS | |
| 6 | Unit + VM tests | as above | PASS | |
| 7 | [manual] screenshots | Lead/Design Lead | not checked | |

## Gate
CI at time of writing: DMG pass, Quality gate and Platform crate pending. Not re-run locally.
E2E on dictation-vm-2: e2e_silence, e2e_pill_states, e2e_overlay_rendered_states_and_error, e2e_cancel_and_busy_presses, e2e_no_words_transcript: 5 passed, 0 failed.

## Untested behaviour found
Added crates/app/tests/h2_edges.rs (4 tests, pass): odd mic names, key press while shown, silent hold then dictation, Esc.

## Failures
None blocking. Minor: a device name that is empty or blank gives "No sound · Check your microphone ()" (empty brackets). The OS rarely returns that; the same name path exists in the recording view. Tracked as a tech-debt issue.
