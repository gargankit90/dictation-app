# P1 test report, run 4

PR: #241 at fc0b73d (rounds 4 and 5: quit-time restore, no quit deadlock, cached Cmd-V key code)
Result: PASS

Run 3's one failing check (no text field) was waived by the Lead; not re-tested.

| # | Check | Evidence | Result |
| - | ----- | -------- | ------ |
| 1 | VM e2e (dictation-vm, run.sh, lock held): `e2e_quit_right_after_paste`, `e2e_layout_paste`, `e2e_first_paste_after_launch` (1 relaunch per key), `e2e_clipboard_restore` | 4 passed, 0 failed, 0 skipped | PASS |
| 2 | Quit within 1 s of a dictation: original clipboard back after exit | `e2e_quit_right_after_paste`: clipboard after the quit = the sentinel; log "restored the clipboard before quitting", 1512 ms after Inserting. Ad-hoc run: clipboard after exit = the sentinel | PASS |
| 3 | Layout switch, then dictate | `e2e_layout_paste`: French, Dvorak, Russian, Dvorak-Right (and the rest of its list): each dictation pasted `hello world` into TextEdit and the layout stayed selected | PASS |
| 4 | Quitting never hangs, under 3 s | ad-hoc: Quit clicked in the tray as soon as the log showed Inserting, process gone 1.661 s later (this includes finishing the pending 1.5 s restore) | PASS |
| 5 | Hosted CI | `gh pr checks 241`: DMG (macOS), Platform crate (macOS), Quality gate (Linux) all pass | PASS |

## Notes
- The ad-hoc quit-timing scenario was uncommitted and removed. One timing sample only; the longest legitimate quit is bounded by the 1.5 s restore plus the 2 s bound.
- I did not drive the GUI by hand through the MCP; all checks are scripted.
- Not re-run: unit tests and mutations (done in runs 2 and 3; the later commits are about quit and the key code).

## Failures
None.
