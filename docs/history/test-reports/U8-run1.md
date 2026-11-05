# U8 test report, run 1

PR: #269 at 67795bd
Result: PASS (one evidence gap, see "Not done")

Run in `dictation-vm-2` with `E2E_VM=dictation-vm-2 tests/e2e/run.sh`. Screenshots in `U8-run1/` (dark mode, from the `e2e_onboarding_screenshots` run).

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Five steps, rail, footer, 740×560 and 680×520 with long Hindi labels, Back, reason line | `e2e_onboarding_five_steps`, `e2e_onboarding_screenshots`, `src/Onboarding.test.tsx` | PASS | Rail shows ✓ / current number / upcoming and "Download running" under Speech; Back and a disabled Continue with its reason line seen (`3-permissions-download-strip-dark`, `2-speech-hindi-680x520-dark`). |
| 2 | Events switch on by default; nothing sent before leaving Welcome, nothing if off | `crates/app/src/usage.rs` and `src-tauri/src/usage.rs` unit tests (fake port); `e2e_onboarding_first_run` checks switch is on | PASS | No VM scenario flips the switch off and reads the log; covered by unit tests only. No key or transcript in any app log. |
| 3 | Recommended model per language; Download and continue; strip with Cancel on later steps; own-key Test & Save | `e2e_onboarding_five_steps`, `e2e_onboarding_first_run`, screenshots | PASS | Strip "Downloading Parakeet English · 88 of 477 MB · Cancel" shown on Permissions. Bad key refused with Continue locked; good key accepted. |
| 4 | Permissions cards, denied, relaunch, live meter | `e2e_microphone_denied_then_recover`, screenshots | PASS | Denied card says "Turned off" with the exact path and the reason line; "Relaunch and resume" returns to Permissions; meter and input name shown once allowed. |
| 5 | Try it driven by real events; blocked while model downloads; failed paste shows words + Copy; skip | `e2e_onboarding_five_steps`, screenshots | PASS | Paste into TextEdit shows words with Copy and doesn't count; paste into the practice box counts. |
| 6 | Ready real states; Start dictating closes and removes Resume setup | `e2e_onboarding_first_run`, `e2e_onboarding_five_steps` | PASS | |
| 7 | Quit and relaunch resumes at the step; partial download | `e2e_onboarding_five_steps`, `e2e_onboarding_first_run`, `3-resume-after-quit-dark` | PASS | Quit mid-download comes back at Permissions with the download picked up. |
| 8 | Existing tests migrated; new VM scenario | the scenarios above | PASS | |
| 9 | [manual] light/dark paths | `e2e_onboarding_screenshots` (each step, light and dark) | PASS (partial) | English/local, Hindi, own-key panel, resume, paste-missed captured. Mic denied captured in light only through the scenario's desktop shot (covered by Terminal windows); see "Not done". |

## VM scenario results (all on 67795bd)

| Scenario | Result |
| -------- | ------ |
| `e2e_onboarding_first_run` ×3 | PASS, PASS, PASS (no flaky secret-set read in 3 runs) |
| `e2e_onboarding_five_steps` | PASS (42 checks) |
| `e2e_dmg_install` | PASS |
| `e2e_launch_at_login` | PASS |
| `e2e_update_keeps_permissions` | PASS |
| `e2e_update_from_dictation_app` | PASS |
| `e2e_microphone_denied_then_recover` | PASS |
| `e2e_onboarding_screenshots` | PASS |

## Gate
Not rerun by the Tester; CI and the Developer's gate cover it.

## Not done
- **Dark-mode screenshot of the microphone-denied state:** not captured. `screencapture -l <window id>` run from the VM fails with "could not create image from window" (a throwaway scenario copy; window id 36/37 from cua-driver; also over ssh). The fallback (cua-driver window capture after closing the Terminal windows) was written but `dictation-vm-2` was then held by another worker's run for 20+ minutes, past the time box. The denied state itself is verified by assertions in `e2e_microphone_denied_then_recover`. Suggest a follow-up scenario that sets dark mode and captures the denied card with the cua-driver window capture.
- Manual driving through `cua-vm-2` and the events-off-on-Welcome check in the log were not done; the latter is covered by unit tests only.
- Hindi path and own-key path (bad then good key) are covered by the scenarios and screenshots, not by manual driving.

## Untested behaviour found
No VM scenario for "events off on Welcome sends nothing" (unit-tested only).

## Failures
None.
