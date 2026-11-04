# U6 test report, run 1

PR: #262 at 35d741e (code identical to a46d812; the only later commit is the Reviewer's `docs/history/reviews/U6-round1.md`)
Result: PASS (criteria 7 and 7b are pending U1/U5, as the spec says; they are not a fail)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | 720×520, sidebar, ⌘,, remembers section, keyboard, names | vitest `criterion_u6_1 *`, `App.test`, Rust `window::tests::criterion_u6_1_*`; VM | PASS | VM: window 720×521, six sections; Down/Up/Home/End move between sections (7 key presses checked); Tab and Space operate the login switch; the section was remembered across close/reopen and relaunch. Window title is still "dictation-app Settings" (U1 renames it; the PR says so). |
| 2 | No Save; each control persists; reopen shows values | vitest `criterion_u6_2 *`, Rust `criterion_2_*`; VM `e2e_settings_save_reopen`; VM by hand | PASS | By hand: language Hindi, events off, login off: all kept after close/reopen and after a full relaunch (also in light mode). |
| 3 | Login switch: Applying…, never blocks, reverts on failure | vitest `criterion_u6_3 *`; VM `e2e_launch_at_login`; VM by hand | PASS | "Applying…" shown (`general-applying-dark.jpg`); while it ran I switched to Privacy and back and the window stayed responsive; final state matched. The failure/revert path is covered by vitest only (the VM's login item can't be made to fail). |
| 4 | One Active; state machine; Use this only when ready | `modelRow.test.ts` (10), vitest `criterion_u6_4 *`, Rust `criterion_4_*`; VM `e2e_local_model`; VM by hand | PASS | See "Manual: local model" below. |
| 5 | Test & Save: test then Keychain; separate Keychain error; key never shown | vitest `criterion_u6_5 *`, Rust `criterion_5_*`; VM `e2e_settings_key_provider_recovery` | PASS | Scenario: wrong key rejected and not saved; right key gives "✓ Connected · saved in Keychain"; dictation recovers in the same process and after relaunch; key in no file. By hand: a bad Groq key gives the inline "Groq rejected this API key…" (`speech-bad-key-dark.jpg`), field outlined red, nothing saved. The good-key path against a mock provider ran only through the scenario. |
| 6 | Test microphone: live meter 5 s, "No sound" if silent | vitest `criterion_u6_6 *`, `mic_test::tests::*`; VM by hand | PASS (partly) | The VM has an "Apple Virtual Sound Device": the bars animate with "Hearing you.", then after about 7 s "✓ The microphone works." (`dictation-mic-test-dark.jpg`). I could not make the VM device silent, so "No sound" was checked only by the fake-timer vitest. |
| 7 | Help → Show log file / Run setup again | vitest `criterion_u6_7 *` | PENDING | Waits for U5 (#248); Help shows Troubleshoot… and Report a problem… for now. Not a fail. |
| 7b | No "Settings → Transcription" | `preferences::tests::criterion_u6_7b_*` | PENDING | Guard test passes on main; fails by design once U1 merges. Not a fail. |
| 7c | Typed fallback removed | vitest `criterion_u6_7c *`; VM | PASS | No "Or type it" in Dictation (`dictation-light.jpg`). |
| 8 | Tests migrated; two VM scenarios pass | vitest 205/205; VM | PASS | |
| 9 | [manual] light/dark, download to Active, bad key, login | VM, `U6-screens/` | PASS | See below. |

## Gate
- vitest: 12 files, 205 tests passed.
- `cargo nextest run --workspace`: 1282 passed, 2 skipped, 0 failed.
- Mutation check (instant apply): in `src-tauri/src/commands.rs` `write_change` I made the change apply to `Settings::default()` instead of the stored settings (a whole-object write from stale state). Two tests failed: `preferences::tests::criterion_2_language_saves_on_its_own_and_keeps_every_other_field` (left `("RightAlt", true, false)`, right `("F6", false, true)`) and `commands::tests::recorded_hotkey::writing_the_key_changes_only_the_key`. Restored; `git status` clean.
- VM scenarios on `dictation-vm-2` (`E2E_VM=dictation-vm-2 tests/e2e/run.sh ...`): **6 passed, 0 failed, 0 skipped**: `e2e_settings_save_reopen`, `e2e_settings_key_provider_recovery`, `e2e_launch_at_login`, `e2e_local_model` (real 477 MB download), `e2e_dmg_install`, `e2e_hotkey_recorder`. The three the Developer didn't run are in this set.
- Hosted CI at the time of writing: DMG (macOS) pass, Platform crate (macOS) pass; Quality gate (Linux) was still running, see the PR comment.

## Manual: local model (the check the Developer could not do)
Default model Parakeet Unified 0.6B, in the VM, dark mode:
1. Delete from the "…" menu: chip "477 MB" and Download (`speech-not-downloaded-477mb-dark.jpg`).
2. Download: "↓ 0 of 477 MB", progress bar, Cancel (`speech-downloading-dark.jpg`).
3. Cancel: "Paused at 221 of 477 MB", Retry and "…" (`speech-paused-after-cancel-dark.jpg`).
4. Retry: completes, "Downloaded" and Use this.
5. Use this: "✓ Active" (green); the own-key row drops to "Use this", so exactly one is Active (`speech-local-active-dark.jpg`).
6. Own key, Use this: own key "✓ Active" with "Saved", Parakeet back to "Downloaded".

## Light and dark screenshots
`docs/history/test-reports/U6-screens/`: General, Dictation, Speech, Privacy, Help, About in light (`*-light.jpg`, after a relaunch with `defaults delete -g AppleInterfaceStyle`) and dark (`*-dark.jpg`), plus the states above. Taken on 35d741e (the latest code), driven through the cua-vm-2 window screenshots because `screencapture` over ssh can't read windows. No e2e Terminal windows were open. About shows "Vocet, Version 0.1.0", MIT, GitHub and credits. Privacy "What's sent" expands with the notice link (`privacy-whats-sent-dark.jpg`).

## Untested behaviour found (none blocking)
- A shortcut change by hand in the GUI wasn't repeated: synthetic keys don't reach the global recorder. `e2e_hotkey_recorder` and `e2e_settings_save_reopen` cover it and pass.
- Cancelling a download shows "Paused at N of M MB" with Retry, not "Download failed" or "Paused: no connection". Reasonable (the partial file is kept and resumes), but the spec's wording is for no connection.
- With the mock engine and no key saved, the own-key row shows "✓ Active" and "No key saved yet." A real user on the cloud engine with no key would see the same.
- A "Login Item Added" notification appeared on each app launch in the VM after the login switch had been on in an earlier scenario; this is S1 behaviour, not U6.
- The Keychain-failure message and the login-switch revert are covered by vitest and Rust only.

## Failures
None.
