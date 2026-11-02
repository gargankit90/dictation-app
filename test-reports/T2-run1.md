# T2 test report, run 1

PR: #137 at 77a6e0b
Result: PASS (criteria 1-6 verified; 7 judged by reading, VM not used; 8 is the owner's)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Saved hotkey at startup and on save; failed re-registration keeps old and errors | `hotkey_settings` (13 pass), `commands::tests::hotkey` (4 pass) | PASS | Mutations, each caught: `startup_spec` ignoring the saved value (3 tests fail); old source stopped before the new one starts (`failed_reregistration_keeps_the_old_hotkey_and_says_why` and `a_hotkey_that_cannot_start_is_not_claimed_as_saved` fail); the failed-switch revert of `settings.hotkey` removed (same two fail). |
| 2 | Single-instance process tests 50x under load | `single_instance_launches` (6 tests) | PASS | `--stress-count 50` with one `yes` per core (hw.ncpu), 50/50 passed, 89 s. VM `e2e_single_instance_cold_race` not run by me (VM not granted); the Developer's run passed it. |
| 3 | ACL test fails on tray/menu permission or remote origin | `capability_tests::main_capability_denies_native_tray_and_menu_mutation` | PASS | I edited `capabilities/default.json` four times (`core:tray:default`, `core:menu:allow-new`, `core:resources:allow-close`, a `remote.urls` entry). The test failed every time; passes when restored. |
| 4 | Four Settings edge tests | `src/Settings.test.tsx` (60 pass) | PASS | Removed the `saving`/`testing` guard on submit: `enter_while_busy_starts_no_save` failed. Removed the stale-run check on Test key: `late_provider_test_result_is_ignored` and the older "drops a reply" test failed. The other two the Developer showed with its 7 mutations; I did not repeat those. |
| 5 | `npm run build` in CI | `scripts/gate.sh:74` | PASS | Step is in the gate that CI runs; it ran here (220 ms). CI itself can't run now (billing). |
| 6 | Stubbed guest failure turns scenario red; 39 key checks pass | `test-runner.sh` (62), `test-key-handling.sh` (39) | PASS | Mutations in `vm_terminal`: status always 0 (6 runner checks fail); any token accepted (2 fail); timeout returns 0 (1 fails). Restored. |
| 7 | Six new VM scenarios | read only | PASS (read) | See below. Not run by me. |
| 8 | Owner manual check | - | n/a | Owner's. |

## Gate
`scripts/gate.sh`: all checks passed, 4m31s locally (under the ~10 min budget). That covers fmt, prettier, clippy, deny, machete, nextest, doctests, coverage, tsc, eslint, vitest, `npm run build`, typos, `test-guards.sh` (63), `test-key-handling.sh` (39), `test-runner.sh` (62). CI not run (billing).

## Criterion 7: do the scenarios assert real outcomes?
Yes, I read all six. They check outcomes, not just that commands ran:
- `settings_save_reopen`: file contents, form values after close/reopen, `hotkey applied` log, a real Left Option hold pastes and Right Option does nothing, repeated after relaunch.
- `tray_error_clears`: tray title read through AX before and after the next recording; nothing pasted while the key is missing.
- `single_instance_cold_race`: process count, one `starting`/`ready`/`engine` line, N-1 forwarded, one tray item, a later press/release forwards and pastes.
- `secure_input_blocks_paste`: has a control paste (so the target can show a paste), Secure Input is checked with ioreg, then clipboard unchanged, error logged, nothing in the target.
- `user_copy_wins`: the copy is timed inside the paste window and checked as `pending=yes`; the clipboard change count shows nothing wrote afterwards.
- `quit_during_recording_then_relaunch`: Quit must end the app by itself, with no Inserting phase, TextEdit empty, no key held, and a dictation works after relaunch.

## Review of the two bugs the Developer fixed
- **Hotkey saved in canonical form** (`96ebd2b`): `the_hotkey_is_saved_in_canonical_form` and the e2e `LeftAlt` checks cover it.
- **`launch_without_focus`** (`73f1178`): there is no unit test, and none seems feasible, because it only sets a Tauri builder flag on macOS. Only `e2e_overlay_focus` guards it. Minor, filed as a tech-debt issue.

## Untested behaviour found
- `launch_without_focus` has no test outside the VM (above).

## Intermittent missed paste after a tray Quit: VM investigation
I recommend a VM investigation, queued after M5.5a and M5.4. It failed in 3 of the Developer's runs and passes depending on scenario order, so it could be a real focus bug in the relaunch path. It could also be an artifact of a synthetic Cmd+V in the VM. Only repeating `e2e_settings_save_reopen` then `e2e_error_bad_key` and capturing the AX key-window state of TextEdit after the relaunch can separate those. The owner's manual step 7 covers the real-Mac case. This is not a blocker for T2: the failing step is in an existing scenario, and the PR makes the trigger (a real tray Quit) newly reachable.

## Failures
None.
