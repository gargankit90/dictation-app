# Q4 test report, run 1

PR: #337 at 1f15861
Result: FAIL (two e2e scenarios fail on the PR's behaviour change; everything else passes)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | OrukQwen is the only local speech model, verified hash/size | `dictation-core local_models` (7 tests); `shasum -a 256` of the owner's upload file | PASS | file hash `057471662f06…b273bb` matches the catalogue |
| 2 | Fake runner: cleaned text, timeout/error/guard → raw, cancel doesn't block | `dictation-transcribe llama` (14), `dictation-app cleanup` (8, incl. pastes cleaned text, raw text on helper timeout, helper restarted after it dies) | PASS | |
| 3 | Parakeet/Qwen3-ASR gone; unknown id loads | grep over `crates src src-tauri site docs/agents tests`; `a_settings_file_with_a_removed_model_still_loads` | PASS | remaining hits are fallback tests, old-settings fixtures, an OrukQwen provenance comment |
| 4 | Gate (incl. licences, macOS build) | `scripts/gate.sh` | PASS | see Gate |
| Local is opt-in | e2e onboarding scenarios | PASS (scenario passes through the key card by default) | see Failures for round2 |
| Helper: loopback, key, no orphan | real helper on the host (below) | PASS | |

## Real helper on this Mac (host, not VM)
Real `llama-server` (`src-tauri/binaries`) + real Gemma 4 E2B GGUF:
- `crates/transcribe/tests/local_cleanup.rs` passes: ready in 19.8 s cold, first clean 523 ms, second 146 ms.
- Launch arguments seen in `ps`: `--host 127.0.0.1 --port <random> … --no-webui --no-slots --log-disable`; no key in argv. `lsof`: only `127.0.0.1:<port>` listening.
- After a normal exit: no `llama-server` left in `ps`.
- `kill -9` of the app-side process while the helper was up: the helper was gone within 3 s (no orphan).
Not done in the VM: the app-level "killed on switch / quit" checks (covered by the `cleanup` unit tests and the supervisor's `stop_kills…` / `closing_the_pipe_kills…` tests, which pass), and the full-app dictation with a real Gemma (criterion 5, the Lead's).

## Gate
`scripts/gate.sh` full: pass on the second run. The first run failed once in
`dictation-platform audio::macos::tests::exact_clip_length_and_alignment_across_supported_rates`
(16173 vs 16333 frames) while two e2e runs loaded the Mac; it passes alone (twice) and the PR does not touch `crates/platform`. Flaky, not Q4.
Coverage: not separately measured (the gate's thresholds passed).

## E2E (dictation-vm, stand-in model server serving the OrukQwen file; `e2e_local_model` excluded because it uses the real Hugging Face URL, which 404s; real_openrouter and real_mic excluded)
Run 1 (full, stopped at `e2e_accessibility_revoked` after ~95 min because of the time box): 33 PASS, 5 FAIL before the stop: `e2e_silence`, `e2e_onboarding_round2`, `e2e_cancel_and_busy_presses`, `e2e_focus_switch_during_transcription`, `e2e_onboarding_screenshots`.
Rerun of those five: `e2e_silence`, `e2e_cancel_and_busy_presses`, `e2e_focus_switch_during_transcription` PASS (timing flakes on a loaded VM). `e2e_onboarding_round2` and `e2e_onboarding_screenshots` FAIL again.
Scenarios after `e2e_accessibility_revoked` in the order were not run.

## Untested behaviour found
- No e2e checks "no `llama-server` after switch to cloud / after Quit" in the VM (needs Gemma in the VM; the stand-in fails Gemma's checksum by design). Covered by unit tests and my host checks; a follow-up scenario is worth a `tech-debt` issue once Gemma can be served.

## Failures
1. `e2e_onboarding_screenshots` (stale expectation, owned by Q4's e2e edits): after pressing the Hindi radio the scenario waits for `30 languages · 590 MB` (`tests/e2e/scenarios/e2e_onboarding_screenshots.sh:76`). OrukQwen is English-only, so the screen now shows `Hindi needs your own key. Vocet's on-device models don't cover it yet.` (`src/onboarding/SpeechStep.tsx:29`). Fails for light and dark. Fix: expect that text (and drop the Hindi on-device size).
2. `e2e_onboarding_round2`: `FAIL: the choice is saved before any quit`. Steps: fresh state, Get started, `radio-press "Use my own OpenRouter key"`; the progress file then says `"speech_choice":null` after 12 s (expected `own_key`). The key card is now the preselected choice (`picked` starts null; owner decision in the PR), so the radio is already on and pressing it apparently does not call `pick()`/`set_speech_choice`. Failed in both runs here. In Q2 run 2 this scenario was flaky; here it is deterministic. Product question for the Developer: if quitting during Speech without clicking anything and coming back is fine (the default is the key card anyway), update the scenario to press the on-device card first and then the key card; if the saved choice matters, `set_speech_choice("own_key")` should be sent for the default too.
