# SIMP1-B test report, run 1

PR: #349 at 1c6c758
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Gate | `scripts/gate.sh` (foreground) | PASS | all checks passed |
| 2 | Full e2e, dictation-vm-2 | `run.sh --exclude e2e_real_openrouter --exclude e2e_real_mic` | PASS | 44 passed, 0 failed, 1 skipped (`e2e_release_build_smoke`, optional) |
| 3 | Typed code + friendly sentence for every error | `cargo nextest run -p dictation-core diagnostics`: 11/11 (`every_code_says_what_to_do`, `every_code_has_one_row_in_order`, ...); e2e `e2e_settings_key_provider_recovery` (rejected key shows "rejected"), `e2e_microphone_denied_then_recover`, `e2e_overlay_rendered_states_and_error`, `e2e_tray_error_clears_on_next_recording`, `e2e_onboarding_first_run` | PASS | Spot-check used the existing scenarios, which use the stand-in provider and no real key. No separate hand-driven check of no-credits and no-internet; those are covered by the enumeration test over `ErrorCode::ALL`, not by a VM scenario of their own. |
| 4 | Launch at login toggles and survives relaunch | `e2e_launch_at_login` | PASS | |
| 5 | Nothing kept was removed | History (`e2e_history` PASS, `src/settings/History`), Help: `src/Troubleshoot.tsx` and "Report a problem" (App.test.tsx), Sentry (still referenced in `usage.rs`, `commands.rs`, telemetry), Welcome demo (`src/onboarding/WelcomeStep.tsx`, `demoScript.ts`), retry with jitter (`crates/core/src/retry.rs`), paste-key fallback ("Paste a key instead" exercised in e2e PASS), `e2e_real_openrouter.sh` present | PASS | Sentry reference count under `src-tauri/src` + `crates` unchanged vs main (6 files). |
| 6 | Release build has no single-instance socket | Static: `src-tauri/src/lib.rs:275` `config.forwarding = cfg!(debug_assertions)`; `single_instance/mod.rs:213,328` no listener/socket when off; unit test `without_forwarding_there_is_no_socket_and_a_second_launch_just_exits` | PASS (static) | Not checked on a built release app: `e2e_release_build_smoke` is optional and skipped, so no runtime check. |

## Gate
All checks passed. Coverage is CI's job; not rerun.

## Untested behaviour found
None new. Gap: no VM scenario for no-credits (402) or no-internet; the enumeration test covers their sentences.

## Failures
None.
