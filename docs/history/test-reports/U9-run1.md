# U9 test report, run 1

PR: #285 at 4a19a2e
Result: FAIL (incomplete: one required e2e scenario failed, and the manual checks for the owner's state were not run inside the time box)

VM: dictation-vm-2, reset before the scenarios. Screenshots (light and dark, from
the layout run, plus the no-permissions end state) are in
`/Users/gauri/vocet-screens/u9/tester-run1/`, together with the own-key scenario
described below.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Both routes go forward without Accessibility | `e2e_setup_no_permissions` re-run on this head; an own-key scenario I wrote (not committed) | PASS | Download route: 17 checks, Accessibility really off. Own-key route: Accessibility really off, a made-up key against the local stand-in provider, Test & Save shows "Connected", Continue moves to Permissions. I did not run the own-key scenario against a pre-fix build, so I can't say it fails without the fix. |
| 2 | No "can't listen" message, no `RightAlt` | both scenarios | PASS | The window's accessibility dump had neither string on step 2 or Permissions. Not checked live on Try it, Ready, Settings or the tray. |
| 3 | New e2e from a clean launch | `e2e_setup_no_permissions` | PASS | |
| 4 | Selected chip in every state | `e2e_onboarding_layout` (25 checks, light and dark) | PASS, harness only | No real click or hover on the English, Hindi or More chips in the app window. NOT TESTED. |
| 5 | Failed-paste panel reachable | `e2e_onboarding_layout` | PASS, harness only | Not reproduced in the real app with a two-key shortcut and a forced paste miss, at default size or 680x520. NOT TESTED. |
| 6 | Focus returns to the Try it box | none live | NOT TESTED | No real dictation without clicking the box, no app switch and back, no cancel. Only the Developer's jsdom and Safari checks exist. |
| 7 | Language search box | `e2e_onboarding_layout` | PASS, harness only | No real click, typing, arrow keys or Enter in setup or Settings. NOT TESTED. |

Other scenarios on this head: `e2e_onboarding_first_run` PASS,
`e2e_settings_save_reopen` PASS, `e2e_settings_key_provider_recovery` PASS.
`e2e_onboarding_five_steps` FAIL (below).

## Gate
Not re-run by me; the PR says `scripts/gate.sh` passed on this head. My fresh
worktree needed `npm ci` before the e2e build would work.

## Untested behaviour found
- The own-key route with no Accessibility has no e2e. I wrote
  `e2e_setup_own_key_no_permissions` (copy in `/Users/gauri/vocet-screens/u9/tester-run1/`);
  it passes on this head. I did not commit it because my brief was to commit
  the report only. The Lead or Developer can add it to `tests/e2e/scenarios/`
  and `ALL_SCENARIOS` in `run.sh`.
- Settings -> Dictation telling the user Accessibility is missing was not
  checked in a live run, only seen in the layout harness' "settings-language"
  screenshot.

## Failures
`e2e_onboarding_five_steps` FAILED on this head, in two separate runs of mine.
- First failing check: "the strip offers to resume". The scenario cancels the
  download and deletes the models folder, expecting "isn't downloaded yet".
  In the VM the download finished in about 7 s, and the delete raced the
  final file move. The app log has `model download failed ... couldn't write
  the model to disk: No such file or directory`, and the window showed
  "Download of Parakeet English stopped: Couldn't save the model to disk"
  instead. Every later check fails because the model never loads.
- My reading: a timing race in the scenario with a fast download, not a U9
  regression, because U9 doesn't touch the download's end. I did not build
  `main` to confirm it fails there as well, so this is unconfirmed. Re-run it
  or compare against main before merging.
