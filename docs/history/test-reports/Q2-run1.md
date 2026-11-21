# MQ2 test report, run 1

PR: #336 at 8aedebf (plus one test-fixture fix by the Tester, below)
Result: FAIL (one e2e scenario, `e2e_update_from_dictation_app`, is stale; criterion 5)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Unit tests: guard, prompt v4 byte for byte, settings fallback, catalog defaults | `cleanup::tests::criterion_1_the_prompt_is_v4_byte_for_byte`, `the_prompt_carries_v4s_rules`, `settings::tests::*` (groq/openai/unknown values load, other fields kept), `cloud_models::tests::*` | PASS | All in the gate's nextest run (1511 tests, 0 failed). Voxtral fallback absent by owner decision (PR body), so not tested. |
| 2 | Pipeline with fakes: cleaned text pasted; timeout, error, guard rejection paste raw; timed-out cleanup cancelled, next dictation not delayed | `crates/app/tests/cleanup_q2.rs` (5 tests) | PASS | |
| 3 | OpenRouter client against a local fake: filters, attribution headers, typed errors, key never leaked | `crates/transcribe/tests/openrouter_q2.rs` (7 tests), `openrouter*.rs` | PASS | Speech to Voxtral fallback not implemented: owner decision, see PR body. |
| 4 | No Groq/OpenAI code paths, settings, UI copy | `grep -rniE 'groq\|openai'` over the tree | PASS | Remaining hits are: model ids/display names `openai/gpt-4o-mini-transcribe` (OpenAI is the model vendor, reached through OpenRouter), privacy copy that says audio goes to OpenAI via OpenRouter, the settings-fallback tests, a catalog comment, the `exp/groq-cleanup` provenance comment, `e2e_update_*` old-0.1.0 scenarios, and the README caption about the old recording. No Groq/OpenAI provider, key account or settings value in `crates/*/src`, `src-tauri/src`, `src`. |
| 5 | `scripts/gate.sh` passes; e2e scenarios updated | gate; `E2E_VM=dictation-vm-2 tests/e2e/run.sh` | FAIL | Gate passes (full run). E2E: see below; `e2e_update_from_dictation_app` fails and was not updated for the no-migration decision. |
| 6 | Real-key VM check | — | Not tested | The Lead's. |

Owner rule check (no migrations, an old settings file with unknown values still loads): PASS. `settings.rs` tests load `"groq"`, `"openai"`, an unknown provider and an unknown model with every other field kept; `config_edges.rs` keeps unknown future fields. Old Keychain items are not read.

## Gate
`scripts/gate.sh` (full, mode=full): all checks passed, including fmt, prettier, clippy, deny, machete, nextest (1511 tests), vitest, tsc, eslint, typos and the e2e helper tests. Coverage is not a separate gate step here; no per-crate figures were measured.

## E2E (dictation-vm-2, build of 8aedebf, `--exclude e2e_real_openrouter`)
Run 1 (stopped by the 30-minute background limit after `e2e_accessibility_revoked` started): 39 PASS, 4 FAIL, 1 SKIP (optional `e2e_update_keychain_control`).

- `e2e_onboarding_round2`: FAIL once, **PASS on rerun** (the Continue press did not advance the first time). Flaky; no product defect seen.
- `e2e_launch_at_login`: FAIL once ("TextEdit didn't come to the front" after the reboot), **PASS on rerun**. Flaky, environment.
- `e2e_unicode_multiline_selection_paste`: FAIL twice, **fixed by the Tester**: the new loopback fake `tests/e2e/fixtures/local-provider.pl` decoded the request JSON as characters (`JSON::PP->new->decode`, no `->utf8`), so the cleanup echo double-encoded every non-ASCII character (pasted `\u{f0}\u{9f}\u{98}\u{80}` instead of an emoji). Product code was right. With `->utf8` the scenario passes. Committed as `test(e2e): decode the fake provider's request as UTF-8`.
- `e2e_update_from_dictation_app`: **FAIL, not flaky.** Still written for the Groq build: it expects the old build's key (account `groq-api-key`) to survive the update (`provider=groq has_key=true` log patterns at lines 203 and 226, `tr_key_in_keychain` for `groq-api-key`, the "Choose Always Allow" notice). By design the new build never reads the old Keychain item, so after the update the log says `provider=openrouter ... has_key=false`, no notice appears, and 4 checks fail (`the notice is there`, `its engine has the same settings`, `... reads a key it saved itself`, `the key is in the keychain`). The scenario needs rewriting to the no-migration behaviour: after the update the settings file loads (this part passes: "the settings file is as it was", engine reads `provider=openrouter`), the old key is left alone, and the user enters an OpenRouter key.

Run 2 (the scenarios run 1 did not reach): `e2e_accessibility_revoked`, `e2e_first_paste_after_launch`, `e2e_history`, `e2e_local_model`, `e2e_notarized_dmg_first_launch` PASS.
**Not run** (time box; the run was cut off): `e2e_onboarding_screenshots`, `e2e_quit_right_after_paste`, `e2e_release_build_smoke`, `e2e_setup_no_permissions`, `e2e_setup_own_key_no_permissions`, `e2e_setup_redesign_screens`, `e2e_tray_menu_screenshots`, `e2e_update_privileged_install`, `e2e_update_to_new_certificate`. `e2e_real_openrouter` is the Lead's.

## Untested behaviour found
- No test or implementation of the speech fallback to Voxtral on no-route/5xx: owner decision recorded in the PR body, not a defect.
- Coverage figures per crate were not measured (no step in the gate prints them).

## Failures
`e2e_update_from_dictation_app`: reproduce with `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_update_from_dictation_app`. Expected (per the spec's "old Keychain items are left alone"): the scenario asserts the new behaviour. Actual: it asserts the old Groq behaviour (key kept, notice shown), so it fails at the four checks above. Log lines: `engine settings provider=openrouter model=openai/gpt-4o-mini-transcribe language=Some("en") has_key=false`. No keys or transcripts are in this report.
