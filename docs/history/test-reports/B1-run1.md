# B1 test report, run 1

PR: #205 at 2a4cd8b
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Next dictation retries after a keychain timeout/error | `criterion_1_the_next_dictation_retries_after_a_keychain_timeout`, `criterion_1_a_failing_read_is_retried_too` | PASS | Read the tests: the first dictation (keychain still blocked) shows the same "Couldn't read your API key from the Keychain" error and pastes nothing; after the fake unblocks, the next dictation pastes with no restart and no key save. Retry is built by `engine_from_config` with the same `KeyReads` and timeout, run in `spawn_blocking`. |
| 2 | A successful stored-key read rebuilds an Unavailable engine | `criterion_2_a_successful_stored_key_read_rebuilds_the_engine` (loops over `read_key` and `onboarding::key_saved`) | PASS | Engine name goes `unavailable` to `remote`. |
| 3 | No extra reads / no loop | `criterion_3_no_extra_reads_when_the_engine_is_fine`, `..._when_no_key_is_saved`, `..._keeps_failing_costs_one_bounded_read_per_dictation`, `..._failing_keychain_is_read_at_most_once_per_dictation`, `a_pinned_engine_never_rebuilds` | PASS | Counts `get_calls` on the fake: 0 extra for a working engine, 0 for no key, 0 new threads while one read is stuck, exactly 1 per dictation for an erroring keychain, 0 for a pinned engine. |
| 4 | "Key works" confirmation | `Onboarding.test.tsx`: "confirms the saved key works when nothing is typed", plus an assertion in the typed-key test (role="status") | PASS | 21/21 vitest in the file pass. Copy is "Key works ✓", shown at the top of the Try step. Brief and plain. It stays for the whole Try step rather than being momentary, which is fine. |
| 5 | No key/transcript in logs; tests; gate | diff review, CI | PASS | The only new log is `tracing::info!(engine = engine.name(), "engine rebuilt after a keychain failure")` (a static engine name). No key or transcript is logged. The onboarding e2e checks "log never holds the transcript / rejected key / saved key" also PASS. |
| 6 | [manual] Dictate after allowing access, no restart | existing e2e only | PARTIAL, for the Lead | See below. |

## Mutations (each reverted afterwards; tree is clean)

- `engine_for_dictation` always returns the current engine (retry disabled): 3 tests FAIL (`criterion_1_the_next_dictation_retries...`, `criterion_1_a_failing_read_is_retried_too`, `criterion_3_a_failing_keychain_is_read_at_most_once_per_dictation`).
- `read_key` no longer calls `rebuild_if_keychain_failed`: `criterion_2_...` FAILS.
- Removing the keychain-failed check inside `update_if_keychain_failed` (always rebuild): no test fails. It is redundant with the pre-check in `rebuild_if_keychain_failed`/`engine_for_dictation`, so the "a good engine is never replaced" re-check under the lock (a race guard) has no direct test. Minor, not a blocker.

## Gate
- `cargo nextest run -p dictation-app`: 185 passed. `-p dictation-transcribe`: 94 passed, 1 skipped.
- `vitest src/Onboarding.test.tsx`: 21 passed.
- Hosted CI on 2a4cd8b: Quality gate (Linux) pass, Platform crate (macOS) pass, DMG (macOS) pass.
- Test merge with `origin/main` (temp branch, not pushed): "Already up to date", no conflicts.
- Coverage numbers: not run locally; covered by the Quality gate (Linux) check, which passed.

## E2E (dictation-vm-2 only)
`E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_onboarding_first_run e2e_error_bad_key e2e_settings_key_provider_recovery`: 3 passed, 0 failed, 0 skipped (output in `tests/e2e/out/20261103-064143-dictation-vm-2/`, git-ignored).

### Criterion 6 (slow keychain)
Not simulated in the VM. There is no test hook to delay the keychain read, and the e2e stores the key through the app's own hook (`tests/e2e/test-key-handling.sh`), not the login keychain I could lock with `security lock-keychain`. I did not try to rework that within the time box. The behaviour is covered by the fake-store Rust tests above (blocked, then unblocked, then the next dictation pastes), which drive the real `LiveEngine`/`Transcribers` retry path. Left for the Lead's manual check: launch a build with the keychain prompt pending, allow access, and dictate in the Try step without restarting.

## Untested behaviour found
- The under-lock re-check in `update_if_keychain_failed` (see mutation 3). No test added; the pre-check makes it unreachable single-threaded. Minor.
- The `cua-vm-2` MCP server failed to connect in this session, so no manual GUI driving was done.

## Failures
None.
