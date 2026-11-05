# U13 test report, run 1

PR: #289 at 5d66d07
Result: PASS

Tested in `dictation-vm-2`: `e2e_pill_states` and `e2e_overlay_rendered_states_and_error` both PASS (2 passed, 0 failed). Real-microphone behaviour is **untested**: the VM feeds a fixture WAV, not a voice. Screenshots are in `/Users/gauri/vocet-screens/u13/tester-run1-*.png`.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | SilenceWatch: No sound only before speech, after 2.5 s | `level::tests::criterion_u13_1_*` (3 tests, fake clock: silence from start at 2.5 s not before; speech then long pause never No sound; silence then speech returns for good); `e2e_pill_states` | PASS | VM: Listening shown 20:12:16.368, No sound 20:12:19.168 (scenario asserts about 2.5 s from Listening; passed). Sound after No sound returns to Listening is the unit test only, not in the VM. The 3 s mid-speech pause is the unit test and the Developer's recording only; the VM fixture has no pause. |
| 2 | Meter -50 to -20 dBFS, 4-18 px | vitest `src/overlay` (61 passed); `barHeight` reads `METER_LOW/HIGH_DBFS` | PASS | Developer's frames: speech bars span the full height; a pause shows seven flat dots with the label still Listening. |
| 3 | "Done" with check, 800 ms, no "Paste sent" | `e2e_pill_states` | PASS | Done frame (`tester-run1-e2e_pill_states.paste_sent.2.png`) shows the check and "Done". Scenario asserts 800 ms ±slack by the log: pass. `grep "Paste sent"` over src, crates, src-tauri, docs (excluding history), site, tests finds nothing. Setup's Try it still says "Pasted" (`TryStep.tsx`). Internal `paste_sent` log name unchanged. |
| 4 | Error 6 s | `e2e_overlay_rendered_states_and_error` | PASS | Rendered 6030 ms on screen, 6002 ms by the log; hidden afterwards. A new recording after the error shows and stays up. |
| 5 | site "Pasted" to "Done" | not checked | n/a | `site/` has no "Pasted" in this branch (grep); the spec allows leaving it to the Design Lead's site PR. |
| 6 | [manual] recordings | Developer's `pause-*.mp4`, `silence-*.mp4`, frames | Looked at frames | Not the Lead's sign-off. |

## Look at the pill
- Listening (speech): seven white bars at varied heights, filling the pill's height. Pause: bars collapse to flat dots, label stays "Listening".
- No sound: flat dots, "No sound · Check your microphone", after the delay.
- Done: green check and "Done". Error and Transcribing frames render.
- The pill is dark in both the Developer's light and dark frames, so light and dark look the same; I found no difference to check.

## Gate
CI not re-run by me. `cargo test -p dictation-core level`: 7 passed. `vitest src/overlay`: 61 passed.

## Untested behaviour found
- No VM test covers speech, then a pause, then speech, or sound after No sound; only unit tests do (fixture is one WAV).
- Real microphone: untested.
- Note: `e2e_pill_states` log timing is measured from the Listening log line, so the first-frame render delay is not included.

## Failures
None.
