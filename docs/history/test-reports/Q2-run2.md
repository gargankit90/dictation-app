# MQ2 test report, run 2

PR: #336 at 013f0ab plus the Tester's test-only commits (below)
Result: PASS

Run 1 (`Q2-run1.md`) failed on one stale e2e scenario. Owner decision: updating from 0.1.0 need not keep the Groq key or settings (no users, no migrations).

## Changes by the Tester (tests only)
- `e2e_update_from_dictation_app`: removed the 4 checks that asserted the Groq key, provider or Keychain notice survive the update (the notice, "engine has the same settings" twice, "key is in the keychain" twice, and the DMG-step key store) plus the three helpers they used. The update itself, the signature, one-copy, settings-file-unchanged, login item, permissions and updater checks stay.
- `e2e_update_to_new_certificate`: same class; removed "0.1.0 reads the key it saved", "the key is still in the login keychain" and "the new build reads the key". Settings and history byte-for-byte checks stay.
Neither scenario became pointless, so neither was deleted.

## Results
| Check | Result | Notes |
| ----- | ------ | ----- |
| Criteria 1–4 (unit, pipeline, client tests, grep) | PASS | Re-run in the gate at 013f0ab (includes db42df9, "paste the raw text for a cut-off reply"). |
| `scripts/gate.sh` (full) | PASS | An earlier attempt failed 2 `vm lock` INT checks only because the gate was started as a shell `&` job (SIGINT ignored); re-run normally it passes. |
| `e2e_update_from_dictation_app` | PASS | after the edit |
| `e2e_update_to_new_certificate` | PASS | after the edit (first run failed on the 3 removed checks) |
| `e2e_onboarding_screenshots`, `e2e_quit_right_after_paste`, `e2e_setup_no_permissions`, `e2e_setup_own_key_no_permissions`, `e2e_setup_redesign_screens`, `e2e_tray_menu_screenshots`, `e2e_update_privileged_install` | PASS | |
| `e2e_release_build_smoke` | SKIP (optional) | |
| `e2e_real_openrouter` | Not run | The Lead's (criterion 6). |

All other scenarios passed in run 1 (flaky `e2e_onboarding_round2` and `e2e_launch_at_login` passed on rerun; `e2e_unicode_multiline_selection_paste` fixed in run 1). The Lead's `e2e-main.sh` held dictation-vm-2, so run 2's scenarios ran in dictation-vm (the coordinator approved). These scenarios were run on the build of the tree at 013f0ab.

## Untested behaviour found
None new. Per-crate coverage figures are still not printed by the gate.

## Failures
None.
