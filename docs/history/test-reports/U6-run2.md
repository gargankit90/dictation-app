# U6 test report, run 2

PR: #262 at 32027b3 (round 4; later head ea82f3f only adds `docs/history/reviews/U6-round4.md`)
Result: PASS (with one test fix pushed by the Tester and one minor finding, below)

## What I ran

| Check | Result | Notes |
| ----- | ------ | ----- |
| Hosted CI on ea82f3f | pass | Quality gate (Linux) 9m26s, Platform crate (macOS), DMG (macOS) all green. |
| vitest | 235/235 pass | 12 files. |
| `cargo nextest run --workspace` | 1326 passed, 2 skipped, 0 failed | |
| `e2e_settings_save_reopen` (VM) | PASS | |
| `e2e_tray_menu_screenshots` (VM) | PASS | U5 menu with Settings. |
| `e2e_settings_key_provider_recovery` (VM) | **FAIL on the pushed head, PASS after my fix** | See "Failure found" below. |
| Help → Show log file | PASS | Finder opened the app's log folder (`com.dictationapp.desktop`, holding `dictation-app.log…`): `run2-help-show-log-file-finder-opened.jpg`, `run2-menu-and-finder-log-folder.jpg`. |
| Help → Run setup again | PASS | The "Set up Vocet" onboarding opened (at step 3 "Add your API key", because setup was never finished on this VM). |
| Menu bar with Settings (U5) | PASS | Menu shows "! No API key saved.", Open Speech settings…, Settings… ⌘,, Troubleshoot…, Report a problem…, Resume setup…. "Open Speech settings…" opens Settings on Speech. "Report a problem…" opens Troubleshoot (checks list and "Report a problem" form). Troubleshoot's "Open Settings" returns to the sections. |
| "Needs a key" until a key is saved | PASS | Own-key row shows "! Needs a key" with "No key saved yet." and stays so after a rejected key (light and dark: `run2-speech-light-needs-a-key.jpg`, `run2-speech-dark-paused-243mb.jpg`). |
| English and Multilingual titles, "audio stays on this Mac" | PASS | Rows read "English / Parakeet Unified 0.6B · English only · works offline · audio stays on this Mac" and "Multilingual / Qwen3-ASR 0.6B · 30 languages, including English and Hindi · works offline · audio stays on this Mac". Window title is "Vocet Settings". |
| Key flow: Test & Save, switch to General and back during the test | PASS (limited) | A bad key typed and Test & Save pressed, then General and back to Speech at once: the row still says "Needs a key", "No key saved yet.", the field is empty, no stale "saved" or error appeared. The VM's network was down for part of the run, so the test itself was not slow; the race is covered by the new vitest/Rust tests (all pass). |
| Remove key | NOT RUN by hand | Needs a saved key, which needs a real provider key. The scenario saved a key with a local provider and passes; Rust `criterion_5_remove_key_*` pass. |
| Cancel a model download, then Resume | PASS | Download then Cancel: chip "Download paused, 0 of 477 MB" with Resume and "…" (`run2-speech-light-download-paused-resume.jpg`). Resume: "↓ 17 of 477 MB" and progress. After the network dropped it showed "Paused: no connection" with Retry and the inline text "The download failed. Check your internet connection and try again; it continues where it stopped." (`run2-speech-light-paused-no-connection.jpg`). After quitting and relaunching the app it showed "Download paused, 243 of 477 MB" with Resume (`run2-speech-dark-paused-243mb.jpg`). I did not finish the 477 MB download this run (the VM's link was very slow, about 0.5 MB/s, then dropped); run 1 covered 477 MB to "✓ Active" on the earlier build, and `e2e_local_model` is unchanged by round 2 to 4 except the labels, which I didn't re-run (not on the list). |
| Light and dark screenshots of Speech and Help | done | `U6-screens/run2-*.jpg`. Light: `run2-speech-light-*.jpg`, `run2-help-light.jpg`. Dark: `run2-speech-dark-paused-243mb.jpg`, `run2-help-dark.jpg`. Help shows Troubleshoot…, Report a problem…, Log file / Show log file, Setup / Run setup again. No e2e Terminal windows were open. |

## Failure found (and fixed in tests)
`e2e_settings_key_provider_recovery` failed on 32027b3 (`ea82f3f`): it still drove the pre-U6 Settings (a "Key saved" caption, a field named `api-key`, a "Test key" button and "Save"). The U6 rewrite of this scenario (present at a46d812: Test & Save, "✓ Connected · saved in Keychain", a wrong key rejected and not saved) was lost when U5's version of the file won a merge into the U6 branch. Result before the fix: 1 failed (many FAIL lines, from "it says a key is saved" on). I restored the a46d812 version and kept U5's tray assertion (`env_tray_fix "Open Speech settings…" tray_shows "! Your API key was rejected." "Vocet: Your API key was rejected."`). With that, the scenario passes in the VM (1 passed, 0 failed). The change is in `tests/e2e/scenarios/e2e_settings_key_provider_recovery.sh` only. Criterion 8 holds again with it.

## Minor findings (not blocking)
- The shared window can keep showing Troubleshoot or onboarding when Settings is asked for from the menu bar: with Troubleshoot showing, the tray's "Settings…" leaves Troubleshoot on screen (title "Vocet Settings", content Troubleshoot; "Open Settings" is the way back). Once, on a fresh launch with onboarding showing, "Open Speech settings…" did the same (title "Vocet Settings", onboarding content); the first time on the same build it worked. Closing the window and choosing Settings… again shows Settings. Issue: #263 (`tech-debt`).
- "Run setup again" resumes onboarding at the saved step (3 of 4) rather than step 1 when setup was never finished. Fine if intended; the Help caption says "Go through the first-run steps again".
- Not seen this run (no real key, no failing Keychain): Remove key by hand, the Keychain-failure text.

## Gate
CI green on ea82f3f. Local: vitest 235/235, nextest 1326 passed. VM: save_reopen PASS, tray_menu_screenshots PASS, key_provider_recovery PASS after the fix.
