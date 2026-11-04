# U5 test report, run 2

PR: #248 at ee0a67f
Result: PASS

Round 2 re-check of run 1's FAIL. The status line now says "Relaunch Vocet" when Accessibility is granted but the hotkey isn't live, and never "Ready" in that state. One observation (below) for the Lead: after an Accessibility re-grant, a hold in the running app worked on `main` and does not on this branch; the PR replaces the in-app recovery with a relaunch prompt.

## Results

| Check | Result | Notes |
| ----- | ------ | ----- |
| Hosted CI on ee0a67f | PASS | Quality gate (Linux), Platform crate (macOS), DMG (macOS) all pass |
| `e2e_accessibility_revoked` (PR build) | PASS | Tray `! Vocet needs Accessibility to hear your key.` with `Open Accessibility settings…`; after re-grant the supervisor restores the listener, and the menu shows `! Relaunch Vocet to finish turning on Accessibility` with `Relaunch Vocet` as the fix (not Ready); Relaunch pressed from the tray; no new PermissionMissing; a real hold dictates after the relaunch |
| `e2e_accessibility_revoked` on `origin/main` (fe93f52) | PASS | The old scenario passed, including "a real hold dictates in the running app" after the re-grant. See observation |
| `e2e_tray_error_clears_on_next_recording` | PASS | Title empty, status `! No API key is saved.`, fix `Open Speech settings…`; error clears on the next recording |
| `e2e_tray_menu_screenshots` | PASS | Also asserts Copy last dictation (below) |
| `e2e_microphone_denied_then_recover` | PASS | Tray `! Vocet needs the microphone to hear you.`, fix `Open Microphone settings…`; fix row opens System Settings; after the grant the fix becomes `Relaunch Vocet`; relaunch records the mic |
| Downloading menu (`e2e_local_model` flow plus a scratch scenario, not committed) | PASS | Menu while downloading: `(off) ↓ Downloading Parakeet Unified 0.6B, 1%`, `(off) Copy last dictation`, Settings…, Troubleshoot…, Report a problem…, Resume setup…, Check for updates…, Quit Vocet. Tooltip `Vocet: downloading Parakeet Unified 0.6B`, title empty. `e2e_local_model` itself passes (download, 3 real local dictations) |
| Copy last dictation | PASS | Disabled before a dictation; enabled after one; pressed in the tray, the clipboard holds exactly the dictated text; the app log never holds the text; after quit and relaunch it is disabled again |
| Show log file from Troubleshoot | PASS | Tray `Troubleshoot…` opens it; pressing `Show log file` brings Finder to the front on `com.dictationapp.desktop` with the log file (`r2-show-log-file.png`) |
| Icons in light and dark menu bars | PASS (caveat) | `r2-bar-{normal,attention,recording}-{light,dark}.png`: three-bar mark, a dot variant for attention, and a recording variant; all render in the menu bar's text colour, and no tray title in any state |
| Mutation check of the fix-row mapping | PASS (run 1) | Not re-run: `tray.rs` mapping unchanged in round 2 besides the status |

## Gate
Hosted CI green (above). I did not re-run the gate locally; round 2 changed `tray.rs`, `tray_menu.rs`, `platform/src/insert/mod.rs` and e2e scripts, and CI covers them.

## Observation (not a blocker)
Run 1: after the Accessibility re-grant, the menu said Ready but a real hold didn't dictate in the running app (2 of 2 runs on 8233701). On `main` the same hold dictated (1 run). So the dead listener after a re-grant is new with this PR, not old behaviour. Round 2 fixes the lie (the menu now asks for a relaunch) and drops the scenario's "a real hold dictates in the running app" check; the scenario's comment attributes a dead tap to #50. The user can recover with the relaunch, but the app no longer recovers by itself as it did on `main`. Worth a `tech-debt` issue and a look at why the supervisor's restored listener doesn't hear keys here.

## Caveats
- The VM's wallpaper is dark, so the "light" menu bar renders with light text just like the dark one; both screens show the template icon in the bar's text colour, but a truly light bar (bright wallpaper) was not seen.
- The Downloading menu is evidenced by the listed items and tooltip (read through the accessibility tree); its screenshot didn't capture the open menu. The Terminal windows from earlier e2e steps pile up on the VM desktop in full-screen shots, so the menu screens here are cropped to the menu bar and menu.
- Crowded menu bar: not checked.
- Hotkey unavailable with Accessibility granted (not just the Accessibility-off case): not driven in the VM; covered by the `tray::` unit tests.

## Screenshots (`docs/history/test-reports/U5-screens/`)
`r2-bar-*.png` (menu bar icon states, light and dark), `r2-menu-ready-{light,dark}.png`, `r2-menu-bad-key-{light,dark}.png`, `r2-show-log-file.png`. Run 1's screens remain.
