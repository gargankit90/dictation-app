# E1 test report, run 1

PR: #225 at fe890d5 (the branch head is now 1bae4cc, which only adds `docs/history/reviews/E1-round1.md`; no code changed)
Result: FAIL

Two product findings (the first is the Reviewer's Major 1, the second confirms their Major 2) and a regression in six existing e2e scenarios. Criteria 3, 4, 5, 6 and the VM checks of criterion 7 pass.

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Codes, one table, next step shown, reuse A1 type | `cargo nextest run -p dictation-core -p dictation-ports -p dictation-pipeline`: 491 passed (code table, message→code, code→`ErrorType`); VM overlay and tray | **FAIL** | The table has 17 codes, tests cover every one, and the codes sit under A1's `ErrorType`. But the overlay and tray show the old `error.to_string()` text plus the code (`crates/app/src/overlay.rs:96`, `crates/app/src/tray.rs:103,288`); the table's message and next step are used only by Troubleshoot. In the VM one error reads differently in the two places: overlay "Microphone is off. Allow it in Privacy & Security, Microphone, then relaunch from the menu bar." vs Troubleshoot "Microphone is off for dictation-app. Allow it in Privacy & Security, then relaunch from the menu bar." Errors such as a model-load or paste failure get no table next step on the overlay. Same as Reviewer Major 1. |
| 2 | Troubleshoot checks, off the event loop, no audio, provider only on the button | `cargo test -p dictation-app` (215 passed); code read; VM | **FAIL** (engine check) | Permission and keychain reads, save and browser open run in `spawn_blocking`; `run_checks` records nothing and makes no network call; `test_connection` is the only provider call, only from its button, and returns early for the local engine. There is no mic test button, so nothing records audio. But "engine ready (local model present **and loadable**)" is a file-size check: `ModelStore::status` → `ready_path` compares file length ("Cheap (no hashing)", `crates/transcribe/src/models.rs:258-277`), so a present model that fails to load shows ✓ Ready and `engine_ready: yes`. Same as Reviewer Major 2; `docs/privacy.md` says "downloaded and verified", which the check does not do. |
| 3 | Report from allowlisted fields; Save / Copy / GitHub; nothing uploaded | `cargo test -p dictation-core report`; VM | PASS | `report::render` takes typed fields only; text fields go through `field()` (plain characters, ≤ 40, no key shape, else `unknown`). Report read in the VM: see below. **Save report…** opened the real native Save dialog and the app said "Saved as dictation-app-report.txt." (the file's bytes could not be read over ssh: the VM's Documents folder is TCC-protected; they come from the same `shown_report` string as Copy). **Copy**: `pbpaste` in the VM returned exactly the text on screen. **Open a GitHub issue** opened Safari at `github.com/gargankit90/dictation-app/issues/new?title=Problem%20report&body=<the report>` (the VM is signed out, so GitHub redirected to sign-in with that URL as `return_to`); nothing was submitted. The only network use in the report path is `reveal::open_url` (`open`, https only, unit-tested); there is no HTTP client in `core::report` or `core::diagnostics`. |
| 4 | Planted-secrets test, mutation-checked | `diagnostics::tests::criterion_4_planted_secrets_and_text_never_reach_the_report` | PASS | Plants a key, a registered secret, transcript, clipboard, app name, home path and a log line in settings, error reasons and environment strings, and asserts none appear, plus the line shape. Mutation 1: added `hotkey_raw: {input.hotkey}` to `report::render` → the test FAILED (`"gsk_PLANTED_KEY_0123456789abcdef" leaked`); restored, passes. Mutation 2: fed `settings.language` (the transcript text) into the hotkey slot in `report_text` → the test still passes, because `field()` turns over-long or non-plain text into `unknown`: the guard working, not a hole. Both reverted (`git status` clean). |
| 5 | Separate from analytics; doc | `docs/privacy.md` read against the code | PASS (one wording issue) | The new section lists exactly the fields `report::render` emits, says the report is never sent, that Test connection is the one network call, and that the analytics switch is independent; all match the code. "downloaded and verified" overstates the engine check (criterion 2). |
| 6 | Gate, vitest, CI | `npx vitest run` 148/148; nextest (core, ports, pipeline) 491 passed; `cargo test -p dictation-app` 215/215; `cargo clippy -p dictation-core -p dictation-app --all-targets -D warnings` clean; `gh pr checks 225` | PASS | DMG (macOS) and Platform crate (macOS) passed on the current head; Quality gate (Linux) was still running when I looked. I did not run the whole `scripts/gate.sh` or coverage. |
| 7 | [manual] VM | `E2E_VM=dictation-vm-2 tests/e2e/run.sh` scenarios + hand-driven `cua-vm-2` | PASS for the Developer's checklist; see Failure A | Details next. |

## Criterion 7, in `dictation-vm-2` (this branch's debug build, installed by `run.sh`)

- Microphone denied inside the VM only (`tccutil reset Microphone com.dictationapp.desktop` over ssh to the VM, then **Don't Allow** on the prompt). A second press showed the overlay error with its code. Screenshot of the 72 px overlay: two full message lines plus the small `mic_denied` line under them, all inside the pill, nothing clipped. CSS clamps the message to two lines (`.overlay-message` `line-clamp: 2`), so a longer message loses its tail rather than overflowing. Tray title "⚠ Dictation failed", tooltip "… (mic_denied)".
- Tray → **Troubleshoot…** opened the window: ✗ Microphone with "Open Microphone Settings", ✓ Accessibility, ✓ Hotkey, ✗ Transcription engine "No API key is saved… (key_missing)" with "Open Settings", ✗ Last error "mic_denied at 2026-11-03T18:32:20Z…" with "Open Microphone Settings". The **Open Microphone Settings** button opened System Settings on the Microphone page, with dictation-app's switch off.
- **Preview report** showed the text below; **Copy** put the same text on the clipboard:

```
dictation-app diagnostics report
Built from a fixed list of fields. It holds no audio, transcripts, text you typed or copied, API keys, app names, file paths or log lines.

app_version: 0.1.0
macos_version: 15.6.1
arch: aarch64
microphone: denied
accessibility: granted
hotkey: RightAlt
engine: cloud (groq)
engine_ready: no
analytics: on
recent_errors:
  - 2026-11-03T18:32:20Z mic_denied
```

- Save and Open a GitHub issue: see criterion 3.
- Not done: "Test connection" against a real provider (no key in the VM; needs `GROQ_API_KEY`), and Troubleshoot with the on-device engine.

## Failure A: existing e2e scenarios fail (stale expectations)

The PR appends the code to the overlay text and the tray tooltip but did not update the scenarios that match those strings exactly. On this PR's build, `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_microphone_denied_then_recover e2e_overlay_rendered_states_and_error e2e_tray_error_clears_on_next_recording e2e_error_bad_key` gave **1 passed, 3 failed**:

- `e2e_microphone_denied_then_recover`: "the overlay renders it" (`overlay "shown: Microphone is off. … menu bar." not within 5s; it is "hidden"`: the rendered text now ends with ` mic_denied`) and "and the tray's tooltip has it" (tooltip ends ` (mic_denied)`).
- `e2e_overlay_rendered_states_and_error`: the error text, its 4 s lifetime and the "Recording replaces it" watch (the text now ends ` key_rejected`).
- `e2e_tray_error_clears_on_next_recording`: tooltip ends ` (key_missing)`.
- By the same pattern, `e2e_settings_key_provider_recovery`, `e2e_accessibility_revoked` and `e2e_hotkey_listener_recovers` match the tooltip too.
- `e2e_error_bad_key` passed (it reads the log).

I patched the expected strings in the six scenarios (append ` <code>` to the overlay text, ` (<code>)` to the tooltip: `mic_denied`, `key_rejected`, `key_missing`, `hotkey_unregistered`) and re-ran all six on the same build: **6 passed, 0 failed**. I did not commit the patch (my instructions allow only this report); it is 81 lines in the Tester's worktree at `/tmp/e1-e2e-fix.patch` and trivial to redo. The Developer should apply it, but only after Major 1 settles the overlay/tray text, since the expected strings change again then.

## Gate
- vitest 148/148; nextest (core, ports, pipeline) 491 passed, 1 skipped; `cargo test -p dictation-app` 215/215; clippy clean for core and app.
- CI on the current head: DMG pass, Platform crate (macOS) pass, Quality gate (Linux) pending when checked. Coverage not run.

## Untested behaviour found
- Local engine "present but fails to load" and "still loading" in Troubleshoot: no test, and the code reports Ready (Reviewer Major 2).
- Overlay/tray text for model-load, paste and no-speech errors carries no table next step, and no test pins the text beyond the code.
- Overlay fit was checked for the two-line mic message only; the clamp bounds longer ones.
- The saved file's bytes in the VM (TCC-protected folder over ssh).

## Failures
1. Criterion 1: overlay and tray do not use the table. Repro: deny the microphone in the VM and compare the overlay text with the Troubleshoot Microphone row.
2. Criterion 2: engine readiness is a file-size check. Repro: put a correctly sized but unloadable model file in the model folder and select the local engine; Troubleshoot shows ✓ Ready.
3. Six e2e scenarios fail on their exact-text checks (Failure A).

No minor findings beyond these, so no tech-debt issues opened.
