# U2 test report, run 1

PR: #249 at 046fd1b
Result: FAIL

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Picker replaces text field; names, no ISO code | `LanguagePicker.test.tsx`, `Settings.test.tsx` | PASS | 201 vitest tests pass; `tsc --noEmit` clean. |
| 2 | Same stored values; `de` loads as "German (Deutsch)"; unsupported flagged and kept | `Settings.test.tsx:179-191`, `LocalModels.test.tsx:273-290` | PASS | `null` shows "Detect automatically"; `de` shows "German (Deutsch)"; `yue` under Groq shows "Cantonese, not supported by Groq", value kept. Mutation check below. |
| 3 | On-device list filtered by model; helper line and link; re-filter on switch | `LocalModels.test.tsx:240-290` | PASS | Helper line uses the catalogue name "Parakeet Unified 0.6B". |
| 4 | Type-to-search, keyboard, accessible names | `LanguagePicker.test.tsx` | PASS | "hin" finds Hindi; arrows/Enter/Esc covered. |
| 5 | HotkeyRecorder keeps K1 behaviour; DS1 keycaps | K1 vitest tests; `e2e_hotkey_recorder` | PASS | e2e_hotkey_recorder PASS in dictation-vm-2 (all checks). A first attempt failed while the VM was shared with other runs (screenshot cluttered with other jobs' Terminal windows); the clean rerun passed. |
| 6 | [manual] VM, light and dark, Hindi helper line, keycaps | none | NOT DONE | The Tester VM lock was held by other runs for most of my 50-minute box; no screenshots taken. The Developer's screenshots are in `docs/history/screenshots/u2/`. |

## Mutation check
Changed `onChange(option.value)` to `onChange(option.label)` in `src/LanguagePicker.tsx:74` (store the name, not the code). 18 vitest tests failed (LanguagePicker search/keyboard/Detect-automatically, Settings save, Onboarding). Restored; 201 pass again.

## Gate
- vitest: 201/201 pass. tsc: clean.
- Hosted CI: all three checks (Quality gate Linux, Platform crate macOS, DMG) were still pending when I looked; see the PR checks for the final state.
- Rust coverage not re-run (the diff touches only `src-tauri` catalogue fields and tests).

## Failures
**F1 (major): `e2e_settings_save_reopen` fails; it was not updated for the picker.**
Run: `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_settings_save_reopen` on 046fd1b gives FAIL.
Cause (from the saved UI tree and the scenario): Language is now an `AXComboBox` with value "Detect automatically". `tests/e2e/scenarios/e2e_settings_save_reopen.sh:98` expects `field_shows language ""`, line 99 types `de` into it with `field-set`, and line 113 expects the field to show `de` after reopen (now "German (Deutsch)"). The scenario only passes with the old text field. The post-merge `scripts/e2e-main.sh` would fail on it. The scenario needs updating (line 98 -> "Detect automatically", line 99 pick German through the combobox, line 113 -> "German (Deutsch)"). I did not edit it; say if you want me to push the scenario fix.

## Other notes
- (4) Helper line: "Parakeet Unified 0.6B understands English only. For Hindi and other languages, choose the multilingual model." The catalogue name is what the model list shows, so it is consistent, but the name is technical and does not say "English"; the spec's "Parakeet English" reads better to a user. Minor.
- Not verified in the real app: switching to the multilingual model clears the flag (vitest only), and a dictation after changing language.
