# K1 test report, run 3

PR: #231 at 4d8395a (head 286dfc4 adds only the Reviewer's round-3 note)
Result: PASS

Re-check of round 3: Save disabled while the recorder listens, the hotkey resumes only when the recorder ends, `e2e_hotkey_recorder` reordered.

| # | What | Test(s) | Result | Notes |
| - | ---- | ------- | ------ | ----- |
| 1 | `e2e_hotkey_recorder`, run twice | `tests/e2e/run.sh` on `dictation-vm` (both VMs were often busy; the run waited for a free one) | PASS, PASS | 1 passed, 0 failed each time. |
| 2 | `e2e_settings_save_reopen` | same | PASS | 2 passed, 0 failed in the first invocation with `e2e_hotkey_recorder`. |
| 3 | Save can't be used while recording | VM, real app (cua-vm) and a throwaway scenario; vitest `Settings.test.tsx` (Save `disabled` while listening) | PASS | While recording, the accessibility tree lists no actionable Save button (before and after recording it lists `Save [press]`). An AXPress on it through `ax.js` returned "pressed" but had no effect: the keycap stayed `Press a key…` and no settings file was written. I could not land a real mouse click on Save: it sits below the fold of the 480x639 Settings window, the VM's scroll tool sends keystrokes (which the recorder would take as keys), and fronting the window for a pixel drag cancelled recording on blur. So the "disabled" proof is the accessibility tree plus no effect plus the vitest assertion, not a mouse click. |
| 4 | Cancel, then the old key works | throwaway scenario | PASS | Cancel returned the keycap to `Right ⌥`, Save pressable again, and a real Right Option hold dictated `hello world` into TextEdit. |

## Gate
- `npx vitest run`: 10 files, 188 tests pass (was 187).
- Hosted CI on the PR head 286dfc4: `Quality gate (Linux)`, `Platform crate (macOS)` and `DMG (macOS)` all pass.

## Untested behaviour found
- A mouse click on a disabled Save (see #3): not done with a real pointer. The vitest assertion and the AX tree cover it.
- The throwaway scenario `e2e_k1_cancel` is not committed (the report is this run's only commit). Its steps: open Settings, press Record, press Save (no effect), still `Press a key…`, no settings file, press Cancel, keycap `Right ⌥`, Save pressable, close Settings, Right Option hold dictates.
- Known from run 2, unchanged and not K1: #239 (the first hold of a combination hotkey pastes nothing).

## Failures
None.
