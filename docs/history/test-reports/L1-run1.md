# L1 test report, run 1

PR: #217 at 2d20646
Result: FAIL (criterion 7 only: the PR conflicts with `main`, so hosted CI never ran; criteria 1-6 and 8 pass)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Local engine behind the port | `local::tests::*` (fake `SpeechModel`), `tests/local_model.rs` (real model, env-gated; skipped here) | PASS | The real model ran end to end in the VM (see 8). `cargo nextest run -p dictation-core -p dictation-transcribe -p dictation-ports -p dictation-platform`: 628 passed, 1 skipped (the env-gated real-model test). |
| 2 | Catalogue | `local_models::tests::criterion_2_*` | PASS | HEAD requests to both Hugging Face URLs: `x-repo-commit`, `x-linked-size` and `x-linked-etag` equal the pinned commit, `size_bytes` and `sha256` for both models (Parakeet 477274496 / a8bf3de2...; Qwen3 589560480 / 5b58f32a...). Exactly two entries, Parakeet first. The VM downloaded the Parakeet file and the app's own SHA-256 check accepted it. |
| 3 | Model manager | `models::tests::*` (wiremock, 16 in `-p dictation-transcribe models::`), `src-tauri/src/models/tests.rs` | PASS | Mutation check: changed `if hashed != expected` to `if false && hashed != expected` in `verify()`; `criterion_3_a_file_with_the_wrong_hash_is_deleted_and_never_used` failed (15 of 16 passed). Restored with `git checkout`; tree clean. Progress, resume, cancel, delete, no-space and typed errors are each covered by a named test. |
| 4 | Settings, rebuild, Unavailable message | `npx vitest run`: 126 passed (7 files); `cargo nextest run -p dictation-app`: 187 passed | PASS | VM: "On this Mac" radio, model list, Download, progress, "Downloaded", Save, "Settings saved." all driven through the real Settings window. Before Save the log shows `engine="unavailable"` with no key. Unavailable message text is covered by `engine::tests::local::criterion_4_*`. |
| 5 | Same pipeline, no audio on disk, no transcripts at info | `engine::tests::local::criterion_5_6_*`; VM log | PASS | VM log phases per dictation: Recording, Transcribing, Inserting, Idle. No transcript text in the app log (grep of the saved log). No `--debug-save-audio` path in the new local code. |
| 6 | Warm model, release on switch, off the event loop | `criterion_5_6_*` (counts loads: 1 across an unrelated save, released on cloud, 2 after returning), `local::tests::criterion_6_*` | PASS | In the VM the 3 dictations after one Save each took 0.17-0.26 s from Transcribing to Inserting, so the model was kept loaded. |
| 7 | Hosted CI, deny, NOTICE | `gh pr checks 217`; `cargo deny check` | FAIL | `gh pr checks` reports no checks and `gh run list` has no run for the branch. `mergeable: CONFLICTING`; `git merge-tree` shows conflicts in `crates/transcribe/Cargo.toml`, `crates/transcribe/src/engine.rs`, `src-tauri/src/engine.rs` (B1 / #205 merged after this branch). GitHub doesn't start CI on a conflicting PR, so the Linux C++ build time and the macOS job are unverified. Locally: `cargo deny check` ok (advisories, bans, licenses, sources). NOTICE credits transcribe.cpp 0.4.1 (with ggml, miniz, Silero VAD), Handy (ideas only) and both model licences. Tracked in #223. |
| 8 | [manual] VM download and dictation, offline, latency | Tester-written `e2e_local_model.sh` (uncommitted, see the end) | PASS (offline NOT proven) | See below. |

## Criterion 8 details (dictation-vm, because the Lead's suite held dictation-vm-2's lock)

`E2E_VM=dictation-vm tests/e2e/run.sh --no-build e2e_local_model` on a debug build of 2d20646. Audio is a `say` clip ("Hello world, this is a dictation test on this Mac.") fed with `--debug-audio-file`; the VM's mic is not used.

- Settings → On this Mac → Download on the first model (Parakeet): started, showed "Downloading", reached "Downloaded" in about 28 s on the VM's network. Save: "Settings saved." (took about 11 s, the model load).
- Three dictations: TextEdit held `Hello world, this is a dictation test on this Mac.` each time.
- Release-to-text latency on the VM (debug build, from the app log): Transcribing to Inserting 0.26 s, 0.17 s, 0.18 s; Transcribing to Idle (paste done) 0.57 s, 0.48 s, 0.49 s. The first dictation after Save is already warm. Host latency was not measured by me; the PR body reports 411 ms decode on an M-series debug build.
- Offline: **not done.** The VM user has no passwordless sudo, so `networksetup` can't switch the network off. Indirect evidence only: the engine test counts 0 network requests for a local dictation, and the dictation path needs no key. Tracked in #222.
- Cloud path: `e2e_onboarding_first_run` PASS and `e2e_settings_key_provider_recovery` PASS (mock provider) in the same run.

## Gate
- `cargo nextest` on core, ports, platform, transcribe: 628 passed, 1 skipped. `-p dictation-app`: 187 passed. vitest: 126 passed. `cargo deny check` ok.
- Coverage (core/transcribe ≥ 85%, app ≥ 70%) not measured: no CI run to read it from.
- `crates/core/tests/settings_migration.rs` change confirmed mechanical: two added lines giving `transcription` and `local_model` their defaults.

## Untested behaviour found
- Offline dictation can't be cut off in the VM (#222).
- Not checked: a download interrupted by killing the app in the VM (resume is covered by the wiremock tests only).
- A first-run (before any Save) scenario with `local` already saved and the model missing is covered by unit tests only, not in the VM.

## Failures
Criterion 7: PR conflicts with `main`. Reproduce: `git merge-tree --write-tree --name-only origin/main origin/l1-local-model`. Expected: the Developer merges `origin/main` in, CI starts, and the Linux C++ build and the gate are green. Re-check after that: CI results only (and a rerun of `e2e_local_model` if the merge touches `engine.rs`).

## Test files written but not committed
Per the task ("commit only the report") these stay in the Tester worktree and are not on the branch: `tests/e2e/scenarios/e2e_local_model.sh`, `tests/e2e/fixtures/speech-16k.wav` (100 KB `say` clip) and a `radio-press` command in `tests/e2e/fixtures/ax.js`. The Lead can ask for them to be committed as `test(e2e): ...`.
