# U8 test report, run 3

PR: #269 at 8202794
Result: PASS

All in `dictation-vm-2`, VM reset before every run (one scenario per run), `--no-build` after one build of 8202794.

| # | Check | Test | Result | Notes |
| - | ----- | ---- | ------ | ----- |
| 1 | First run, own key, three times | `e2e_onboarding_first_run` (3 resets) | PASS x3 | the failing run-2 step (typing the key into the password field) now passes every time |
| 2 | Five steps | `e2e_onboarding_five_steps` | PASS | |
| 3 | Microphone denied, then recover | `e2e_microphone_denied_then_recover` | PASS | |
| 4 | History (U7) | `e2e_history` | PASS | |
| 5 | Ready's History row opens Settings → History, setup closed | tester-local scenario `e2e_u8_r3_extra` (not committed) | PASS | the row's Change pressed; Settings showed "Keep recent dictations"; no "Set up" window left on screen |
| 6 | Practice words and history | same scenario | PASS | limit 25: the practice words are in `history.json` and listed in Settings → History. History Off (set in Settings, Delete confirmed, file removed), setup run again, practice dictation: no `history.json` created |
| 7 | Welcome: rapid sharing on/off, then Continue | `e2e_onboarding_round2` (5 rounds off+Continue) and tester-local `e2e_u8_r3_rapid` (6 x on/off at once, ends off, Continue) | PASS | settings say `share_usage_events:false`, no install ID file, log has no send/drop of events |
| 8 | Ready light and dark screenshots | `e2e_u8_r3_extra` | done | `U8-run3/ready-light.png`, `U8-run3/ready-dark.png` |

## Screenshots

Terminal windows were closed first (`close_terminal`). `screencapture -l <window id>` failed over ssh; the fallback, a full-screen `screencapture` cropped to the window, held only the wallpaper (an ssh session's capture has no app windows), so the pictures come from the cua-driver daemon's window capture (`get_window_state`), as in earlier runs. Appearance was set explicitly before each launch (the VM image starts dark).

## Notes

- Resuming "Run setup again" from an unfinished setup opens at Ready (the saved step), not Welcome. Not a defect; noted because my first draft assumed Welcome.
- Tester-local scenarios (`e2e_u8_r3_extra`, `e2e_u8_r3_rapid`, an `ax.js press-nth` command) were not committed; per the brief only the report and screenshots are.

## Gate

Not re-run (CI and the push hook run it).

## Untested behaviour found

None new.

## Failures

None.
