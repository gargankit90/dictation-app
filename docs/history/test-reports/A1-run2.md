# A1 test report, run 2

PR: #220 at 990cd63 (code); hosted CI checked on b8bebe8, which only adds `docs/history/reviews/A1-round2.md`
Result: PASS

Re-check of round 2 only: the Reviewer's blocker (events sent after opt-out)
and major (opt-out racing with Save).

| Check | How | Result |
| ----- | --- | ------ |
| New telemetry tests | `cargo nextest run -p dictation-telemetry`: 17 pass, incl. `a_batch_taken_before_opt_out_is_not_sent_after_it`, `a_batch_taken_before_off_and_on_is_not_sent_with_the_old_id`, `opt_out_cancels_a_request_that_is_under_way`, `a_saved_off_choice_sends_and_stores_nothing` | PASS |
| Shell tests | `cargo nextest run -p dictation-app`: all pass, incl. `commands::tests::sharing::a_save_cannot_turn_sharing_on_either` | PASS |
| Vitest | 123/123 pass. "turning it off uses its own command at once, without the Save button", "a Save pressed while the opt-out is pending does not undo it", "a later Save keeps the choice", "stays as it was and says why when the command fails" | PASS |
| Save no longer overwrites sharing | `Settings.tsx` `setSharing` calls `commands.setShareUsageEvents` only (no get/save round trip); Rust test shows Save can't turn sharing on or off | PASS |
| Hosted CI | Quality gate (Linux), Platform crate (macOS), DMG (macOS) all green | PASS |
| VM e2e | Not re-run: Settings UI markup is unchanged; only the handler behind the toggle changed | skipped |

## Mutation checks (opt-out drop), all restored with `git checkout`

1. Removed both guards (pre-send generation check and the cancel signal): 3 tests fail (`a_batch_taken_before_opt_out_is_not_sent_after_it`, `a_batch_taken_before_off_and_on_is_not_sent_with_the_old_id`, `opt_out_cancels_a_request_that_is_under_way`).
2. Removed only the cancel signal (`send_replace`): `opt_out_cancels_a_request_that_is_under_way` fails.
3. Removed only the pre-send generation check: all 17 still pass (see minor).

## Minor (no blocker)

The pre-send generation check in `send_batch` is not covered on its own: with
the cancel signal in place, the `select!` drops the request anyway, so no test
fails when the check alone is removed. The behaviour is guarded; the check is
defence in depth. Worth a tech-debt note, not a block.

## Failures

None.
