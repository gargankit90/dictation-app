# T3 test report, run 2

PR: #141 at aec34ce (includes the default-branch merge 51e8d87)
Result: PASS

Local on macOS, no VM. Re-checked what changed since run 1: the merge and the R1 fix (honest failed undo).

## New tests fail when the old behaviour is put back (temporary local edits, restored, never committed)

| Mutation | Result |
| --- | --- |
| Hotkey undo error dropped (`hotkey: None`) in `save_with` | `commands::tests::hotkey::a_failed_write_and_a_failed_switch_back_name_the_working_hotkey` fails |
| Login restore error dropped (`login: None`) | `commands::tests::launch_at_login::a_login_item_that_cannot_be_put_back_is_reported` fails |
| `get_settings` returns the file's hotkey, not the listening one | `a_failed_write_and_a_failed_switch_back_name_the_working_hotkey` fails |
| `with_active` made a no-op | `hotkey_settings.rs` `settings_show_the_hotkey_that_listens` and `a_failed_undo_names_the_hotkey_that_still_works` (line 500) fail |
| `HotkeyChange::undo` returns `Ok` when the switch back fails | `a_failed_undo_names_the_hotkey_that_still_works` and the commands.rs hotkey test fail |
| `restore_or_report` returns `Ok` | `launch::tests::a_failed_restore_says_where_the_login_item_was_left` and the commands.rs login test fail |

All pass on the unmodified branch (`a_failed_write_*` 4/4, pipeline 3/3).

## Run-1 checks after the merge (bug put back)

| Issue | Mutation | Result |
| --- | --- | --- |
| #113 | `KeyWrites::write` ignores an earlier write | 3 fail: `a_stuck_write_times_out_and_a_retry_starts_nothing_until_it_finishes`, `blocked_key_save_returns_actionable_outcome`, `late_key_save_cannot_overwrite_a_newer_choice` |
| #121 | `settings.validate()` removed from `save_with` | `invalid_settings_never_change_login_item` fails |
| #124 | panic hook not installed | `spawned_pipeline_logs_stay_private_across_rotation` fails |
| #112 | coalescing off (`if false`) in `UiSender::send` | **Inconclusive**: `a_stalled_overlay_stays_bounded_and_ends_on_the_current_phase` did not finish in over 10 minutes, so I killed it. A hang is not a pass, but it is not a clean assertion failure either. Run 1 saw a clean fail with a different mutation. Minor: this test hangs instead of failing when coalescing is off |

## Gate

`scripts/gate.sh` on aec34ce: `gate: all checks passed`, 298 s. Includes coverage, `test-guards.sh`, and `test-key-handling.sh` (39 passed).

## Failures

None.
