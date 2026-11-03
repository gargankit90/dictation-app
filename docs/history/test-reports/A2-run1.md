# A2 test report, run 1

PR: #228 at d78b535
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Spike notes clear; native-crash issue exists | read `docs/history/milestones/A2.md`; `gh issue view 226` | PASS | Spike records crates tried (sentry/sentry-minidump 0.49.5), panics captured, native capture works but the minidump holds stacks and environment (planted string found in it) and `before_send` cannot edit attachments, so decision is panic + error capture only. #226 is open: "native crash reports (minidumps) without leaking memory". |
| 2 | Inert without DSN and with switch off | `without_a_dsn_nothing_is_sent_or_written`, `off_stops_reports_and_on_resumes_them`, `a_waiting_panic_report_is_sent_at_the_next_launch_once` (off at launch deletes unsent); code read | PASS | No DSN: no client, no runtime; `set_enabled`/`report`/`record_panic`/`install_panic_hook` return early; no dir created. Scripted launches pass no DSN. Switch off: `report` and `record_panic` check `enabled`. |
| 3 | Scrubbing | `an_error_report_holds_the_code_and_type_and_no_text`, `a_panic_report_keeps_places_and_drops_everything_else`, `paths_become_crate_relative`; mutation checks | PASS | The tests plant an API key, dictated words, a home path, user name, host name and a password in error `reason` strings and in a backtrace/location under a home path, and assert none is in the serialised body, and that `server_name`, `breadcrumbs`, `"user"`, `"request"`, `contexts` are absent. Events are built from an allow-list (`SentryReporter::event`): release, environment, sdk, tags (arch, macos_major, error_code, error_type), fingerprint, exception type/value (code name or `panicked at <crate-relative>:line`), frames. The panic message is never an input. No user, IP, hostname or device name fields. **Mutation 1** (event value = `error.to_string()`): `an_error_report_holds_the_code_and_type_and_no_text` FAILED ("leaked my private dictated words"). **Mutation 2** (raw panic file path instead of `crate_relative`): `a_panic_report_keeps_places_and_drops_everything_else` FAILED. File restored; tree clean. |
| 4 | Off stops at once, drops queued; on re-enables | `off_stops_a_request_under_way_and_deletes_a_waiting_panic_report`, `off_stops_reports_and_on_resumes_them` | PASS | A request delayed 30 s is cancelled (in_flight back to 0) and the pending panic file is deleted when switched off; nothing reaches the mock server while off; after on, a report arrives. `Usage::set_enabled` drives both PostHog and Sentry from the one switch, and the call sites were switched over (incl. `write_sharing`). |
| 5 | `scripts/release.sh` | `scripts/test-release.sh` (stub `security`, `gh`, `npm`). I did not run `release.sh` myself, never used `--publish`, never touched the Keychain | PASS | Read the diff: the optional items are read with `security find-generic-password -w 2>/dev/null` and default to empty; a missing item prints `WARNING: no Keychain item …, so this build sends NO …` and the build goes on; a found item prints only "found <service>". Values are exported only inside the build subshell after `unset` of any caller value; `redact` also masks both values in build output. The test checks (a) the stub npm receives both fake values, (b) with the items missing it warns for both, drops the caller's env values and still builds `latest.json`, (c) the fake values never appear in the output, including with a leaky build and under `bash -x`, and are not written anywhere in the repo. All lines `ok`. docs/releasing.md has the two `security add-generic-password -s … -a "$USER" -w` commands. |
| 6 | privacy.md and Settings "What's sent" | read both against `errors.rs` | PASS | The privacy.md field table matches the event fields (release, environment, error_code, error_type, arch, macos_major, exception type, panic location and stack); it says US region, off deletes the waiting report and drops requests under way, 30 per run (= `MAX_PER_RUN`), no native crashes. The Settings label "Share usage and error events" and the new list item match; `Settings.test.tsx` updated. |
| 7 | Hosted CI | `gh pr checks 228` | PASS | Quality gate (Linux), Platform crate (macOS), DMG (macOS) all pass. |
| E2E | `e2e_settings_save_reopen`, `e2e_error_bad_key` in dictation-vm-2 | `E2E_VM=dictation-vm-2 tests/e2e/run.sh` | PASS | 2 passed, 0 failed, 0 skipped. (The first attempt waited ~20 min for another run's VM lock.) |

## Gate
CI: all three checks pass. Locally: `cargo test -p dictation-telemetry`: 26 passed, 0 failed. `scripts/test-release.sh`: all ok. I did not rerun the whole `scripts/gate.sh` or coverage (CI did).

## Untested behaviour found
- No test installs the real panic hook and panics, so the hook's `Backtrace` capture feeding `record_panic` is covered only in parts (`record_panic` with a hand-written trace; the launch send with a dummy file). Minor.
- Error-chain scrubbing is tested with one-level `DictationError` reasons; `ErrorReporter::report` never reads the error's text, so a deeper chain can't leak by construction.
- Real Sentry ingestion is not testable here (agents hold no DSN).

## Failures
None.
