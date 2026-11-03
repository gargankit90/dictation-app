# L1 test report, run 2

PR: #217 at 87b8cb5 (code identical to 89e1e7b; the last commit is a review doc)
Result: PASS

Re-check of what changed since run 1 (2d20646): `main` merged in, the Tester's `e2e_local_model` scenario joined the suite, and three Reviewer majors were fixed (re-hash before first use, one download at a time, free the old model before loading the new one).

| # | Criterion | Result | Notes |
| - | --------- | ------ | ----- |
| 7 | Hosted CI, deny, NOTICE | PASS | The PR merges cleanly. On 87b8cb5: Quality gate (Linux) pass in 12m42s, with the C++ transcribe-cpp build included; Platform crate (macOS) pass in 2m17s; DMG (macOS) pass in 3m9s. Run 1's conflict is gone. |
| 3 | Model manager (Reviewer major: re-hash before first use) | PASS | New tests `an_existing_file_is_hashed_on_first_use_in_a_run`, `criterion_3_a_damaged_final_file_is_never_handed_out_for_loading`, `criterion_3_a_damaged_final_file_of_the_right_size_is_downloaded_again`. Mutation: in `verified_path` I changed `if hashed != model.sha256` to `if false && hashed != model.sha256`; all three failed (16 of 19 passed). Restored with `git checkout`; tree clean. |
| 4-6 | One download at a time, free old model before loading new | PASS | `cargo nextest run` on core, ports, platform, transcribe and app: 873 passed, 1 skipped (the env-gated real-model test). vitest: 128 passed. |
| 8 | [manual] VM | PASS (offline still blocked on #222, not a fail) | `E2E_VM=dictation-vm tests/e2e/run.sh e2e_local_model e2e_onboarding_first_run`: 2 passed, 0 failed, 0 skipped (the runner rebuilt the app from this head). `e2e_local_model`: download, Save, then three dictations pasted "Hello world, this is a dictation test on this Mac." into TextEdit. Whole press+2 s hold+release to Idle, measured from the host over ssh: 4.8 s, 3.6 s, 3.7 s (includes the 2 s hold and about 1 s ssh/launch overhead). No sudo in the VM, so the network could not be cut off (#222). |

The cloud path (`e2e_onboarding_first_run`, with the mock provider) passes. `e2e_settings_key_provider_recovery` was not re-run: run 1 passed and this round did not touch that path beyond the merge.

## Untested behaviour found
None new.

## Failures
None.
