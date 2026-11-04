# U5 test report, run 1

PR: #248 at 8233701
Result: FAIL

One e2e scenario the Developer updated but did not re-run, `e2e_accessibility_revoked`, fails in the VM, twice. Everything else I ran passes. The time box (60 min, much of it waiting for other runs' VM lock) ended before the manual VM walk-through; what was not checked is listed below.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Menu order per state | `tray::tests::criterion_1_*` (ready local/Groq, downloading, setup incomplete) | PASS | 17 `tray::` tests pass. Ready and bad-key menus seen in the VM (screens). |
| 2 | Status disabled, live, key + engine | `criterion_2_*`, `the_status_follows_a_changed_key_and_engine`; e2e status read | PASS | Status line shown dimmed in screens. 1 s refresh covered only by the shell's refresh loop, not timed by me. |
| 3 | One fix per code | `criterion_3_each_code_maps_to_exactly_one_fix_row` | PASS | Mutation checks below. |
| 4 | Title never set, dot icon, template images | `criterion_4_*`; e2e `tray title: []` in every scenario | PASS | Title empty in all three scenarios; icon states in light and dark below. |
| 5 | Copy last dictation, memory only | `last_dictation::tests::criterion_5_*` | PASS (unit) / NOT CHECKED in VM | Disabled state seen (greyed in the ready-menu screens). Copy text, and cleared after quit and relaunch, not driven in the VM. |
| 6 | No Reveal log; Show log file; Relaunch only as fix; Resume setup only while incomplete | `criterion_6_*`, vitest `Troubleshoot`, `App` | PASS (unit) / NOT CHECKED in VM | Menu in screens has no Reveal log and no Relaunch; "Resume setup…" shown with no key saved (onboarding incomplete). Show log file not opened in the VM. |
| 7 | e2e scenarios updated | `e2e_tray_error_clears_on_next_recording`, `e2e_tray_menu_screenshots`, `e2e_accessibility_revoked` | **FAIL** (1 of 3) | See Failures. |
| 8 | [manual] light and dark icons and menus | `e2e_tray_menu_screenshots` | PASS (partial) | Screens in `U5-screens/`. Crowded menu bar not checked. |

## Gate
- Hosted CI on 8233701: Quality gate (Linux) pass, Platform crate (macOS) pass, DMG (macOS) pass.
- `cargo nextest run -p dictation-pipeline -p dictation-app`: 485 run, 485 passed, 1 skipped.
- `npx vitest run`: 195 passed.
- Coverage: not re-measured (CI gate covers it).
- Licences: `cargo deny check` ok (advisories, bans, licenses, sources); `scripts/check-licences.sh` exit 0. New crates in `Cargo.lock` vs `origin/main`: `image 0.25.10`, `moxcms 0.8.1`, `pxfm 0.1.30`, `byteorder-lite 0.1.0`; all four appear in `THIRD-PARTY-LICENSES.txt`.

## Mutation check (fix-row mapping, `crates/app/src/tray.rs`)
- `MicDenied => OpenAccessibilitySettings`: 3 `tray::` tests fail (`criterion_3_each_code_maps_to_exactly_one_fix_row`, `a_microphone_denied_at_launch_shows_at_once`, `criterion_6_relaunch_replaces_the_grant_fix_after_it_was_opened`).
- `HotkeyUnregistered => Troubleshoot`: 3 tests fail (`criterion_3_*`, `the_hotkey_outage_is_a_problem_until_restored`, `the_hotkey_fix_is_the_grant_when_accessibility_is_missing`).
- Both restored (`git checkout`); worktree clean.

## E2E (VM `dictation-vm-2`, `E2E_VM=dictation-vm-2 tests/e2e/run.sh --no-build ...`)
- `e2e_tray_error_clears_on_next_recording`: PASS (no key: status `! No API key is saved.`, fix `Open Speech settings…`, empty title).
- `e2e_tray_menu_screenshots`: PASS (ready, no-key and recording, each in light and dark; 10 screenshots).
- `e2e_accessibility_revoked`: FAIL, twice (runs 00:18 and 00:34).

## Screenshots (`docs/history/test-reports/U5-screens/`)
`icon-normal|attention|recording-light|dark.png`, `menu-ready-light|dark.png`, `menu-bad-key-light|dark.png`, `err-no-key.png`. Menu bars in the VM are light and dark as named. No tray title in any. The bad-key menu: `! No API key is saved.`, `Open Speech settings…`, `Copy last dictation` (disabled), Settings…, Troubleshoot…, Report a problem…, Resume setup…, Check for updates…, Quit Vocet.

## Not checked (time box)
In a VM: Downloading state, Ready on Groq vs local (the screens show only the no-key and ready-with-Groq-off states from the scenario), mic denied, Accessibility missing menu beyond the scenario's tray read (it read `! Accessibility is off for dictation-app.` with `Open Accessibility settings…`, PASS), hotkey unavailable with Accessibility granted, setup incomplete beyond the bad-key menu, Copy last dictation after a dictation and after relaunch, Show log file from Troubleshoot, crowded menu bar. Unit tests cover the model for these; the shell wiring is not seen.

## Untested behaviour found
- The 1 s refresh promise (criterion 2) has no timed test; only the loop's existence.

## Failures
### `e2e_accessibility_revoked`: a real hold doesn't dictate in the running app after Accessibility is switched back on
Steps: `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_accessibility_revoked`.
- Passes: status/fix while off (`! Accessibility is off for dictation-app.`, `Open Accessibility settings…`), typing still works, switch on, tray returns to `✓ Ready · Hold Right ⌥` within 15 s.
- Fails: `a real hold dictates in the running app` (TextEdit stays empty, 15–20 s after Ready). After quit and relaunch, `a real hold dictates after the relaunch` passes (`hello world`).
- Expected: the hotkey listener recovers by itself, as the old scenario asserted. Actual: the menu says Ready but the hold does nothing until relaunch.
- Same failure both runs. I did not run the scenario on `main`, so I can't say whether this is a regression from U5 or the same on main (the `tray_is_clear` and hold asserts are unchanged apart from the tray text). The scenario was updated by this PR and not re-run by the Developer. The U5 menu shows Ready here although the hotkey listener isn't hearing keys; if that is the cause, the status is wrong, which is U5's to fix. Needs a run on `main` (or the Developer's look at the listener recovery path).
