# T3 test report, run 1

PR: #141 at df84261
Result: PASS

CI can't run (GitHub billing), so everything below is local on macOS. No VM and no e2e scenario run (criterion 4: no scenario changed).

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Each issue has a test that fails when its bug is put back | per-issue table below | PASS | 13 of 14 mutations I made fail a T3 test; #139 can't be put back deterministically (see table) |
| 2 | No sleep as proof; fences and bounded outcomes | grep of the whole PR diff | PASS | No `sleep` anywhere in added lines. Waits are condvar/channel fences with 20-30 s failure bounds. One bounded `yield_now` spin (see below) |
| 3 | Gate under ~10 min; changed tests 50x under CPU load | see below | PASS | Gate 3:46 (226 s). All changed tests 50/50 under 8 `yes` processes |
| 4 | `tests/e2e/run.sh` still passes | not run | N/A | No scenario changed, as briefed. `gate.sh` includes `tests/e2e/test-key-handling.sh`: 39 passed |

## Criterion 1: each bug put back (temporary local edits, never committed)

| Issue | Mutation | Result |
| --- | --- | --- |
| #113 | `KeyWrites::write` never sees an earlier write (no queue) | 3 fail: `a_stuck_write_times_out_and_a_retry_starts_nothing_until_it_finishes`, `blocked_key_save_returns_actionable_outcome`, `late_key_save_cannot_overwrite_a_newer_choice` |
| #121 | `settings.validate()` removed from `save_with_login` | `invalid_settings_never_change_login_item` fails |
| #121 | `launch::restore` call removed | `failed_settings_write_reports_actual_login_state` fails (`left: [true]`) |
| #124 / panic leak | `std::panic::set_hook(log_panic)` removed | `spawned_pipeline_logs_stay_private_across_rotation` fails (stderr holds the key) |
| #130 | `delivering == 0` becomes `!= 0` in `Delivery::drop` | `delivery_completion_wakes_shutdown_without_waiting_for_deadline` fails after its 20 s bound (`Err(Timeout)`) |
| #133 | `!` dropped in `Server::stop` (`if !begin()`) | `lock_stays_held_until_delivery_shutdown_finishes` fails |
| #120 | physical `/ scale` becomes `* scale` (x) | `physical_work_area_converts_offsets_and_size_at_2x` and `physical_work_areas_at_other_scales_and_origins` fail (`x: 5760.0`) |
| #129 | `choose_dir` guard `&& false` | `default_dir_prefers_stable_user_cache_when_socket_fits` fails |
| #112 | coalescing off in `ui_channel` | `a_stalled_overlay_stays_bounded_and_ends_on_the_current_phase` fails |
| #110 | `allow-has-api-key` dropped from `capabilities/default.json` | `the_settings_window_may_invoke_all_five_app_commands` fails (also 2 older capability tests) |
| #125 | resampler delay skip set to 0 | `exact_clip_length_and_alignment_across_supported_rates` fails (`left: 1026`) |
| #122 | `set_enabled` returns `Ok` for everything | 4 fail (`delegates_to_the_mac_adapter`, `already_in_requested_state_is_idempotent`, `refusal_with_mismatching_status_is_reported`, `requires_approval_is_not_enabled`) |
| #127 | shell `ScratchConfig::drop` stops removing its dir | `the_config_fixture_leaves_nothing_behind` fails |
| #139 | not put back | The fix is a unique board name in the test helper; the old failure was about 1 in 100 across processes. I checked the new naming (pid + nanos + counter) by running `insert::macos` 50x under load: all pass. Not a deterministic proof |
| #115 | not re-mutated | `pipeline_shutdown_*` tests pass in all runs; the PR's own `abort_all` to `detach_all` mutation was not repeated |
| #128 | not testable | CI is down, as the PR says |

## Criterion 3

- Gate (`scripts/gate.sh`, no load): `gate: all checks passed`, 3:46 total. Coverage: core and transcribe at or above 85%, app (dictation-pipeline) 97.81% (threshold 70%). `test-guards.sh` 63 passed; `test-key-handling.sh` 39 passed. Workspace nextest: 850 run, 850 passed, 2 skipped.
- Stress, `cargo nextest --stress-count 50` with 8 `yes` processes running:
  - dictation-app (key_save, key_writes, launch_at_login, capability_tests, pipeline_shutdown, spawned_pipeline_logs, scratch fixture): 24 tests, 50/50 iterations.
  - dictation-pipeline (a_stalled_overlay, restore_): 4 tests, 50/50.
  - dictation-platform (launch::, overlay::, single_instance, exact_clip_length, insert::macos, scratch): 69 tests, 50/50.

## Panic messages hidden entirely: does it lose information?

Yes, some, and it is a judgement call. What `log_panic` records: thread, `file:line:col`, the message length, and a backtrace only if `RUST_BACKTRACE` is set. What a user sending us a log gets: "a thread panicked, location X, message 12 bytes". Often the location is enough (an `unwrap` on `None`, an index out of bounds). It is not enough for panics whose useful part is a value (`index out of bounds: the len is 3 but the index is 5`, or `Result::unwrap()` on an `Err(...)`), and there is no backtrace in a normal install.

Options the Developer or Lead could weigh:
1. Log the message when the payload is a `&'static str`. A literal `panic!("...")` or `expect("...")` cannot hold a runtime key or transcript; only formatted `String` payloads can. Hide those. This keeps the readable half at no leak risk.
2. Log the message only when debug mode is on (the existing opt-in for audio).
3. Keep a backtrace on by default for panics (cheap, no data values).

Related gaps I saw (minor, filed as one issue): the hook is installed inside Tauri `setup` after `try_init`, so a panic before that point, or a failed `try_init`, still uses Rust's default hook and prints the message unscrubbed to stderr. The `eprintln!` calls in `lib.rs` (lines 183-202, 278) print CLI results and a start-up reason, not panic text.

## Untested behaviour found

- The `stop_acknowledged` stand-in in `audio/macos/tests.rs` busy-spins with `yield_now()` for up to 20 s. Bounded and not a sleep-as-proof, but it burns a core under load. No change needed.
- Two surviving mutants the Developer already listed (`io_timeout * 2` to `/ 2`; `is_stopping` to `false`) are not covered. Not blockers; the Developer proposed tech-debt issues.
- No new tests added: every criterion-1 issue already had a test that failed on the mutation.

## Failures

None.
