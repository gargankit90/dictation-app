# P1 test report, run 3

PR: #241 at 858ec45 (round 3: fixed 1.5 s restore on a timer)
Result: FAIL (one check; everything else passes)

| # | Check | Evidence | Result |
| - | ----- | -------- | ------ |
| 1 | Unit tests | `cargo test -p dictation-platform --lib`: 256 pass | PASS |
| 2 | Mutation: restore without the token check (`MemClipboard::restore_if_unchanged` always restores) | 5 insert tests fail (`a_copy_at_1_0_s_is_never_overwritten`, `a_copy_before_the_second_dictation_is_what_it_gives_back`, `an_abort_after_the_write_keeps_what_the_user_copied_since`, `even_the_same_text_copied_meanwhile_counts_as_a_change`, `leaves_the_clipboard_alone_when_something_else_changed_it`) | PASS |
| 3 | Mutation: `RESTORE_DELAY` 300 ms | 4 tests fail (`a_target_reading_at_1_2_s_gets_the_transcript`, `a_second_dictation_at_0_8_s_finishes_the_first_restore_then_saves_the_original`, `other_readers_before_and_after_the_chord_change_nothing`, `the_original_comes_back_at_1_5_s`). Both mutations reverted, tree clean. I did not run the two combined: each alone already fails tests. | PASS |
| 4 | VM e2e (dictation-vm, run.sh, lock held): `e2e_first_paste_after_launch` x2 (12/12 landed), `e2e_clipboard_restore`, `e2e_image_clipboard_restore`, `e2e_user_copy_wins`, `e2e_unicode_multiline_selection_paste` | 6 passed, 0 failed, 0 skipped | PASS |
| 5 | Copy 0.5 s after a dictation is kept | `e2e_user_copy_wins` with `E2E_USER_COPY_DELAY=0.5` (dictation-vm-2): the user's copy is on the clipboard at the end, the old clipboard did not come back | PASS |
| 6 | Two dictations 0.8 s apart: original back at the end | ad-hoc scripted: final clipboard = the original sentinel | PASS |
| 7 | Dictation into Finder (no text field): transcript stays on the clipboard | ad-hoc scripted, `pbpaste` sampled every ~0.1 s in the VM: sentinel, then `hello world` (the transcript), then **the sentinel again about 1.5 s later**. The chord was posted fine, so the app restored the original on the timer; the transcript did not stay. | **FAIL** |
| 8 | Hosted CI | `gh pr checks 241`: DMG (macOS), Platform crate (macOS), Quality gate (Linux) all pass | PASS |

## Failure: check 7
Steps: set the clipboard to a sentinel, put Finder in front, dictate (synthetic press/release), sample the clipboard.
Expected (per the task brief and P1 criterion 4, "the user isn't left with nothing"): the transcript stays on the clipboard, or E1 shows it with Copy.
Actual: the transcript is on the clipboard for about 1.5 s and then the original comes back. The text went nowhere and is gone. Cause by design in `PasteInserter::insert`: only a failed chord keeps the transcript (the `Err(error)` branch); a posted chord always restores after `RESTORE_DELAY`, whether or not anything read the paste. Round 2's caret check detected the no-landing case; round 3 dropped it. The Lead can waive this as the trade-off of the simpler design; if so, criterion 4 should be reworded.

## Notes
- The Terminal and Finder ad-hoc scenarios were uncommitted and removed. I did not drive the GUI by hand through the MCP.
- The VMs were busy with other workers' runs for a long time; I waited and never cleared a lock.
- The e2e `clipboard()` helper waits for the restore, so it can't show the transcript phase; I sampled `pbpaste` in the VM for check 7.
