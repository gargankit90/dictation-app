# U11 test report, run 2

PR: #300 at bd8c9f7
Result: PASS (with the gaps listed under "Not tested")

VM dictation-vm-2, reset per run. Local stand-in provider and made-up keys only. "By hand" below means I drove the real app
through the real accessibility and key helpers with scratch scenarios of my own (not committed); I did not click by eye.

## Scenarios on bd8c9f7
| Scenario | Result |
| -------- | ------ |
| e2e_onboarding_round2 | PASS |
| e2e_setup_own_key_no_permissions | PASS |
| e2e_setup_no_permissions | PASS |
| e2e_onboarding_first_run | PASS |
| e2e_pill_states | PASS |
| e2e_setup_redesign_screens | PASS (frames and Reduce Motion pictures, below) |

## Item by item
1. Connect your key (scratch scenario, real app, local provider): no key blocks Continue; after Test & Save "Key saved"; back to Speech and Continue again shows "Key saved", "In your macOS Keychain. It's never shown again.", Replace, no key field, no "Remove key". With the provider's gate closed, Continue (a slow saved-key test) then Back after 2 s: Speech stays, and 5 s after releasing the reply setup is still on Speech, not Permissions (Codex's bug does not reproduce). A saved key the provider now rejects: Continue shows "Groq rejected this API key…" and stays on Connect your key. Replace reveals the key field and Test & Save. Provider change (popup) was not driven: not tested.
2. Try it (scratch scenario, own-key route): no "Reset to default"; "Hold the key, say a sentence, let go."; Change key shows "Press a key…" with no "Use default"; Cancel closes it. A dictation without clicking the box ran to Idle and the log shows the paste sent; "hello world" is in the app's UI tree afterwards. My own check that reads the box's value failed twice (it reads the wrong element), so I do not count "lands in the box" as cleanly proven by an assertion. Recording a new key by posting Right Command did not register in my harness: not verified. Focus returning after each is only implied by the dictation landing without a click; not asserted.
3. Welcome: e2e_setup_redesign_screens gives 14 frames per theme and size and Reduce Motion pictures; Reduce Motion shows the static final frame with all three captions emphasised and the words already in the box, and it stays that way later (the "later" pictures). The keycap sits in the same place across frames. "No timers after leaving Welcome" could not be checked from outside; not tested.
4. Fresh install to Ready on the download route and quit/resume: e2e_onboarding_first_run (own key), e2e_setup_no_permissions and e2e_setup_own_key_no_permissions (no permissions), e2e_onboarding_five_steps ran in run 1 on the earlier head; it was not rerun on bd8c9f7 (not requested in the list). Resume after quit at each step: Speech (round2) and Permissions mid-download (in my tray runs, quit and relaunch six times returned to the download strip). Welcome, Try it and Ready resume not tested.
5. Ready's "On this Mac · English · ✓ Ready" is asserted by five_steps (run 1 head). The Gujarati card with the key route selected is in the screenshots (setup-speech-uncovered): "Gujarati needs your own key. Vocet's on-device models don't cover it yet." with the key card selected. Ready for the Gujarati route was not driven.
6. Empty tray menu: see below.
7. Scenarios: all five requested passed, plus e2e_setup_redesign_screens.

## Item 6: the empty tray menu
Not reproduced in 52 attempts, and the evidence points at the harness.
- Scratch runs: (a) 40 openings of the tray menu while a model download went from 3% to about 70% (the app rebuilds the tray menu about once a second, per the log): 40 of 40 listed the menu. (b) A fast download with the tray Quit clicked at 6, 8, 9, 10, 11 and 12 s, covering the finish and the engine load: 6 of 6 quit. (c) Six quit/relaunch cycles mid-download at a slower rate: 6 of 6.
- The failing run 1 log has no warnings ("could not update the tray" appears in no log I have). The menu list is normal around the failure (`status,-,copy-last,…,quit` at 01:15:07 to :17) and Quit was clicked as the download hit 93% to 100%.
- The harness opens a new Terminal window in the VM for every job, and the app's menu closes when another app takes focus (ax.js's own comment says so). The failing screenshot shows a pile of ~10 Terminal windows over the app. A menu that closes while it is being read gives the "no menu item starting with Quit in []" message.
- Real-app risk I cannot rule out: the app calls `set_menu` with a new menu every time the status text changes (about 1 per second during a download), so a person who has the menu open while that happens could in theory see it replaced. I did not see this; I did not test it with a person's mouse. A user-visible check would need the menu held open for several seconds during a download.
Verdict: very likely the harness, not proven; no app bug seen.

## Not tested
- Provider change by hand (popup), Replace followed by a successful new key.
- Reduce Motion by hand (the scenario sets it through System Settings; pictures only).
- No timers leak after leaving Welcome.
- Try it: new key recorded; focus returning asserted; box value asserted.
- Fresh install to Ready on bd8c9f7 on the download route (five_steps not rerun), Ready for Gujarati, resume at Welcome, Try it and Ready.
- The 40-minute box ended before these.

## Screenshots
/Users/gauri/vocet-screens/u11/run2/ (setup-*.png, 14 Welcome frames per theme and size, Reduce Motion, key none/saved/replace, uncovered language, Try it, recorder) and harness-terminal-windows-over-app.png.
