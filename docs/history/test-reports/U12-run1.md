# U12 test report, run 1

PR: #290 at 36d97b4
Result: PASS (with gaps: see "Not tested")

Run in `dictation-vm-2` (reset before the run), foreground, on the PR head.
Screenshots: `/Users/gauri/vocet-screens/u12/tester-run1/`.

## VM e2e (8 passed, 0 failed, 0 skipped)

`e2e_dmg_install`, `e2e_microphone_denied_then_recover`, `e2e_history`, `e2e_launch_at_login` (not yet run by the Developer on this head), plus `e2e_tray_menu_screenshots`, `e2e_hotkey_recorder`, `e2e_settings_save_reopen`, `e2e_settings_key_provider_recovery`.

`e2e_dmg_install` passing includes its key step (the made-up key, local stand-in provider, Test & Save says "✓ Connected · saved in Keychain").

## Criteria

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Back returns to Help (#277) | vitest `u12_criterion_1` (App.test.tsx, re-run: pass) | PASS (unit only) | Not driven by hand in the VM: the cua-vm-2 MCP connection closed mid-run. |
| 2 | Separate Troubleshoot / Report, titles, Details | Report/Troubleshoot vitest (pass) | PASS (unit only) | Window titles and save/copy/open not checked by hand. |
| 3 | Pick-one Speech list | Settings vitest (pass); `e2e_settings_save_reopen`, `e2e_settings_key_provider_recovery` | PASS (partial) | Model-download rows held (hotspot). The "..." menu at final width not checked by hand. |
| 4 | Key sheet | `e2e_settings_key_provider_recovery`, `e2e_dmg_install` | PASS | Sheet reopening after Troubleshoot not checked. |
| 5 | Dictation row | `e2e_hotkey_recorder`; vitest | PASS | 640x460 on one line not checked by me; the Developer's screenshot exists. |
| 6 | "Dictation key" wording | `wording.test.ts` (pass) | PASS | |
| 7 | Menu | `e2e_tray_menu_screenshots`; `menu-ready-dark.png` shows exactly the 5 rows | PASS | Developer's notes (Resume setup… while setup is incomplete; a fix row can read "Troubleshoot…") are in the PR; not exercised by hand. |
| 8 | Tokens/contrast | `tokens.test.ts` (pass) | PASS | Dark screens only seen in the menu capture. |
| 9 | Sidebar focus ring | CSS test only | NOT TESTED | Needs keyboard driving. |
| 10-12 | About, Keychain notice, Help/History labels | Settings vitest (pass); `e2e_history` | PASS (unit) | Keychain notice not seen on screen. |

vitest: 9 files, 173 tests, all pass (App, Settings, Report, Troubleshoot, HotkeyRecorder, wording, tokens, settings/).

## Not tested

- Held (model download, hotspot rule): `e2e_local_model`, `e2e_onboarding_screenshots`, `e2e_setup_no_permissions`, `e2e_onboarding_five_steps`.
- Hand checks that need the `cua-vm-2` MCP (it returned "Connection closed"): the #277 flow in the real app (menu bar History, then Help, Troubleshoot, Back), Report's save/copy/open, window titles, the "..." menu, sheet reopening after Troubleshoot, sidebar focus ring, Keychain notice, light/dark review of each changed screen.
- Full suite not run.

## Failures

None.
