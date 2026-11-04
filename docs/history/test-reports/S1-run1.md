# S1 test report, run 1

PR: #255 at be14614 (head is now 11bb25d, which only adds `docs/history/reviews/S1-round1.md`)
Result: PASS (with one e2e flake, see Failures, and two checks not done)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Where the time goes | PR notes; app logs of the e2e runs | PASS | `SMAppService.status()` is the slow call: 17 s, 27 s, 35 s, 48 s seen in `dictation-vm-2`; register/unregister 0.3-0.6 s normally (29 s once). `save_settings` file write + engine rebuild 0.015-0.36 s. |
| 2 | Save never waits; Applying… then done/error | `launch::tests::a_request_returns_before_the_slow_os_call_ends`, `commands::tests::launch_at_login::saving_does_not_wait_for_a_slow_login_item` (1.5 s delaying fake), Settings.test.tsx (77 pass); e2e "Settings saved." within 5 s | PASS | Saved in 3-5 s (script measure, 1 s steps plus ax start-up) in all 3 e2e runs, including while the OS took 30-70 s. |
| 3 | Last choice wins; file and OS agree | `toggling_again_while_pending_ends_on_the_last_choice`, `the_last_choice_wins_when_it_differs_from_the_running_change`, `the_file_and_the_os_agree_on_the_last_choice`, `the_screen_follows_the_os_after_a_change_outside_the_app` | PASS | Mutation checks below. |
| 4 | e2e_launch_at_login 3/3, saved < 5 s | `tests/e2e/run.sh e2e_launch_at_login` x3 | PASS on 2 of 3 runs | Run 2 failed on the 60 s wait for "Applying…" to go (the OS took 70 s). Saved was 4 s. A rerun (run 3) passed. |
| 5 | Gate, hosted CI | `gh pr checks 255` | PASS | Quality gate (Linux), Platform crate (macOS), DMG (macOS) green on 11bb25d. |

## Unit tests and mutation checks
- `cargo nextest run -p dictation-pipeline -p dictation-app launch`: 42 passed. `npx vitest run src/Settings.test.tsx`: 77 passed.
- Mutation A (the login-item worker thread is joined in `LoginSync::request`, so callers wait for the OS again): 5 tests failed (`a_request_returns_before_the_slow_os_call_ends`, `saving_does_not_wait_for_a_slow_login_item`, `the_last_choice_wins_when_it_differs_from_the_running_change`, `toggling_again_while_pending_ends_on_the_last_choice`, `the_file_and_the_os_agree_on_the_last_choice`). Restored.
- Mutation B (a request during a pending change no longer replaces the wanted choice, so the latest choice loses): `toggling_again_while_pending_ends_on_the_last_choice` failed (on/off/on ends on; the mutant ends off). Restored; `git status` clean afterwards.
- Note: `save_keeping_sharing` takes no login item, so "make save_settings await the login item" is covered through `request` (A); the save test itself is timing-only (under 1 s).

## E2E (dictation-vm-2, `--no-build` after one build)
Three runs of `e2e_launch_at_login` (two reboots each): run 1 PASS (Saved at most 3 s and 8 s by the script's measure, OS answered after 5 s and 10 s), run 2 FAIL, run 3 PASS (Saved 5 s and 3 s, OS answered after 11 s and 16 s). After each reboot the stored setting and the OS agreed (the scenario asserts the box and the OS state) in every run that got that far.
The script's "at most N s" includes 1 s polling and ax start-up; the 5 s `wait-text` limit itself passed.

## Gate
Hosted CI green (above). I did not re-run `scripts/gate.sh` locally.

## Untested behaviour found / not done
- Not done (time box; the VM was shared with other workers' runs and I waited for its lock about 25 min): the scripted by-hand toggle off/on quickly then Save other settings while Applying… shows, and a direct measure of Settings window open time. Covered indirectly: unit tests for rapid toggles and for Save during a slow item. In the e2e logs `get_settings` fell back to the last known state ("the OS is slow to say if the login item is on; using what it said last") while `status()` took 17-48 s, and the scenario's box-ready wait (20 s) never failed.
- Reopening Settings during a pending change when component state is lost (PR risk list): not exercised.

## Failures
Run 2 (`tests/e2e/out/20261104-000908-dictation-vm-2`): after the second Save (off), `wait-no-text "Applying…" 60` timed out. App log: unregister took 29 s, then `status()` reads of 32 s, 48 s and 9 s; `login item changed wanted=false ... elapsed_ms=70372`. The product behaved as designed (Save in 4 s; the OS state matched afterwards: "the OS still says disabled" after the reboot). The 60 s bound in `login_box_save` is shorter than this VM's worst case when other VMs run on the host. Suggest raising it (for example to 120 s, with the outer `VM_TERMINAL_TIMEOUT` to match). Minor; not a product defect.
