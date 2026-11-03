# T5 test report, run 1

PR: #163 at 84f205b
Result: PASS

Method: every claim was checked by putting the bug back (a temporary local
edit, never committed, reverted with `git checkout`) and watching the new test
fail, and by running the fixed tests under load. Load was 14 `yes >/dev/null`
processes (one per core), stopped afterwards. The VM work used
`E2E_VM=dictation-vm-2` through `tests/e2e/run.sh` only.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | #147, #149 stable 30 in a row under load; each still fails with its real bug back | `tests/e2e/test-key-handling.sh`, `tests/e2e/test-runner.sh` | PASS | `test-key-handling.sh`: 30/30 in the foreground plus 10/10 as a `&` background job (the case that ignores SIGINT), under load. `test-runner.sh`: 30/30 under load. Bug back: deleting the `perl` SIGINT reset makes the background-job run fail ("INT after the store: the run fails with 130", 41 passed, 1 failed; the foreground run does not show it, as the PR explains). Making `vm_terminal` return 0 instead of the guest status (the harness not propagating a guest failure) fails 6 runner checks. |
| 2 | #150 fails within seconds with coalescing off | `overlay::tests::a_stalled_overlay_stays_bounded_and_ends_on_the_current_phase` | PASS | With the `queue.coalesce()` call removed in `ui_events.rs`, the test fails in 0.02 s: "the overlay channel is not bounded: 3017 events queued, at most 64 allowed". The nextest override (5 s, terminate after 1 period) is in `.config/nextest.toml`. |
| 3a | #153 unreadable TextEdit fails the check | `test-runner.sh` textedit_is_empty cases; live check in `dictation-vm-2` | PASS | In `dictation-vm-2` with TextEdit not running, `textedit_text` fails ("Can't get document 1", -1719) and `textedit_is_empty` returns 1 and logs "could not read TextEdit's text". A fresh empty document returns 0. With the old one-liner back, the two new runner cases fail (71 passed, 2 failed). |
| 3b | #159 onboarding auto-advance pinned to "granted" | `src/Onboarding.test.tsx` (2 new tests) | PASS | With the `state === "granted" &&` condition removed, both new tests fail (microphone and Accessibility; 2 failed, 18 passed). |
| 3c | #156 modifier reconciliation isolated from the live keyboard | `tap_disabled_pseudo_event_*` in `third_party/handy-keys` | PASS | With the handler reading the live `CGEventSource::flags_state` again, `tap_disabled_pseudo_event_takes_the_session_flags` fails (left `Modifiers(0x0)`, right `CTRL_LEFT`). Restored: 3/3 pass. |
| 4a | #136 `e2e_layout_paste` tells the paste key apart | `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_layout_paste` | PASS | Real lookup: PASS on all 5 layouts (French, Dvorak, Russian, Dvorak Right-Handed, Turkish). With `paste_chord` always returning the ANSI V key: FAIL on Dvorak, Dvorak Right-Handed and Turkish (TextEdit empty); French and Russian still pass, as the scenario's comment says. |
| 4b | #138 `launch_without_focus` outside the VM | `menu_bar::tests::the_builder_does_not_activate_the_app_at_launch` | PASS | Fails with `(false)` flipped to `(true)` and with the call dropped. |
| 4c | #142 singleton stop budget and accept-loop exit | `stop_waits_for_a_delivery_for_twice_the_io_timeout`, `the_accept_thread_ends_when_the_server_stops` | PASS | Budget `io_timeout * 2` changed to `io_timeout`: fails in 0.01 s ("stop's budget ... is twice the I/O timeout"). Accept-loop wake-up connect removed: fails after the 10 s bound with "the accept thread was still running after stop". |
| 5 | CI lints and doctests `dictation-platform` without `testing`; gate under ~10 min | `.github/workflows/ci.yml`, `scripts/gate.sh` | PASS | CI run 41421478755 on 84f205b: "Platform crate (macOS)" ran `cargo clippy -p dictation-platform -- -D warnings` and `cargo test -p dictation-platform --doc` as separate green steps (job 43 s). "Quality gate (macOS, self-hosted)" 3 m 54 s; whole CI run 8 m 30 s from creation (09:32:19Z) to completion (09:40:49Z), including queueing. DMG 1 m 7 s. |

## Gate
The gate was not run locally (the push hook runs it). CI on 84f205b: all three
checks pass (Quality gate 3m54s, Platform crate 43s, DMG 1m7s). Coverage is
part of the gate and passed there.

## Untested behaviour found
None blocking. Notes:
- The #147 fix is only visible when the script runs as a `&` background job
  (SIGINT inherited as ignored). The foreground run passes with or without the
  `perl` reset, so the fix is guarded by the cases that start it as a
  background job in the gate, not by every run.
- #143 and the doc/refactor items (#63, #49, #55, #54) are outside the
  criteria; not mutation-tested.

## Failures
None. All temporary edits were reverted (`git status` clean); the `yes` load
processes were stopped (`pgrep -x yes` shows none); `dictation-vm-golden` was
not touched.
