# T1 test report, run 1

PR: #107 at 64527e1
Result: PASS

Method: for each main criterion I reintroduced the bug locally (never committed), ran the tests, saw them fail, and restored the file (`git checkout`). Criterion 10 (VM suite) was not rerun: the Developer reported 10/10 and the coordinator waived the VM.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | CI macOS runs vendored patch tests; #77/#80 fail | `scripts/vendor-tests.sh` (macOS CI job + gate) | PASS | Clean run: 14 handy-keys + 2 coreaudio tests ok. #77: freeing the buffer with capacity 0 fails both coreaudio tests ("freed with its real capacity"). #80: swapping left/right in `modifiers_from_flags` fails 9 of 14 hotkey tests. CI "Platform crate (macOS)" green (44 s). |
| 2 | Overflow ends Idle, no recording/transcription, no Esc/sleep | `overflow_outcome::overflow_fence_discards_all_untrusted_queued_keys`, `m2_3_edges::criterion_7_*`, `audio_worker_tests::{keys_overflowed_discards..., a_dropped_key_cancels_the_recording}` | PASS | Failed with each of: queue not discarded (4 fail), no `CancelPressed` (4 fail), overflow wake-up ignored (4 fail), overflow `select!` arm moved below the keys arm = the #91 ordering bug (3 fail). |
| 3 | UI queue bounded, policy, recovers | `ui_events::tests::*`, `pipeline::tests::stalled_ui_consumer_keeps_bounded_state_and_errors` | PASS | Raising the 64 bound to 1,000,000 fails 3 tests. |
| 4 | Stale results ignored for old ID, handled for current | `pipeline::tests::all_job_result_variants_ignore_stale_ids`, `stale_results_change_neither_the_phase_nor_the_errors` | PASS | Guard replaced by `true` on `NotTranscribed` and on `AudioFailed(other)`: both tests fail each time. |
| 5 | Test key: exact clip, own deadline, one request, keychain bounded | `check_key::tests::*`, `transport_lifecycle::check_key_*`, `commands::tests::key_test::key_lookup_deadline_shows_retry` | PASS | `/ 1000` -> `/ 500`: 2 fail. Language override dropped: 2 fail. Keychain deadline ignored (1 h): `key_lookup_deadline_shows_retry` hits nextest's 60 s limit (fails, but slowly). |
| 6 | HTTP lifecycle | `transport_lifecycle.rs` (9 tests) | PASS | `TokioWait::wait` without the sleep: `tokio_wait_honors_delay` fails. Shutdown/backoff abort tests rely on the PR's own mutation run; I did not mutate those. |
| 7 | `cargo mutants` before/after | PR body table | NOT RERUN | Not rerun (time box). My hand mutations above match the table's entries (check_key arithmetic/language, TokioWait, stale guards, overflow). The one stated survivor (`Pipeline::spawn` log-once `!`) is diagnostic only. |
| 8 | coverage.sh fails closed | `scripts/test-coverage.sh` | PASS | Fixture: untested crate fails ("failed as expected"), tested crate passes. `--no-tests=pass` is gone from `coverage.sh`. |
| 9 | #47, #89, #73, #70, #86 | see notes | PASS | #47: Esc's release no longer falling through fails `esc_after_a_missed_modifier_release_clears_the_stuck_state`. #89: ignoring the session flags fails `a_synthetic_event_with_other_flags_while_option_is_held_sends_no_resync`. #73: accepting any next arg as the path fails `debug_audio_file_without_a_path_does_not_swallow_the_next_flag`. #86: dropping 3xx classification fails 3 `no_redirects` tests. #70: tests present and pass in the gate; not mutated. |
| 10 | VM suite | not rerun | n/a | Developer: 10 passed, 0 failed. Coordinator said no VM needed. |

## Gate

Local `scripts/gate.sh` on this checkout (macOS, warm cargo cache): all checks passed in 2 m 52 s, including `vendor-tests`, `test-coverage`, `test-guards` (63 passed) and `tests/e2e/test-key-handling.sh` (39 passed). Coverage: core 99.36 % (85), transcribe 97.91 % (85), app 97.62 % (70). CI on the PR: Platform (macOS) pass 44 s; Quality gate (Linux) pass 9 m 59 s.

## Flakiness

`cargo nextest run -p dictation-pipeline -p dictation-transcribe --stress-count 20 --retries 0`: 232 tests x 20 iterations, 20/20 iterations passed (3 m 27 s), including `overflow_outcome`, `m2_3_edges`, `transport_lifecycle` and the concurrent config saves. No flaky test seen.

## Untested behaviour found

- Nothing blocking, so I added no tests. `key_lookup_deadline_shows_retry` only fails by timeout (60 s) when the deadline is removed; minor, no issue opened.
- Not exercised: #89 against a real remote-desktop tool (the PR states this) and #88's listener restart (left open by design).

## Failures

None.
