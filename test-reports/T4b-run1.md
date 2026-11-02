# T4b test report, run 1

PR: #160 at 9c2782b
Result: PASS

All runs on `dictation-vm-2` through `tests/e2e/run.sh` (it holds the VM lock). Made-up keys only.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | New scenarios pass | `e2e_overlay_rendered_states_and_error` (run twice), `e2e_launch_at_login` (two reboots), `e2e_microphone_denied_then_recover`, each once | PASS | Run 1: 4 passed, 0 failed, 0 skipped (the three plus `e2e_real_mic`). Overlay re-run alone: PASS. Not run: the full `run.sh` on `dictation-vm`. That VM is the Developers' and the brief asked for the three scenarios on `dictation-vm-2`. |
| 2 | Each fails when its bug is put back | Temporary local edits, restored after each (`git status` clean) | PASS | See mutations below. |
| 3 | Golden untouched; mic restored | `e2e_real_mic` after the mic scenario; `E2E_INJECT_FAILURE=after_mic_denied` | PASS | See below. |
| 4 | 39 key checks, `test-runner.sh` | `test-key-handling.sh`, `test-runner.sh` | PASS | 39 passed, 0 failed; runner 67 passed, 0 failed. |

## Mutations (AC2)

| Mutation (local only) | Scenario | Result |
| --- | --- | --- |
| `Overlay.tsx`: the `announceListening()` call (handshake) removed | overlay | FAIL: "the overlay renders Recording for a press right at launch (#119)" |
| `launch/macos.rs`: `unregister()` replaced by `Ok(())` (disable says Disabled, the item stays) | launch at login | FAIL: "the box is set off and saved", "the box shows unchecked", "the OS says disabled", "no copy started by itself". The cleanup also failed, as the OS state was wrong. |
| `coordinator.rs`: both `PermissionMissing` arms no longer `ShowError` (error hidden) | microphone | FAIL: "a second press shows the actionable error", "the overlay renders it", "the tray's tooltip has it" |

I did not separately mutate "app left Recording"; the hidden-error mutation covers the denial path.

## AC3 details

- After the microphone scenario passed in run 1, `e2e_real_mic` in the same run passed (Recording, Transcribing, Inserting, Idle; no permission error).
- `E2E_INJECT_FAILURE=after_mic_denied` (the README's way to inject; set on the microphone scenario, since `e2e_real_mic` has no injection point) died at "injected failure at after_mic_denied" and the cleanup logged "the microphone is granted again". `e2e_real_mic` run next with `--no-reset` PASSED.
- `dictation-vm-golden`: `disk.img` and `nvram.bin` mtimes unchanged (Oct 31 21:55) after all runs.

## Eyeball check

`e2e_microphone_denied_then_recover.error.png`: the overlay shows "Microphone permission is missing. Allow it in Privacy & Security, Microphone, then reopen the app." on two lines, whole, inside the pill, no clipping.

## Gate

Not run locally (laptop busy; the push hook and CI run it). Not assessed by me.

## Secrets

Grepped all run output (`tests/e2e/out/*-dictation-vm-2`) and the reports for the made-up provider keys (`t4agoodkey…`, `t4abadkey…`) and for `sk-`/`gsk_` patterns: no hits.

## Untested behaviour found

None blocking. Minor: the "app left Recording" mutation is not exercised separately.

## Failures

None.
