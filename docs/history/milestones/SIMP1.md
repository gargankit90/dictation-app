# SIMP1: Simplification pass for 0.2.0 (after Q2 and Q4 merge)

Owner direction, 2026-11-20: "We want 0.2.0 release to be a very simple product and release." Vocet has
no users: no migrations or compat. Sources (read them; line numbers are on the branches they name, re-check
on current main): `/Users/gauri/vocet-screens/u9/simplify-audit.md` (Claude),
`/Users/gauri/vocet-screens/u9/gemini-simplify-report.md`, `/Users/gauri/vocet-screens/u9/codex-simplify-report.md`.

## Owner decisions (2026-11-20)
- **Keep unchanged:** the animated Welcome demo; retry with jitter; **History** as it is; **Help**
  (Troubleshoot + Report a problem) as it is; **Sentry** crash reporting as it is (don't merge it into
  PostHog's path); the OpenRouter paste-key fallback and the real-OpenRouter e2e scenario.
- **Launch at login:** keep the feature, simplify the code (plain blocking call off the main thread,
  switch disabled while saving; drop the dedicated worker/coalescing machinery).
- **"Try it" in onboarding:** a plain practice box: the words to say, a text box the dictation pastes into,
  and Done. Remove stage polling, the paste-grace timer, practice copy recovery and the outcome commands
  (`dictation_progress`, `practice_words`, `copy_practice_words`, `practice_paste_missed`,
  `set_try_outcome` or whatever remains of them) and their telemetry.
- **Remove "Test microphone"**: the Settings test and onboarding's live permission meter, the second
  capture lifecycle (`crates/app/src/mic_test.rs`) and its three commands. Keep the permission prompt and
  the overlay's level meter / "No sound" state.
- **Typed errors**: replace `ErrorCode::from_reason` (substring matching on user-facing text) and
  `KeyTestKind`/`describe` with typed codes set where the error happens; one table maps each code to a
  plain sentence, a fix action and the telemetry category; unify `tray::Fix` and `diagnostics::FixAction`.
  A test enumerates every code → non-empty friendly sentence.

## Safe deletions and merges (no product change)
1. Remove the language picker end to end (`LanguagePicker.tsx`, `languages.ts`, `set_language`,
   `Settings.language`/`local_language`, language telemetry); 0.2.0 is English-only. Pin English at the
   engine boundary only if the eval shows it helps; otherwise send no language.
2. Onboarding: delete the progress file / `SetupProgress` / `ALREADY_SET_UP` and the micro-commands; keep
   one `onboarding_completed` flag plus React state; keep reopening setup after permission loss.
3. Delete the runtime rename notice (`rename.rs`, `launch_notice`, dismissal state) and the finished
   rename/certificate-change update e2e scenarios.
4. Delete the unused layout harness (`src/onboarding/layoutHarness.tsx`, `vite.layout.config.ts`, its
   scenario) and the three screenshot-walkthrough e2e scenarios (move any unique behaviour assertion into
   functional scenarios first).
5. Dead code: `save_settings`, `set_window_page`/`WindowPage` title sniffing, `has_api_key` (use
   `saved_key_hint`), unused `Link` variants, unreachable non-compact `HotkeyRecorder` branch, unused
   `MockTranscriber` builders, `stand_ins.rs`, write-only `history::Entry.engine` (History UI unchanged).
6. Leftovers of the old network stack still on the Q2/Q3 branches: `upload_limits.rs`, `httpdate`,
   `Redirected(u16)`, `SILENCE_PHRASES`/`transcribe_detailed` if only Whisper needed them; keep the
   transport hardening that has tests (retries, timeouts, no-redirect, long upload).
7. Remove the one-variant `Provider` enum (settings, bindings, commands, telemetry, React tests) and the
   non-editable `Settings.model`/`local_model`; collapse `use_cloud_engine`/`use_local_model`/
   `choose_local_model` into one `set_transcription(Cloud|Local)`. Trim `cloud_models.rs` to two constants.
8. Telemetry: drop dead `SetupProblem` variants (key-test failures, paste missed, practice skipped,
   download cancelled) and `SettingName::{Provider,Model,Language}`; drop `UpdateInstalled` (AppOpened
   carries the version). Update docs/privacy.md to match.
9. Tray: remove download percentages and the 1 Hz settings.json polling (keep error/fix rows and Copy last;
   History and Help items stay).
10. Duplicates: one bounded Keychain helper (`key_writes.rs` + `KeyReads`); one copy each of
    percent-encode, base64/base64url, loopback check, `sw_vers`, keycap labels, model display names; one
    core enum each for permission state and progress view; generated overlay payload types.
11. `crates/app/tests`: one shared helper module; fold `*_edges` files into their base files.
12. Single-instance socket: keep it; open it only in debug builds (`#[cfg(debug_assertions)]` around
    `listen_for_later_launches`) so release builds have no socket. No rewrite before 0.2.0.
13. Tooling tests: trim `scripts/test-vm-lock.sh` and `scripts/test-release.sh` to the cases that protect a
    real failure; keep the gate receipts. Never touch guard files or required checks.
14. Delete `.github/workflows/dmg.yml` unless the release process uses it; dedupe macOS CI passes only if
    targets/features/coverage are identical (keep required job names).
15. Docs: move implemented `docs/design/specs/U*.md` and U11 mocks into `docs/history/` (low priority);
    fix `docs/TESTERS.md` (no Groq/OpenAI, no Parakeet/Qwen3, Connect OpenRouter, OrukQwen + Gemma).

16. **Test removals** from the three test audits (`/Users/gauri/vocet-screens/u9/test-audit.md`,
    `test-audit-codex.md`, `test-audit-gemini.md`): remove what at least two agree on, e.g. the Whisper
    `no_speech_prob` silence filter and its tests (OpenRouter replies never carry it), overlapping
    command-list tests in `commands.rs`, tests of our own fakes (`fakes_contract.rs`, `mock_edges.rs`),
    duplicated Retry-After variants, LanguagePicker/Try-it/mic-test tests, and shrink Onboarding/Settings
    test files; e2e from 58 to about 42 scenarios (delete `error_bad_key` and `update_keychain_control` if
    covered elsewhere; merge overlapping flows). Move shared shell helpers out of
    `e2e_onboarding_screenshots.sh` and `e2e_settings_save_reopen.sh` before deleting them; removing the
    Provider enum breaks the `provider=openrouter` log check in 5 scenarios: update them. Note:
    `test-vm-lock.sh` lives in `tests/e2e/`, not `scripts/`. Keep `e2e_real_openrouter`.

## Acceptance criteria
1. [auto] Gate passes; e2e suite passes with the remaining scenarios (including e2e_real_openrouter skip
   path and the onboarding/tray functional scenarios).
2. [auto] PR notes list lines removed per item and anything skipped with the reason.
3. [auto] Every user-visible error has a typed code and a friendly sentence (enumeration test).
4. [manual] Lead: VM e2e on the merge; look at onboarding (Try it), Settings and error messages.

Developer: Opus 5.5. Two PRs: (A) items 1–5, 8, 9, 11, 13–15 and the Try-it/mic removals; (B) typed
errors, items 6, 7, 10, 12, launch-at-login. Time box: 1 day per PR.
