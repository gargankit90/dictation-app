# P1 test report, run 1

PR: #241 at 33495a1
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Relaunch-and-dictate loop in a VM | `e2e_first_paste_after_launch` (3 relaunches x Right Option and Ctrl+Shift+A) | PASS | The pre-fix failure rate is the Developer's to state in the PR; not re-measured here (needs the pre-fix build). |
| 2 | Root cause with log evidence | PR notes; app log `first_read_ms` | PASS | Run 1 of 2, Ctrl+Shift+A try 1: the target read the paste 1080 ms after the chord, well past the old fixed 300 ms, so the old code would have restored the old clipboard under the paste. Matches the stated cause (restore before the target reads). |
| 3 | Fix + unit test + 10/10 style loop | `cargo test -p dictation-platform --lib insert` (49 pass); e2e loop run twice | PASS | 12 of 12 first pastes landed over two runs (3 per key per run, 2 runs): text in TextEdit, clipboard back to the sentinel every time. Mutation check below. |
| 4 | Unconfirmed paste never loses the transcript | unit `without_a_read_the_transcript_stays_on_the_clipboard`; ad-hoc VM run | PASS | Ad-hoc VM run, Finder in front (no text field): no read in 2 s, the app logged `no app read the paste`, clipboard = the transcript, not the sentinel. |
| 5 | Loop is a VM scenario in the suite | `e2e_first_paste_after_launch` in `ALL_SCENARIOS` | PASS | Default 3 relaunches per key. |
| 6 | Gate; hosted CI green | `gh pr checks 241` | PASS | DMG (macOS), Platform crate (macOS), Quality gate (Linux) all pass. |

## Mutation check
Replaced `wait_for_read` with the old behaviour (fixed `DEFAULT_SETTLE` wait, return at once) locally: 4 tests failed
(`a_read_before_the_chord_is_not_the_paste`, `a_copy_while_waiting_ends_the_wait_and_is_kept`,
`the_restore_waits_for_a_late_read_by_the_target`, `without_a_read_the_transcript_stays_on_the_clipboard`). Reverted; tree clean.

## E2E (E2E_VM=dictation-vm-2, run.sh, lock held)
- `e2e_first_paste_after_launch` x2: PASS both (12/12 landed). `first_read_ms` mostly 10 ms, one 60 ms, one 1080 ms.
- `e2e_clipboard_restore`, `e2e_unicode_multiline_selection_paste`, `e2e_focus_switch_during_transcription`: PASS.
- `e2e_user_copy_wins` (extra): PASS. The user's copy is made 0.2 s after the chord, inside the new 2 s window, and is not overwritten.

## By-hand checks (scripted, in the VM)
The sentinel, relaunch, Ctrl+Shift+A, dictate-immediately sequence is exactly what `fp_try` does, with real key events: transcript lands, clipboard is the sentinel again. The no-text-field case was an ad-hoc uncommitted scenario (Finder in front): nothing pasted, transcript left on the clipboard. The new-copy-within-2 s case is `e2e_user_copy_wins`. I did not drive the GUI with the MCP by hand.

## Gate
Not rebuilt; CI green (above). NOTICE credits Handy `paste_tx/macos.rs` and `paste_tx/mod.rs` (MIT, commit 9e57a0d); THIRD-PARTY-LICENSES.txt regenerated.

## Untested behaviour found
- The "no app read the paste" path (transcript stays on the clipboard) has unit coverage only, no committed VM scenario; filed as a minor tech-debt issue.
- A stale transcript left on the clipboard after a timeout is by design (documented in `insert/mod.rs`).

## Failures
None.
