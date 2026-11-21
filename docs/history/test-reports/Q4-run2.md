# Q4 test report, run 2

PR: #337 at 5ac2ce7 (round 4)
Result: FAIL (four e2e scenarios fail on Q4's changes or still-stale expectations; the rest passes)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | OrukQwen only, verified hash/size | `dictation-core local_models` (7) | PASS | |
| 2 | Local cleaner: cleaned text, timeout/error → raw, cancel doesn't block, restart | `dictation-transcribe llama` (18), `dictation-app cleanup` (12) | PASS | one `llama` run had 2 failures under load (the gate and a solo rerun passed all 18): timing flake |
| 3 | Parakeet / Qwen3-ASR gone, unknown id loads | `local_models` tests; grep (run 1) | PASS | |
| 4 | Gate | `scripts/gate.sh` (full) | PASS | first try, no flaky test this time |
| — | Onboarding speech_choice bug (run 1, finding 2) | `e2e_onboarding_round2` | NOT VERIFIED | the scenario fails earlier, on a stale label (below) |
| — | Hindi stale expectation in `e2e_onboarding_screenshots` | same scenario, in the full run | PASS | |
| — | Single download row | not covered by e2e (needs Gemma in the VM) | NOT TESTED | unit tests only |

## E2E
Full run in dictation-vm-2 (stand-in model server with the OrukQwen file; `e2e_local_model`, `e2e_real_openrouter`, `e2e_real_mic` excluded): **47 passed, 7 failed, 2 skipped** (the optional `e2e_update_keychain_control` and one more optional). This run includes the scenarios run 1 did not reach (`e2e_accessibility_revoked` and after: PASS).
Rerun of the 7 failures once: `e2e_settings_key_provider_recovery` and `e2e_password_field_not_read` PASS (timing flakes). Five fail twice:

1. `e2e_onboarding_round2`: `no radio button "Recommended for your language"` (`e2e_onboarding_round2.sh:26`). Q4 relabelled the on-device card "On this Mac only" (`SpeechStep.tsx:349`); the scenario was not updated. Q4's change, a stale test. Fix: press "On this Mac only". Until that is fixed, the speech_choice fix can't be checked in the VM.
2. `e2e_setup_redesign_screens`: still waits for `30 languages · 590 MB` after Hindi (the same stale line I fixed in `e2e_onboarding_screenshots`). Q4's change; fix: expect "Hindi needs".
3. `e2e_onboarding_five_steps`: after Cancel and `rm -rf` of the models folder, the strip never shows "isn't downloaded yet" (`e2e_onboarding_five_steps.sh:87`; copy in `src/onboarding/logic.ts:133`). The app ended at Ready with the model loaded, so the path the scenario expects (the paused download then a missing model) looks changed by the single combined download. Could be a real behaviour change; the Developer should check what the strip shows after the model is deleted.
4. `e2e_dmg_install`: `no text field "api-key"`, no "Test & Save" button: the scenario types a key into the old key field; the UI is now Connect OpenRouter (Q3, already on main).
5. `e2e_onboarding_layout`: `LAYOUT key-none OK` / `key-saved OK` not reached at the minimum size, light and dark (welcome variants pass). This scenario is not touched by Q4. It may come from Q3's key screen (it is on main); I did not run it on main, so this is unverified. If it fails on main too, it is not Q4's.
Scenarios 4 and 5 likely fail on `main` after Q3 too; I did not verify, because that needs a second build. The Lead's post-merge e2e of Q3 would show it.

## Untested behaviour found
- No e2e checks that no `llama-server` is left after a switch or Quit in the VM (needs Gemma in the VM), and none shows the single combined download row (unit tests only).
- Host checks from run 1 (loopback only, no orphan after exit or `kill -9`) were not repeated on round 4's shutdown change; the `llama` unit tests (SIGTERM, grace, SIGKILL, reap) pass.

## Failures
See E2E above; app logs and screenshots are in `tests/e2e/out/20261120-232432-dictation-vm-2/` (local, git-ignored).
