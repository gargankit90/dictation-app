# U12 test report, run 2

PR: #290 at 021c205
Result: PASS (the by-hand GUI checks are unit-covered only; see "Not tested")

`dictation-vm-2`, reset before the run, foreground. Screenshots: `/Users/gauri/vocet-screens/u12/tester-run2/`.

## VM e2e (4 passed, 0 failed, 0 skipped)

`e2e_setup_no_permissions`, `e2e_onboarding_five_steps` (neither run by the Developer), `e2e_settings_key_provider_recovery`, `e2e_tray_menu_screenshots`.

## Round 2 items

| Item | Evidence | Result |
| ---- | -------- | ------ |
| Provider change in the key sheet doesn't switch a working engine until a key works | vitest "doesn't switch to a provider with no key", "Done goes back to the provider in use", "switches at once to a provider that has a saved key", "a slow lookup can't switch after the provider changed again"; `e2e_settings_key_provider_recovery` (local stand-in, made-up key) | PASS |
| Done or Esc during Remove key, then reopen, shows the true state | vitest "Done and Esc wait for it, then the sheet says no key is saved", "Speech reopened during it doesn't say saved about the deleted key", "after leaving Speech and coming back, an old test doesn't undo Remove key" | PASS (unit) |
| Add key… vs Change key… | vitest "the row says Add key… with no key saved, and Change key… with one" | PASS (unit) |
| Back to Settings visible in Troubleshoot and Report | vitest "keeps Back outside what scrolls" in both | PASS (unit); 640x460 not looked at on screen |
| 5-row menu | `menu-ready-light/dark.png` from `e2e_tray_menu_screenshots` | PASS |
| Dictation key while recording at minimum size | vitest "recording says Press a key… with Use default and Cancel"; `e2e_hotkey_recorder` passed in run 1 | PASS (unit); minimum size not looked at on screen |

vitest, whole suite: 16 files, 387 tests, all pass.

## Not tested

- The `cua-vm-2` MCP connection closed again ("Connection closed" on the first call), so nothing was driven by hand: the sheet and Remove key flows, Back at 640x460 in Troubleshoot and Report, and the recording keycap at minimum size are covered by unit tests only. The #277 flow and the other run 1 hand checks remain unit-only too.
- No other scenarios were run (full suite not requested).

## Failures

None.
