# SIMP1-A test report, run 1

PR: #345 at e85c582 (plus the two test fixes below)
Result: PASS

The spec is `docs/history/milestones/SIMP1.md`. This run checks what the Developer could not run: the whole e2e suite in a VM.

## E2E (dictation-vm-2, full suite, `--exclude e2e_real_openrouter --exclude e2e_real_mic`)

First run on e85c582: **40 passed, 2 failed, 1 skipped** (`e2e_release_build_smoke`, optional). The two failures were stale scenarios, not app faults (the app behaves as designed in both):

| Scenario | Why it failed | Fix (committed here, `tests/e2e` only) |
| --- | --- | --- |
| `e2e_onboarding_five_steps` | OpenRouter is the default Speech choice, so the first screen has "Continue", not "Download and continue" | pick "Use this Mac" before the first press |
| `e2e_dmg_install` | Settings → Speech now hides the key field behind "Paste a key instead", and the success text is "✓ Connected to OpenRouter" (stale on main too: the scenario was not touched by this PR) | press "Paste a key instead"; expect the current text |

Re-run after the fixes: `e2e_onboarding_five_steps` PASS (including the new restart path: back at Ready, Back x3 to Speech, Use this Mac, Download and continue, resume with the server asked for the rest only) and `e2e_dmg_install` PASS.

## What the owner asked to confirm

| Area | Scenario(s) | Result |
| --- | --- | --- |
| Onboarding, state in memory; Welcome counted as passed after settings exist or Relaunch | `e2e_onboarding_first_run`, `e2e_onboarding_five_steps`, `e2e_setup_no_permissions`, `e2e_microphone_denied_then_recover` (Relaunch) | PASS |
| Release, DMG, update scenarios (no ALREADY_SET_UP) | `e2e_dmg_install`, `e2e_update_keeps_permissions`, `e2e_update_privileged_install` | PASS (`e2e_release_build_smoke` optional, skipped) |
| Plain Try-it practice box and Done | `e2e_onboarding_five_steps` | PASS |
| Settings (no Test microphone, no language picker), save and reopen | `e2e_settings_save_reopen`, `e2e_settings_key_provider_recovery`, `e2e_connect_openrouter` | PASS |
| Tray without percentages | `e2e_onboarding_five_steps` (strip says "Downloading English…"), `e2e_tray_error_clears_on_next_recording` | PASS |
| History | `e2e_history` | PASS |
| Launch at login | `e2e_launch_at_login` | PASS |
| Paste-key fallback | `e2e_secure_input_blocks_paste`, `e2e_layout_paste`, `e2e_unicode_multiline_selection_paste` | PASS |
| Local model | `e2e_local_model` | PASS |

Not driven by a scenario in this run, so not confirmed by it: the Help window (Troubleshoot and Report), Sentry reporting, the Welcome demo and the retry-with-jitter path (Retry-After and jitter are covered by unit tests in CI). The e2e suite has no scenario for them; I did not add one in the time box. The gate and CI cover the Rust and TypeScript tests for them.

## Gate
`scripts/gate.sh` passed (full mode) on e85c582 and again with the two test fixes: formatting, spelling, lint, unit tests and the shell test suites (key handling 37, vm lock 184, runner 118, ci-run 32).

## Untested behaviour found
- No e2e scenario covers Help, Sentry or the Welcome demo. Left as is (owner keeps them; unit tests exist).

## Failures
None open. The two above are fixed in the test scripts.
