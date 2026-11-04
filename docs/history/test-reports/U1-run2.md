# U1 test report, run 2

PR: #250 at 7157daaa70 (the head moved from 5a0ef88 while I tested; I rebuilt and re-ran at 7157daaa70, which adds the "Keychain sentence is true" text and the Troubleshoot "failed relaunch is shown" change)
Result: **FAIL** (one required e2e scenario, `e2e_update_from_dictation_app`, depends on run order and fails in a full run; the product behaviour is correct)

VM: `dictation-vm` (the lock for `dictation-vm-2` was held for over an hour by the Lead's post-merge run; `dictation-vm`'s lock was free and I took it through the runner). Build: `Vocet.app` from 7157daaa70.

| Check | Result | Evidence |
| ----- | ------ | -------- |
| `e2e_onboarding_first_run` (run 1's stale scenario) | PASS | round 2 fixed the copy and the `Finish` button |
| `e2e_microphone_denied_then_recover` | PASS | overlay and tooltip read "Vocet can't use the microphone. Allow it in System Settings → Privacy & Security → Microphone, then relaunch Vocet." |
| `e2e_error_bad_key` | PASS | |
| `e2e_update_from_dictation_app` | **FAIL** when run after another scenario, PASS alone (see Failures) | alone: 2 of 2 PASS (04:12, 04:18); after `e2e_onboarding_first_run`: 2 of 2 FAIL (run 1 on 5a0ef88, run 2 on 7157daaa70) |
| Mic sentence says to relaunch | PASS | above, and the Troubleshoot "Microphone" row shows the same sentence (`u1r2-3`) |
| Troubleshoot "Relaunch Vocet" works | PASS | my scenario `e2e_u1_round2` (not committed): mic denied, Troubleshoot shows "Relaunch Vocet" on the Microphone row, pressing it starts the app again (`starting` logged twice), one app running |
| Keychain sentence on the first launch after updating from dictation-app | PASS | `u1r2-7` (via the in-app update, `e2e_update_from_dictation_app` alone) and `u1r2-1` (old-style install: settings file, no remembered name): "After this update, macOS may ask once whether Vocet can use the key saved in your Keychain. Choose Always Allow…" |
| NOT on the next launch | PASS | `u1r2-2`: after quitting and relaunching, Settings has no notice (also asserted by `ax wait-text` finding nothing in 6 s) |
| Onboarding window says Vocet | PASS | `u1r2-5` ("Set up Vocet") |
| Menu with "Quit Vocet" | not captured as a picture | the open status-item menu does not survive the screenshot path; the AX listing in the mic scenario's log shows the items `Settings…, Set up dictation…, Troubleshoot…, Check for updates…, Reveal log, Quit Vocet` |
| Hosted CI | not green yet | see Gate |

## Gate
When I checked, `gh pr checks 250` reported no checks on the branch for 7157daaa70/5a0ef88 (the earlier head ec05519 was green: Quality gate, Platform crate, DMG). I did not run a full local gate. See the PR for CI once it starts.

## Failures
**`e2e_update_from_dictation_app` fails whenever another scenario has launched Vocet before it** (every full run, including the Lead's post-merge e2e): check "the notice is there" (`wait-text "Choose Always Allow"`) times out.
Cause (confirmed by experiment): the app remembers its name in `last-run-app-name` in the config folder; a launch of Vocet by an earlier scenario (e.g. `e2e_onboarding_first_run`) writes it, and no scenario or `fresh_app_state` removes it, so the "old build" install looks like a same-name launch and the notice (correctly) does not show. The pre-rename build never writes that file, so a real user is unaffected. Adding `rm -f ".../com.dictationapp.desktop/last-run-app-name"` to the scenario's setup (before `upd_serve good`, around line 297) made `e2e_onboarding_first_run` + `e2e_update_from_dictation_app` pass together (04:35, local uncommitted edit, reverted).
Reproduce: `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_onboarding_first_run e2e_update_from_dictation_app`.

## Other observations
- macOS shows "Login Item Added: "dictation-app" will open automatically when you log in" after the update path (seen in `u1r2-3`): the login item keeps the old name in System Settings after an update from dictation-app. The Developer's evidence note says registration is left as is; flagging it because the spec asks for no "dictation-app" in user-visible text. Minor; the OS builds that text from the stored name.
- The e2e Terminal windows cover the VM desktop; I hid Terminal before each screenshot.

## Screenshots
`U1-screens/u1r2-1-settings-keychain-notice-first-launch.png`, `u1r2-2-settings-no-notice-second-launch.png`, `u1r2-3-troubleshoot-mic-relaunch-button.png`, `u1r2-5-onboarding-window.png`, `u1r2-7-update-path-keychain-notice.png`.
