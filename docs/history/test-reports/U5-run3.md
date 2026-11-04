# U5 test report, run 3

PR: #248 at 4c216c2 (the head since then, 7a953f9, adds only `docs/history/reviews/U5-round2.md` and `U5-round3.md`)
Result: PASS

Run 3 re-checks the regression from runs 1 and 2: after an Accessibility re-grant, a hold didn't dictate in the running app on this branch but did on `main`. On 4c216c2 it does. All runs below are in `dictation-vm-2`, in the foreground.

| Check | Result | Notes |
| ----- | ------ | ----- |
| `e2e_accessibility_revoked`, branch, run A | PASS | Tray `! Vocet needs Accessibility to hear your key.` + `Open Accessibility settings…`; after the re-grant `✓ Ready · Hold Right ⌥`; **a real hold dictates in the running app**; a hold also dictates after a relaunch |
| `e2e_accessibility_revoked`, branch, run B | PASS | Same, again |
| `e2e_accessibility_revoked`, `origin/main` (a1d128b) | PASS | Same hold-in-running-app and hold-after-relaunch passes, so main and this branch now behave alike |
| `e2e_hotkey_listener_recovers` | PASS | |
| `e2e_tray_menu_screenshots` | PASS | Includes Copy last dictation (disabled before, exact text after, nothing in the log, disabled again after relaunch), ready / no-key / recording in light and dark |
| Downloading menu (scratch scenario, not committed; the real local model download) | PASS | While downloading: `(off) ↓ Downloading Parakeet Unified 0.6B, 26%`, `(off) Copy last dictation`, Settings…, Troubleshoot…, Report a problem…, Resume setup…, Check for updates…, Quit Vocet. Tooltip `Vocet: downloading Parakeet Unified 0.6B`; tray title empty |
| Show log file from Troubleshoot (same scratch scenario) | PASS | `Troubleshoot…` from the tray, then `Show log file` pressed; Finder comes to the front (shown on the log folder in run 2's screenshot) |
| Hosted CI | PASS | CI and DMG succeeded on 4c216c2. The later head 7a953f9 is docs only and had no check run yet |

## Regression verdict
Gone. Both runs on the branch dictated after the re-grant without a relaunch, as `main` does; the tray shows Ready only when the hold works, and the scenario now asserts it. Run 1 had failed this twice on 8233701; I couldn't see the cause, only that it is no longer there on 4c216c2.

## Not re-run
The unit tests, licence and mutation checks from runs 1 and 2 (the mapping in `tray.rs` is unchanged in kind; CI runs the unit tests). The Codex "false Relaunch status" change was checked only through the scenario above (no Relaunch row after a restore). No truly light menu bar (dark wallpaper) and no crowded menu bar, as before.

## Failures
None.
