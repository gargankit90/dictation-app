# P2 test report, run 1

PR: #268 at d2126b4 (the branch tip f6430fc only adds the review file)
Result: PASS (two Settings checks could not be verified; see "Not verified")

VM: `dictation-vm-2`, build of d2126b4 (debug bundle). Tester's own probe scenarios were local and are not committed.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1a | After a tray Quit, relaunch, wait 3 s, the target is still frontmost and a dictation lands, x5 | local probe, TextEdit target | PASS 5 of 5 | frontmost was TextEdit before launch and 3 s after, every time |
| 1b | Same with Terminal as target, x5 | local probe (paste read from `cat` in raw mode) | PASS 5 of 5 | frontmost Terminal before and after; "hello world" received |
| 1c | Fresh launch with no prior Quit | local probe | PASS: TextEdit 2 of 2, Terminal 1 of 1 | |
| 2a | Open Settings from the menu, hotkey recorder gets keys without a click | `e2e_hotkey_recorder` | PASS | all steps, incl. refused letter, Esc, Right Cmd, relaunch |
| 2b | Typing in the API key field with no click | local probe | NOT VERIFIED | field is `AXSecureTextField`, its value can't be read back; my probe was inconclusive |
| 2c | Troubleshoot and Run setup again take focus | local probe | NOT VERIFIED | probe invalid: the AX press runs in a Terminal job, which leaves Terminal frontmost, and no second window showed |
| 3 | `e2e_secure_input_blocks_paste`, `e2e_first_paste_after_launch`, `e2e_overlay_focus`, `e2e_settings_save_reopen` | run.sh | PASS 4 of 4 | |
| 4 | `e2e_onboarding_first_run` fails the same way on main | run.sh on `ce5e9d8` (origin/main) | CONFIRMED | same 3 steps fail on main and on this PR: "Settings shows Help, Set up dictation…", "Set up dictation… … opens it again", "at the test dictation". Stale after U6; does not block. |

## Gate
Not rerun (CI and the Developer's `scripts/gate.sh` cover it). Unit tests `window::tests::*` not rerun.

## Untested behaviour found
- No permanent VM scenario for "launch doesn't take the front after a tray Quit" (the PR says so too). My probe is not committed: it is a rough local script.
- 2b and 2c above need a manual check or a better probe. `cua-vm-2` MCP closed its connection when I tried it, so I could not do them by hand.

## Failures
None attributable to this PR. `e2e_onboarding_first_run` fails identically on main.

Note: the probe's first Terminal attempts failed only because of my own capture (a canonical-mode tty holds a line without a newline); fixed with `stty -icanon`. The app log showed `paste_sent` and Terminal frontmost in those runs.
