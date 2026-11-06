# H1 test report, run 1

PR: #305 at e20b003
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Timeout = 30 s + 0.5 s per audio second, per request, pure in core | `upload_limits` unit tests (300 s -> 180 s, 10 s -> 35 s, 0 -> 30 s, saturating); code reads `.timeout()` per request, client has none | PASS | |
| 2 | Connect timeout ~10 s | `CONNECT_TIMEOUT` = 10 s; added ignored test `a_dead_network_fails_fast_for_a_five_minute_clip` (10.255.255.1, 300 s clip): `Timeout` after 20.2 s (2 x 10 s), not 180 s | PASS | run by hand with `--include-ignored` |
| 3 | Long clip (>= 60 s) retries a timeout once; short and other failures unchanged | unit tests at 59.999 s / 60 s; `long_upload` (4 s clip, threshold 3 s: 2 requests; 2 s clip: 3); added `other_failures_keep_every_attempt_at_any_clip_length`: 429 (Retry-After 0), 500, 503 at 1/59/60/61/300 s all make 3 requests, refused connection fails in < 5 s | PASS | |
| 4 | Slow server succeeds past 30 s base; silent server times out within new limit | `long_upload` tests | PASS | |
| 5 | Esc as before (Lead's note: Esc doesn't cancel transcription) | `e2e_cancel_and_busy_presses` in dictation-vm-2 | PASS | no change to coordinator |
| 6 | Key check, downloads, local models unchanged | `check_key` sets `timeout_grows_with_audio: false` (fixed budget); downloads use their own client (`http.rs` stall client, untouched); transcribe suite; `e2e_error_bad_key`, `e2e_settings_key_provider_recovery` | PASS | |

## Gate
Core and transcribe test suites pass. Local note: `the_key_never_appears_in_logs_or_errors` fails when its file runs in parallel on this Mac, on the base commit 74edb3a as well as the PR (log capture race: "retrying/giving up" missing); passes with `--test-threads=1`. Not caused by H1.

## E2E (dictation-vm-2)
`e2e_cancel_and_busy_presses`, `e2e_error_bad_key`, `e2e_settings_key_provider_recovery`: 3 passed, 0 failed. The key scan found no key in any file or log.

## Untested behaviour found
- Other failures at long clip lengths, dead-network at 5 min: tests added (above).
- A real long clip through the app on a throttled link was not tried: no simple way to throttle the VM's traffic to the local provider. Left to the owner's hotspot pass.
- Observation (not a defect): a connect timeout maps to `Timeout`, so for a long clip a dead network gives up after 2 x 10 s; short clips 3 x 10 s.
- Observation: no UI wording change; the worst waits are in the Lead's table.
