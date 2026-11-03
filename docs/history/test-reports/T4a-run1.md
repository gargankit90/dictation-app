# T4a test report, run 1

PR: #157 at 228505a (head with the Developer's R1 fix, "reach a loopback provider directly, never through a proxy"; `git ls-remote` confirmed this was still the branch head when the report was written)
Result: PASS

VM: `dictation-vm-2` (E2E_VM), every run through `tests/e2e/run.sh`. Date 2026-11-01.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Endpoint override only in test builds | `flags::tests::release_builds_ignore_the_provider_url`, `the_provider_url_is_honoured_only_in_test_builds`, loopback-only tests; `cargo test -p dictation-app --release flags::` (25 passed) | PASS | Code path read: `parse` takes `--debug-provider-url` only under `cfg!(debug_assertions)`; `LiveEngine::at` is `#[cfg(any(test, debug_assertions))]`; `with_provider_url` is a no-op in release. Only loopback `http` is accepted even in a test build. |
| 2 | `pipeline_shutdown_closes_real_http_without_retry_or_insert` | `cargo nextest run -p dictation-app -E 'test(pipeline_shutdown)'` | PASS | Both the real-HTTP and the backoff variant pass. The VM mutation for "shutdown not cancelling" (see 4) fails `e2e_quit_during_request`. I did not mutate the Rust unit test itself. |
| 3 | The five new VM scenarios (+ the #151 scenario) | full e2e run, 6 scenarios | PASS | 6 passed, 0 failed, 0 skipped (`out/20261101-203522-dictation-vm-2`). Re-run of recovery and tray scenarios on a clean build after the mutation runs: both PASS (`out/20261101-205126-dictation-vm-2`). |
| 4 | Each scenario fails with its bug put back | local, uncommitted edits, restored with `git checkout -- .` (tree clean) | PASS | Table below. |
| 5 | 39 key-handling checks, `test-runner.sh`, no key in logs/reports | `test-key-handling.sh` 39/39; `test-runner.sh` 67/67; `test-vm-lock.sh` 271/271 | PASS | The made-up keys (`t4agood…`, `t4abad…`) and the key-handling sentinels appear in no file under `tests/e2e/out/20261101-20*` (all runs of this session, mutated ones included), no scratch log and no report. |

Scenarios in the clean run:

| Scenario | Result |
| --- | --- |
| `e2e_settings_key_provider_recovery` | PASS |
| `e2e_cancel_and_busy_presses` | PASS |
| `e2e_focus_switch_during_transcription` | PASS |
| `e2e_unicode_multiline_selection_paste` | PASS |
| `e2e_quit_during_request` | PASS |
| `e2e_tray_error_clears_on_next_recording` (#151) | PASS |

## Mutations (criterion 4)

| Scenario | Bug put back (temporary edit) | Result |
| --- | --- | --- |
| `e2e_settings_key_provider_recovery` | removed `live.rebuild(config)` in `engine::changed` | FAIL: "the engine was rebuilt from the new key", "it was sent with the new key", "TextEdit holds exactly the transcript", "the tray's error is gone" |
| `e2e_cancel_and_busy_presses` | `(Transcribing, HotkeyPressed)` starts a recording (busy press queued) in `coordinator::transition` | FAIL: "neither started anything", "nor sent a request", "it pasted once and nothing followed", "still two requests" |
| `e2e_unicode_multiline_selection_paste` | `clean` replaces `\n` with a space | FAIL: caret insert, selection replace and "exactly once" checks |
| `e2e_quit_during_request` | `Pipeline::shutdown` waits 30 s before cancelling | FAIL: "Quit in the tray ends the app by itself within 10s", "pipeline stopped", "nothing was inserted" |
| `e2e_focus_switch_during_transcription` | `PasteInserter::insert` sends the paste keys twice | FAIL: "B got exactly one transcript, A is unchanged" (both rounds). A pure wrong-window mutation would need new product code; this proves the scenario's exactly-once and A-unchanged checks bite. |
| `e2e_tray_error_clears_on_next_recording` and recovery (#151) | tray title is the full error text | FAIL: "the tray shows the error (short title, message in the tooltip)"; recovery also fails "the tray shows the error, and still fits in the menu bar" and then cannot open Settings from the tray |

Mutations 1-4 were built into one binary and run together; each scenario fails on its own bug's checks. The recovery scenario also failed its Quit check there, from the shutdown mutation in the same build; its own checks failed for the missing rebuild regardless. The focus scenario passed in that combined run (none of those four bugs touch it) and failed in its own run with the double-paste edit.

## Gate
Not re-run: CI and lefthook cover it; the dev's notes say the gate passed. Run here: host tests above, plus `cargo nextest run -p dictation-transcribe -E 'test(loopback)'` (3 passed), the R1 proxy-bypass regression tests: `loopback_no_proxy::loopback_override_requests_bypass_an_environment_proxy`, `child_sends_through_the_loopback_override`, `http::tests::only_plain_http_to_loopback_bypasses_proxies`. Coverage not measured by me.

## Eye check (#151)
Screenshot of `dictation-vm-2` after the no-key error (`out/20261101-205126-dictation-vm-2/e2e_tray_error_clears_on_next_recording-no-key.png`): the status item is in the menu bar showing "⚠ Dictation failed" next to the icon, and the full message is in the overlay. For the bad-key case the scenario checks the same title, and then opens Settings from the tray (it only works while the item is visible); both pass. The `cua-vm` MCP points at `dictation-vm` (the Developers' VM), so I used the scenario screenshots from `dictation-vm-2` rather than drive it live.

## Untested behaviour found
None blocking. The focus-switch mutation is a stand-in (double paste), not a true wrong-window paste. The Rust unit `pipeline_shutdown_closes_real_http_without_retry_or_insert` was not mutated by me; the VM twin catches the same bug.

## Failures
None on the clean head.
