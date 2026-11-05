# U7 test report, run 2

PR: #267, tested at 054bda2 (round 2 head when the run started)
Result: PASS

The branch has since moved to 2b988fc (a merge of `main` and a test-only commit, "finish reaches the inserter through Recording"). I did not re-run on it.

Re-check of what round 2 changed (damaged history file handling, non-blocking tray and Settings reads, extended `e2e_history`). Everything ran in `dictation-vm-2` through the runner's lock.

| # | Check | Test(s) | Result | Notes |
| - | ----- | ------- | ------ | ----- |
| 1 | `e2e_history` passes | `run.sh --no-build e2e_history` | PASS | Includes the new limit-popup steps (10 / Cancel / 25 / Off) and the damaged-file steps. |
| 2 | `e2e_tray_menu_screenshots` passes | `run.sh --no-build e2e_tray_menu_screenshots` | PASS | Light and dark, all states. |
| 3 | Limit popup 50 and 100 work | scratch scenario, not committed (popup driven by keyboard through AX) | PASS | 25 -> 100 saved `history_limit` 100 with no question; 100 -> 50 saved 50 and kept the 25 entries; 60 entries under limit 100, pick 50 asked "delete the 10 oldest dictations?", Delete left exactly 50 entries. |
| 4 | Damaged file: notice in Settings, not overwritten by the next dictation, Clear history removes it | `e2e_history` (damaged-file block) | PASS | The file `this file is damaged` was byte-identical after a dictation. Settings showed "saved history can't be read" plus this run's dictation. Clear history named the unreadable file, Delete removed it and the notice went. |
| 5 | Menu opens instantly while dictating | scratch scenario: hold the hotkey (Recording), open the tray menu with 100 entries stored | PASS | The menu opened and listed History…. It took 1-2 s end to end, including the ssh round trip and the AX call, so this shows "no stall", not a precise latency. |

## Gate
Not re-run; CI and the pre-push hook run it. No product code was touched.

## Untested behaviour found
- `pickPopup` in `tests/e2e/fixtures/ax.js` types only the first character of the option, so `history_pick 100` selects **10** (type-select takes the first match). It is a test-helper gap, not an app bug: the app saved and asked correctly once the right item was chosen. `history_pick 50` works. I checked 100 with a local, uncommitted fix that types every character (`POPUP_KEYS` gets `0: 29`). Worth a follow-up in the e2e helper.
- A seeded file with more entries than the limit (120 at limit 25) is trimmed to 25 when the app loads. That is sensible; a future test should seed under the limit.

## Failures
None in the app. My first two scratch attempts failed only because of my own script (raising the limit asks nothing, and the `100` helper gap above). VM-2 was also busy with other runs for about 50 minutes, so this took longer than the time box.
