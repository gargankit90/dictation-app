# U1 test report, run 1

PR: #250 at 43dbf3a
Result: **FAIL** (one required e2e scenario, `e2e_onboarding_first_run`, is stale and fails; every product behaviour checked is correct)

VM: `dictation-vm-2`, build of the PR head (`Vocet.app`), installed by the e2e runner over the golden image's `dictation-app.app` (the runner removes the old copy and registers Vocet.app). Evidence under `U1-screens/`.

| # | Criterion | Check | Result | Notes |
| - | --------- | ----- | ------ | ----- |
| 1 | Vocet in every visible string | `cargo test -p dictation-app --test vocet_name` (3 pass); built bundle: Info.plist `CFBundleName`/`DisplayName` = Vocet, mic string says Vocet; `strings` on the binary and `grep` on `dist/`; live windows and tray in the VM | PASS | Remaining "dictation-app": executable name, log prefix, crate doc strings, a console log line in `overlay.html` and its hidden `<title>`, update URL. None is shown to a user. Window titles seen: "Set up Vocet", "Vocet Settings"; tray "Quit Vocet" (e2e log), tooltip "Vocet". Minor: the tray item and heading still read "Set up dictation…" (describes the feature; no "-app"). Troubleshoot is a page inside "Vocet Settings", not a window titled "Vocet Troubleshoot". |
| 2 | Identifiers unchanged; old settings and key load | Bundle id `com.dictationapp.desktop` in the built plist; app-data folder and settings path used by the scenarios unchanged; `saved_settings_and_key_still_load_after_the_rename` passes; log is `dictation-app.log` | PASS | Keychain names covered by the same test. |
| 3 | `.app` rename: scripts and e2e updated | `e2e_dmg_install` PASS (DMG built from Vocet.app); CI "DMG (macOS)" green; the other scenarios found Vocet.app | PASS | |
| 4 | Pill error: one sentence, no code | VM `u1-7`: overlay reads "Your API key was rejected. Open Settings → Transcription to fix it." No code. `e2e_microphone_denied_then_recover` PASS | PASS | |
| 5 | Sentences name the fix; tray uses the sentence | `every_user_facing_sentence_names_a_fix_and_no_code` passes. Tray after a bad key: title `⚠ Dictation failed`, tooltip `Vocet: Your API key was rejected. Open Settings → Transcription to fix it.` | PASS (with the Developer's disclosed deviation) | The menu-bar **title** is the short "Dictation failed", not the sentence (#151); the tooltip carries the sentence. No code in either. |
| 6 | Try it succeeds only on a real dictation; Skip | `u1-3`/`u1-4`: typing `typedbyhand123` into the practice box and waiting 8 s: no "It works"; hint "Nothing yet. This confirms only after Vocet records, transcribes and pastes your dictation."; Finish dimmed. `u1-5`: Skip for now, then Finish reads "Finish (test skipped)". `u1-6`: after a real dictation (e2e helper): "It works: Vocet recorded, transcribed and pasted your dictation." | PASS | vitest 195/195. |
| 7 | Failure hint shows the error sentence inline | vitest inline-error test. Not driven in the VM. | PASS (unit only) | |
| 8 | Troubleshoot hides Test connection in local mode | Settings with `transcription: local` and no model: `u1-8` and `u1-troubleshoot.ax.txt`: no "Test connection"; readiness line "Model on this Mac. Not downloaded: get the model in Settings."; the row's "(model_missing)" code stays in Troubleshoot, as criterion 4 allows. | PASS | Groq/OpenAI still show the button: vitest. |
| 9 | VM: Vocet, no codes | `u1-1` onboarding opens "Set up Vocet"; the mic prompt reads "Vocet would like to access the microphone." (Developer's `1-mic-permission-prompt-says-Vocet.png`; my `e2e_microphone_denied_then_recover` run also saw the prompt and PASSED); bad key `u1-7`; "Quit Vocet" in the tray. Grants kept: after the install over the old app, Troubleshoot shows Microphone and Accessibility "Allowed" (`u1-8`) and onboarding opened at the key step. | PASS | |

## E2E (dictation-vm-2)

| Scenario | Result |
| -------- | ------ |
| e2e_microphone_denied_then_recover | PASS (the Developer's untested lib.sh fix works) |
| e2e_accessibility_revoked | PASS |
| e2e_error_bad_key | PASS |
| e2e_dmg_install | PASS |
| e2e_onboarding_first_run | **FAIL** |
| e2e_u1_visual (my throw-away scenario, not committed) | PASS on run 3; runs 1 and 2 failed only because `ax.js field-set` types lowercase letters and digits only (my text) |

## Gate

CI on 43dbf3a: Quality gate (Linux) pass, Platform crate (macOS) pass, DMG (macOS) pass. I ran no full gate locally: `npx vitest run` 195/195, `cargo test -p dictation-core diagnostics` and `-p dictation-app --test vocet_name` pass.

## Untested behaviour found

- A real failing dictation showing the inline error sentence in Try it (criterion 7): unit-tested only.
- An upgrade where the old install is `dictation-app.app` outside the runner (the runner deletes the old copy): the Developer already lists this as unverified.

## Failures

**e2e_onboarding_first_run** (`tests/e2e/scenarios/e2e_onboarding_first_run.sh`) still expects the old copy and button:

1. Line 66 waits for `"It works."` (with the full stop); the new copy is "It works: Vocet recorded, transcribed and pasted your dictation." Result: `"It works." not within 5s`.
2. Line 68 presses `Done`; the button is now `Finish` (or `Finish (test skipped)` after a skip). Result: `no button "Done"`.
3. So the window never hides and `onboarding finished` is never logged (the next two checks).

The app is right; the scenario needs `It works` and `Finish`. The Developer's PR notes don't list this scenario among the passing ones.
Reproduce: `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_onboarding_first_run`; the UI tree is in the run's `e2e_onboarding_first_run.ax.txt`.

## Screenshots

`U1-screens/u1-1-onboarding-key-step.png`, `u1-2-onboarding-bad-key.png`, `u1-3-try-it-before-typing.png`, `u1-4-try-it-after-typing-no-success.png`, `u1-5-finish-test-skipped.png`, `u1-6-try-it-after-real-dictation.png`, `u1-7-bad-key-overlay-and-menu-bar.png`, `u1-8-troubleshoot-local-mode.png`.
