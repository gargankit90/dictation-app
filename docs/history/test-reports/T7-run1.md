# T7 test report, run 1

PR: #192 at 09b8213
Result: PASS

VM: `dictation-vm-2` only through `tests/e2e/run.sh` (waited on a live lock
twice, never cleared it). Made-up keys only.

| # | Criterion | Test(s) | Result | Notes |
| --- | --- | --- | --- | --- |
| 1 | Root cause found and explained | On `origin/main`, `textedit_set()` is defined in `scenarios/_provider_helpers.sh:208` (base64) and in `scenarios/e2e_hotkey_shortcuts.sh:28` (plain text: `set text of document 1 to "$1"`). `run.sh:23` sources `scenarios/*.sh` in glob order into one shell, and `_provider_helpers.sh` sorts before `e2e_hotkey_shortcuts.sh`. Sourcing a copy of main's `tests/e2e` and running `declare -f textedit_set` printed the plain-text `vm_osascript` version: the later definition wins, so the unicode scenario seeded raw base64. | PASS | A second collision, `frontmost_app` (`_provider_helpers.sh:256` vs `e2e_overlay_focus.sh:15`), also exists on main; the PR renames the overlay one to `overlay_frontmost_app`. |
| 2 | Seeding fixed | `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_unicode_multiline_selection_paste`, twice | PASS, PASS | 17 checks each. The three "a document with ..." setup checks now include the read-back (`textedit_seeded`) and pass. |
| 3 | Setup reports itself | Fixtures `seed_broken` and `seed_right` (`tests/e2e/test-runner.sh`: 99 passed, 0 failed). Mutation: `textedit_set_b64` made to skip the base64 decode. | PASS | The scenario failed at the first setup: `TextEdit holds [TGluZSBv...]`, `setup: document not as expected`, `FAIL: a document with the caret inside its text`. No dictation or paste check ran. Restored. |
| 4 | Scenario passes; other seeding scenarios pass | One `run.sh` invocation with `e2e_hotkey_shortcuts e2e_overlay_focus e2e_unicode_multiline_selection_paste` | PASS | 3 passed, 0 failed (46 checks), the order in which the old shadowing happened. The criterion says 3 runs on `dictation-vm`; as tasked, run on `dictation-vm-2`: 3 runs of the unicode scenario in all (2 alone, 1 in the combo), all PASS. |
| 5 | No check weakened | `git diff origin/main...HEAD` on the scenario | PASS | No `-`/`+` line touches a paste assertion (`textedit_is` expectations, `phases_since`, clipboard, `provider_requests_are 3`). Changes are only seeding, `unicode_setup` and `unicode_end`. |
| 6 | Hosted CI | `gh pr checks 192` | PASS | Platform crate (macOS) pass; Quality gate (Linux) pass. |

## Mutations (all restored, nothing committed)

- Duplicate guard: appended `textedit_set_b64() { :; }` to
  `e2e_overlay_focus.sh`. `test-runner.sh` failed with
  `no shell function is defined in two e2e files (found: textedit_set_b64)`.
  The clean runner passes 99.
- Broken seeding: see criterion 3.

## Notes

- The duplicate guard matches only `name() {` at the start of a line. Not
  blocking.
