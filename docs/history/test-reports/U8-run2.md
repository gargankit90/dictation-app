# U8 test report, run 2

PR: #269 at e077fb0
Result: FAIL (`e2e_onboarding_first_run` fails 2 of 2 runs; it passed 3 of 3 at 67795bd)

`dictation-vm-2`, `E2E_VM=dictation-vm-2 tests/e2e/run.sh`. Screenshots in `U8-run2/`.

## What changed in round 2, and how I checked it

| Change | Check | Result |
| ------ | ----- | ------ |
| Continue waits for the Welcome switch to persist | New scenario `e2e_onboarding_round2`: switch sharing off, press Continue at once, Back, five times. | PASS: each round, Speech opens and `settings.json` has `share_usage_events: false`. No install-ID file was created and the app log has no event send or drop. The app has no log line for a send (scripted launches send nothing and there is no build key), so "no send" rests on the saved setting, the missing install ID and the unit tests. |
| Speech choice persists; resume wording | `e2e_onboarding_round2`: pick "Use my own key", check the progress file has `speech_choice: own_key`, quit from the tray, relaunch. | PASS: relaunch logs `onboarding shown at launch step=Speech` and the own-key panel ("Continue unlocks after Test & Save succeeds") is shown. The engine-less own-key case has no download note, so there is nothing wrong to say. The download wording after a quit ("Continued from …") is in `e2e_onboarding_five_steps` and `3-resume-after-quit-*.png`. |
| Footer wraps for long text at 680x520 | `e2e_onboarding_layout` (real WebKit, pseudo-long text, 6 steps, light and dark) and the 680x520 shots | PASS: nothing clipped. Checked `2-speech-hindi-680x520-dark.png` and `3-permissions-680x520-light.png` by eye (Back and the primary button fully visible, strip above them). |
| Auto-resume bug | `e2e_onboarding_five_steps` (quit mid-download, resume) | PASS |

## VM scenarios (e077fb0)

| Scenario | Result |
| -------- | ------ |
| `e2e_onboarding_first_run` | **FAIL**, twice (see below) |
| `e2e_onboarding_five_steps` | PASS |
| `e2e_onboarding_layout` | PASS (all steps, light and dark) |
| `e2e_dmg_install` | PASS |
| `e2e_onboarding_screenshots` | PASS (light and dark) |
| `e2e_onboarding_round2` (new) | PASS |

## Failure: `e2e_onboarding_first_run`

Steps: own-key path; type the rejected key and press Test & Save (rejected, correctly); then `ax.js secret-set api-key <good key>`.
Expected: the field is emptied and holds the good key (18 characters).
Actual: `typed 18 characters but the field holds 34`. The 16-character rejected key is still in the field, the provider receives the bad key again (`request n=2 ... auth=bad`), "Connected" never shows, and every later step fails. Same place in both runs, so it is not random.
At 67795bd the same scenario passed 3 of 3. Between 67795bd and e077fb0 the scenario, `ax.js` and the helpers did not change; the app did (`SpeechStep.tsx` now calls `set_speech_choice` on each pick, `Onboarding.tsx`, `logic.ts`). So either the app now moves focus or re-renders the key field after a rejected test, or it changed how the field clears. I did not find which. A person who corrects a rejected key by select-all and retyping may hit it; worth a manual check.
Log: `U8-run2/first_run-fail.app.log` (no keys, no transcripts).

## Screenshots

`U8-run2/onboarding-<n>-<step>-<state>-{light,dark}.png`: each step, the download strip, paused, resume after quit, 680x520, try-it states, and `onboarding-layout-<step>-{light,dark}.png` from the layout page. They were taken by the scenario with the cua-driver window capture (the window alone, no Terminal windows). `screencapture -l` was not needed. Resized to 900 px for the repo.

## Not done
- A manual look at the key-field focus after a rejected test (above). Time box used up waiting for the VM lock behind other runs.
- Gate and coverage: not rerun.
