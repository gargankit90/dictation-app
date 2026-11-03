# E1 test report, run 2

PR: #225 at bed6ef9 (branch head now 3446af4, which only adds `docs/history/reviews/E1-round2.md`)
Result: PASS

Re-check of run 1's failures after the Developer's round 2 (`6f5cd72` fix, `bed6ef9` e2e scenarios).

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Overlay and tray use the table's message and next step | `cargo nextest run -p dictation-core -p dictation-ports -p dictation-pipeline`: 493 passed; VM | PASS | `ErrorCode::user_text()` (message + next step, capped at 105 characters, tested for every code) is now what the overlay and tray show (`crates/app/src/overlay.rs:100`). The table's texts were shortened to fit. In the VM the overlay for a denied microphone reads "Microphone is off for dictation-app. Allow it in Privacy & Security, then relaunch." with `mic_denied` under it, the same wording as the Troubleshoot row. |
| 2 | Local engine state is real | `cargo test -p dictation-app` 218 passed; VM | PASS | `local_state(EngineView, ModelState)` combines the live engine (loaded / loading / failed / missing) with the download status; only `Loaded` counts as ready. New tests: `a_present_model_that_fails_to_load_is_not_ready`, `a_model_that_is_loading_is_not_ready_yet`, `the_report_says_no_for_a_model_that_cannot_load`. |
| 3-5 | Report, planted secrets, privacy doc | unchanged since run 1 | PASS | Only `docs/privacy.md` (3 lines, wording about the engine check) and the engine-state code changed; the planted-secrets test still passes. |
| 6 | Gate pieces, CI | vitest 148/148; nextest 493 passed; `cargo test -p dictation-app` 218 passed; `gh pr checks 225` | PASS | Hosted CI is green on the current head: DMG (macOS) pass, Platform crate (macOS) pass, Quality gate (Linux) pass. |
| 7 | [manual] VM | `E2E_VM=dictation-vm-2 tests/e2e/run.sh` + hand-driven | PASS | See below. |

## Mutation check of the readiness fix
Made `local_state` return `Loaded` whenever the download status is `Ready` (the old file-size behaviour). `cargo test -p dictation-app diagnostics`: **2 failed** (`a_present_model_that_fails_to_load_is_not_ready`, `a_model_that_is_loading_is_not_ready_yet`). Reverted (`git status` clean); both pass again.

## VM, `dictation-vm-2` (this branch's debug build, installed by `run.sh`)
- Scenarios, one run: `e2e_microphone_denied_then_recover`, `e2e_overlay_rendered_states_and_error`, `e2e_tray_error_clears_on_next_recording`, `e2e_settings_key_provider_recovery`, `e2e_accessibility_revoked`, `e2e_hotkey_listener_recovers` and `e2e_error_bad_key`: **7 passed, 0 failed, 0 skipped**.
- Mic denied by hand (`tccutil reset Microphone` inside the VM, Don't Allow, press again): the 72 px overlay shows two lines of message plus the small `mic_denied` line, all inside the pill, nothing clipped (screenshot read by eye). The longest table text is `key_unreadable` at 88 characters (5 more than this one; the cap is 105 and the CSS clamps to two lines); I could not trigger it in the VM, so its fit is by arithmetic only.
- Local engine in Troubleshoot: with settings on "local" and a correctly sized but bogus model file in the models folder, the app's own launch check found the checksum wrong and deleted it (log: "an on-device model failed its checksum; deleted"); Troubleshoot then showed ✗ Transcription engine "The on-device model isn't downloaded. Download one in Settings. (model_missing)" with an Open Settings button. So a file that is present but wrong is not shown as ready. A real model that loads (✓ Ready), and one that is loading, were not driven in the VM (a 477 MB download); they are covered by the unit tests above.

## Untested behaviour found
- ✓ state for a genuinely loaded model, and the loading state, only in unit tests.
- Overlay fit for the 88-character message by arithmetic only.

## Failures
None. No minor findings, so no tech-debt issues opened.
