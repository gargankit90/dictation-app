# U7 test report, run 1

PR: #267 at e5e0902
Result: PASS (no failures found; some manual checks were not completed, listed below)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | `history_limit`, default 25, migration | Developer's unit tests; popup showed 25 on a fresh launch (VM) | PASS | Not re-run by me; CI covers it |
| 2 | Entry per dictation, 0600, `pasted:false`, limit | `e2e_history` PASS; VM: file mode 600, 3 real dictations stored | PASS | Entry fields are id, text, created_at, engine, pasted only |
| 3 | Lowering the limit, Off, Clear | `e2e_history` (Clear with count, Cancel, Delete; file removed) PASS | PARTIAL | Lowering and Off through the popup NOT driven in the VM (see "Not verified") |
| 4 | Privacy | VM: mode 0600, no app/window field in the file; grep of the app log for 5 dictated or seeded phrases: 0 hits (69 log lines); `e2e_history` log check PASS | PASS | Password-field refusal not run in the VM |
| 5 | List, time, 2-line preview, chip, Copy, Delete, empty state | `e2e_history`; screenshots | PASS | Copy/Delete are hidden at rest, appear on keyboard focus (screenshots) |
| 6 | Menu History…, Copy last | `e2e_history`, `e2e_tray_menu_screenshots` PASS | PASS | Memory-only when Off covered by `e2e_tray_menu_screenshots` |
| 7 | docs/privacy.md | not read in depth | NOT CHECKED | |

## E2E (dictation-vm-2, run.sh)
`e2e_history`, `e2e_tray_menu_screenshots`, `e2e_settings_save_reopen`: 3 passed, 0 failed, 0 skipped.

## Manual checks in the VM
- Seeded 25 entries (made-up text, one Not pasted, multi-line text). Rows show time ("2 min ago", "Today 19:14", "Yesterday 20:44"), a two-line preview, and the amber "Not pasted" chip.
- Keyboard focus: after Tab, Copy and Delete appear on the focused row with a visible focus ring. Screenshots: `U7-run1/focus-light.png`, `U7-run1/focus-dark.png`. Copy/Delete have accessible names ("Copy dictation from 2 min ago").
- `docs`: `src/settings.css:754` shows `.history-row:hover .history-actions` and `:focus-within` share one rule, so hover matches the focus screenshots.

## Not verified (time box / tooling)
- **Hover** could not be shown: synthetic mouse moves (CGEventPost over ssh, and the driver's `move_cursor`) did not trigger `:hover`, and the first attempts were covered by e2e Terminal windows. Only the CSS rule was checked.
- **Popup Off/10/25/50/100**: the driver's element tokens went stale between CLI calls and desktop clicks did not land, so I did not drive the popup. Lowering with a count confirmation and Off still rest on the Developer's Rust and vitest tests.
- **Password-field refusal not stored**: not run in the VM (`e2e_secure_input_blocks_paste` was not in the requested set). Developer's test `a_paste_refused_by_secure_input_is_never_kept` covers it.
- Menu showing no dictation text: only through `e2e_tray_menu_screenshots`.

## Untested behaviour found
- The engine is stored as "Groq" for the mock transcriber in e2e runs (cosmetic, only with `--transcriber mock`).
- An Automation prompt from my own Terminal-closing script blocked the VM screen until I stopped that dialog's process; not an app issue.

## Failures
None.
