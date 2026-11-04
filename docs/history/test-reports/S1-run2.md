# S1 test report, run 2

PR: #255 at 1f0c79a
Result: PASS

Changes since run 1: no OS read under the lock, a failed store shown to the user (`NotStored`), e2e wait raised to 150 s.

| # | Check | Test(s) | Result | Notes |
| - | ----- | ------- | ------ | ----- |
| 1 | New tests | `a_slow_os_read_after_a_change_does_not_block_requests_or_the_screen`, `a_failed_store_is_reported_with_the_real_state`, `a_settings_file_that_cannot_be_written_is_reported` | PASS | 45 tests pass in `-p dictation-pipeline -p dictation-app launch`; Settings.test.tsx 77 pass. |
| 2 | Mutation: OS read back under the lock | `let _ = login.state();` inside the locked block of `work()` | PASS (killed) | `a_slow_os_read_after_a_change_does_not_block_requests_or_the_screen` failed (44 passed, 1 failed). Restored; tree clean. |
| 3 | e2e_launch_at_login x2 | `tests/e2e/run.sh e2e_launch_at_login` in dictation-vm-2 | PASS 2/2 | Run A (after the quick-toggle scenario, same VM session): Saved ≤3 s and ≤4 s, OS answered after 4 s and 33 s. Run B (fresh VM): Saved ≤3 s and ≤3 s, OS answered after 4 s and 12 s. |
| 4 | Quick toggle (scripted, untracked scenario `e2e_zz_s1_quick_toggle`) | box on at start; off, on, off in 7 s; type a language; Save; settle; reopen; relaunch | PASS | Saved in 3.4 s; the language was in the settings file at once; box unchecked, "Applying…" gone after 2 s, OS disabled, settings file `launch_at_login: false`. After Quit and relaunch: box unchecked, OS disabled, file false. One check of my own failed: "Applying… is showing" after the third toggle: the OS answered in about 2 s, so the text had already gone by the time Save was pressed. So Save while "Applying…" visible was not seen live this time; unit tests cover it. |
| 5 | Settings open time right after a toggle | same scenario | PASS | Toggle on, close, reopen from the tray: window shown in 4.8 s (includes tray click and ax start-up), box ready 2.0 s later, showing the pending choice (checked); settled 1.8 s later, OS enabled. No long hang. |
| 6 | Hosted CI | `gh pr checks 255` | PASS | Quality gate (Linux), Platform crate (macOS), DMG (macOS) green on 1f0c79a. |

## Gate
Hosted CI green. `scripts/gate.sh` not re-run locally.

## Untested behaviour found
- "Save while Applying… shows" was only exercised by unit tests this run, because the OS was fast in the scripted check (about 2 s to settle). In run 1 the slow OS (30-70 s) covered Save during a long pending change.
- The failed-store path in the real app (settings file unwritable after the OS changed) is covered by unit tests only.

## Failures
None in the product. One infrastructure flake: the first attempt stopped at "installing dictation-app.app in the VM" with `Connection timed out during banner exchange` (ssh to the freshly reset VM while the host was busy building); the rerun with the same build passed. The quick-toggle scenario is untracked in my worktree (`tests/e2e/scenarios/e2e_zz_s1_quick_toggle.sh`) and not committed.
