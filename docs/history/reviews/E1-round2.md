Reviewer: APPROVED

PR: #225 at bed6ef9. Re-review limited to 6f5cd72 and bed6ef9 against the two round 1 majors and regressions introduced by those changes. No open blockers, majors or new minors.

## Findings resolved

1. **Round 1 Major 1 fixed in 6f5cd72.** `crates/app/src/overlay.rs:99` and `crates/app/src/tray.rs:104` now use `ErrorCode::user_text()`, which combines the table's message and next step. Model-load, paste, Accessibility, microphone and no-speech surface tests verify recovery text. Raw adapter reasons no longer reach these surfaces. Shortened table copy retains the appropriate Settings, permission, retry and relaunch guidance; every entry is bounded to 105 characters. The six changed e2e scenarios now expect this text plus the code, while log expectations retain the separate Display text.
2. **Round 1 Major 2 fixed in 6f5cd72.** `src-tauri/src/diagnostics.rs:421` reads the live engine snapshot and combines it with download state. Loaded, loading, load-failed and missing engines now map to distinct diagnostic states; a failed loader produces `model_loading_failed`, a Settings fix, and `engine_ready: no`. The report uses that same readiness decision. New tests cover failed and loading engines, a present-but-not-loaded model, and report readiness. `docs/privacy.md` now describes loaded state rather than claiming size-based status proves verification.

## Checks

- Traced `LiveEngine::current_engine`, `Transcribers::current/update_staged`, local loading and settings rebuild. The loader publishes ModelLoading before verification/loading and publishes its result afterward; diagnostics reads the current snapshot without taking the long-lived loader/update lock or initiating loading, hashing, recording or network requests. Settings and engine snapshots are separate reads, so a refresh racing a settings change can briefly describe a transition; the loading path itself no longer reports Ready throughout the load. No new consequential race found.
- Checked the changed table actions against their next steps, overlay/tray payloads, shortened messages and all six e2e expectation changes. No new data leaves the app, report fields are unchanged, and reasons remain excluded from surface text and report history. Architecture, IPC capabilities, dependencies, licensing and analytics are unchanged by these fixes.
- `cargo test -p dictation-app diagnostics`: **11 passed**.
- `cargo nextest run -p dictation-core -p dictation-pipeline -p dictation-ports`: **493 passed, 1 skipped**.
- The initial broad `cargo test` invocation passed core/ports but failed the existing pipeline logging test because its captured output lacked `dictation failed`. That test passed when run alone, and the whole selected suite passed under the repository's nextest runner; this is not evidence of a regression in the changed paths.
- `git diff --check 1bae4cc..bed6ef9` passed. No VM run performed by this reviewer; developer reports six changed scenarios passing in the VM. Exact font rendering remains part of the Tester/Lead VM checks; a character-count test alone does not establish pixel fit.

Only this review file is committed. No product, test, spec or guard edits; no new tech-debt issues and no merge performed. Approval addresses the code review; Tester PASS and the Lead's manual checks remain required before merge.
