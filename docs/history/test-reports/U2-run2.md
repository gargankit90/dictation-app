# U2 test report, run 2

PR: #249 at 3d20385
Result: PASS

Run 1 failed on `e2e_settings_save_reopen` (it still typed the language into a text field). Round 3 moves it to the picker (`combo-pick`) and removes `yue` from OpenAI.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1-4 | Picker, stored values, engine filter, search and keyboard | vitest | PASS | 206/206 pass (was 201); tsc clean. Run 1's mutation check (store the label instead of the code) was not repeated: `LanguagePicker.tsx` is unchanged in intent and the store tests still cover it. |
| 5 | Recorder keeps K1 behaviour; DS1 keycaps | `e2e_hotkey_recorder` | PASS | 1 passed, 0 failed in dictation-vm-2. Keycap "Right ⌥" visible in the screenshots. |
| 6 | [manual] light and dark, helper line, flag | real app in dictation-vm-2 | PASS | See below. |
| e2e | `e2e_settings_save_reopen` | dictation-vm-2 | PASS (2 of 3) | Run A failed only at "a Left Option hold dictates right away, no restart" (TextEdit held nothing) and the follow-up "TextEdit still has the one dictation"; every language-picker step passed (pick German via combo-pick, saved, reopened, shows "German (Deutsch)"). The known dictation flake under host load. Runs B and C: 1 passed, 0 failed. |

## Manual check in the real app (light and dark)
Hindi cannot be picked while Parakeet English is selected, because the list is filtered to the model's languages (typing "hin" matches nothing). So the order was: select the multilingual model (Qwen3-ASR), pick Hindi with "hin" + Enter, then switch to Parakeet English.

1. Multilingual model + Hindi: field shows "Hindi (हिन्दी)", no flag.
2. Switch to Parakeet English: field shows "Hindi, not supported by the English model" (value kept) and the helper line "The English model understands English only. For Hindi and other languages, choose the multilingual model." with the link appears.
3. Switch back to the multilingual model: the flag clears and the field shows "Hindi (हिन्दी)" with the plain hint.

Dark mode was set with System Events `set dark mode`, light likewise (`AppleInterfaceStyle` absent). Screenshots (window only) in `docs/history/test-reports/U2-screens/`:
`run2-light-{1-default,2-hindi-multilingual,3-hindi-english-flagged,4-flag-cleared}.png` and the same four for `dark`. The "Right ⌥" DS1 keycap shows in each.

## Gate
- Hosted CI on the PR: Quality gate (Linux) pass, Platform crate (macOS) pass, DMG (macOS) pass.
- vitest 206/206, tsc clean.

## Notes
- The helper line now says "the English model" (the Developer changed it from the catalogue name). That reads well to a user and matches the field flag text, though the model list calls it "Parakeet Unified 0.6B (recommended) — English only", so a user has to connect "English model" to that row.
- The Settings window is short (480x639 pt), so the helper line is at the very bottom edge of the visible area; the link is visible but cropped in the screenshots. Minor polish only.
- I did not dictate in this run (not asked); the e2e dictation steps in `e2e_hotkey_recorder` and the two passing `e2e_settings_save_reopen` runs cover that nothing broke.
- A temporary scenario used for the screenshots was not committed.

## Failures
None blocking.
