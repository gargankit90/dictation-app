# P1 test report, run 2

PR: #241 at 09c8e0c (round 2: caret check replaces the read receipt)
Result: PASS

| # | Check | Evidence | Result |
| - | ----- | -------- | ------ |
| 1 | Unit tests | `cargo test -p dictation-platform --lib`: 263 pass | PASS |
| 2 | Mutation: `Caret::shows_paste_of` always true | 7 insert tests failed (`a_paste_never_seen_landing_is_unconfirmed_after_the_timeout`, `a_copy_while_waiting_for_the_paste_ends_the_wait_and_is_kept`, `a_caret_in_another_field_is_not_the_paste`, `a_busy_target_gets_the_transcript_whatever_other_apps_read`, `shows_paste_of_needs_the_same_field_a_moved_plain_caret_and_the_last_char`, `the_hotkeys_own_selection_is_not_the_paste`, `the_same_text_again_waits_for_the_caret_to_move`); reverted, tree clean | PASS |
| 3 | `e2e_first_paste_after_launch` x2 (dictation-vm-2, run.sh) | 12/12 landed, clipboard = sentinel every time; `landed_ms` 20-100; one try logged "no readable caret" and still landed and restored | PASS |
| 4 | `e2e_clipboard_restore`, `e2e_user_copy_wins`, `e2e_unicode_multiline_selection_paste`, `e2e_focus_switch_during_transcription` | all PASS (6 passed, 0 failed, 0 skipped in the run) | PASS |
| 5 | No readable caret (Finder in front, ad-hoc scripted) | text on clipboard 0.5 s after the dictation, original sentinel back at 4.5 s; log "no readable caret" | PASS |
| 6 | Copy within 3 s not overwritten | `e2e_user_copy_wins` (no-caret Finder, copy at 1 s, checked after the 3 s restore: user's copy kept, no later clipboard write) | PASS |
| 7 | Dictate twice quickly (no-caret Finder, ad-hoc) | after each dictation the transcript is on the clipboard; 4 s after the second the original sentinel is back | PASS |
| 8 | Terminal (ad-hoc, raw-read loop writing to a file) | the transcript landed; two quick dictations landed both; clipboard back to the sentinel (caret was readable there, `landed_ms=60`) | PASS |
| 9 | Hosted CI | `gh pr checks 241`: DMG (macOS), Platform crate (macOS), Quality gate (Linux) all pass | PASS |

## Notes
- Ad-hoc scenarios were uncommitted and removed. I did not drive the GUI by hand through the MCP (Terminal AppleEvents time out in the VM), so results were read from files and the clipboard.
- The real-hotkey (Ctrl+Shift+A) path is covered by the first-paste loop; the ad-hoc runs used the synthetic press/release.
- Browser text area not tested (no browser set up in the VM); Finder stood in for a field without a readable caret.

## Failures
None.
