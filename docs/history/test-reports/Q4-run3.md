# Q4 test report, run 3

PR: #337 at bf467bc (full run), re-checked at 019dc41 (the Developer's round-5 fixes)
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | OrukQwen only, verified hash/size | gate (`dictation-core local_models`) at bf467bc and 019dc41 | PASS | |
| 2 | Local cleaner: cleaned text, timeout/error to raw, cancel, restart | gate (`llama`, `cleanup` tests); `e2e_cleanup_applied`, `e2e_cleanup_deadline` | PASS | |
| 3 | Parakeet / Qwen3-ASR gone, unknown id loads | gate unit tests | PASS | |
| 4 | Gate | `scripts/gate.sh` at bf467bc; `scripts/gate.sh --full` at 019dc41 | PASS | |
| 5 | Onboarding download resume after the round-5 fix | `e2e_onboarding_five_steps`, `e2e_onboarding_first_run` at 019dc41 | PASS | |
| - | Onboarding speech_choice bug (run 1, finding 2) | none | NOT APPLICABLE | SIMP1-A (on main, merged in) removed `e2e_onboarding_round2` and the persisted `speech_choice`: the Speech pick is now in-memory only (`Onboarding.tsx`). Nothing left to verify in the VM. |
| - | Single download row (OrukQwen + Gemma as one) | unit tests only | NOT TESTED | Staging Gemma in the VM was not possible in the time box. |
| - | No `llama-server` left after switch to cloud or Quit | unit tests (`llama`: SIGTERM, grace, SIGKILL, reap) | NOT TESTED in the VM | Same reason; no e2e scenario covers it. |

## E2E
Full run at bf467bc in dictation-vm-2 (`e2e_real_openrouter`, `e2e_real_mic` excluded): **42 scenarios PASS, 0 FAIL** before the runner was killed by the 30 min background limit. Not reached in that run: `e2e_history`, `e2e_notarized_dmg_first_launch`, `e2e_release_build_smoke`, `e2e_update_privileged_install`. They are not touched by Q4. All five run-2 failures (`e2e_setup_redesign_screens` and `e2e_onboarding_round2` were deleted by QT1; `e2e_onboarding_five_steps`, `e2e_dmg_install`, onboarding layout) now pass or no longer exist.
At 019dc41, after the round-5 fixes: `e2e_onboarding_five_steps` PASS, `e2e_onboarding_first_run` PASS (2 passed, 0 failed).

## Untested behaviour found
- No e2e shows the single combined download row or that no `llama-server` is left after a switch or Quit (needs Gemma in the VM). Unit tests cover shutdown.
- The four scenarios listed above were not run at either commit in this run.

## Failures
None.
