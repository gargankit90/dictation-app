Reviewer: APPROVED

# U9 review, round 2

PR: #285 at 4db13b2

## Findings

No open blocker or major finding. Round 1 finding 1 is fixed.

## Changes re-checked

- **Report:** the keycap formatter moved unchanged from pipeline/tray into core and remains re-exported for existing callers. The report first parses `HotkeySpec`, then formats the canonical value; arbitrary paths/text cannot enter through this field. Modifier, combination, invalid-value and UTF-8 issue-link tests cover it. The field name stays `hotkey`; repository searches found no consumer parsing the old report value. Settings and the listener still use stored specs, not report labels. Privacy documentation now matches the visible report.
- **Debug download URL:** `TEST_FLAGS` is `cfg!(debug_assertions)` at every production parse call; false ignores the new flag, and the release variant of `with_model_url` returns the original downloads unchanged. Only the debug variant calls `from_stand_in`. The command takes a model id, never a caller-provided URL. The override is restricted to credential-free loopback HTTP, applies only at initial setup, and retains catalogue size/hash verification. The internal setter remains compiled in release but has no reachable release input path. No new production download destination is exposed.
- **Model fixture:** Perl binds only 127.0.0.1 and serves the explicitly supplied model; it does not interpolate request paths into disk paths. Full and ranged responses have consistent lengths/ranges, and short writes seek back to the actual sent offset. The hold file makes cancellation/resume deterministic while sending enough bytes to avoid a stalled transfer. Scenarios retain real catalogue SHA-256 checks. The host model is copied into the clone, with no network fallback or golden-image write. Server shutdown is scoped by fixture name inside the exclusively locked test VM; I did not invoke the VM helper during review.
- **Removed download:** the new `Removed` error reaches the existing failed-download UI through `user_message`. An unlinked open file no longer silently consumes the entire remaining download: existence is checked at each 1 MiB progress boundary, plus verification and rename. This is byte-progress based detection, not an instantaneous filesystem watcher. Once detected, returning drops the response/file, removes the running entry, and records the retryable error. Retry clears the failed entry and recreates the directory. Tests exercise deletion during transfer, deletion after the final progress callback, and successful retry. Real disk errors remain distinct.
- **Own-key scenario:** registered in the runner, starts a local provider with its existing fake key, resets settings, revokes Accessibility and proves the real listener is down. Test & Save must connect, Continue must reach Permissions, and the raw-name/hotkey notice must be absent. Its EXIT hook restores Accessibility, stops the provider and removes the stored fake key using existing cleanup helpers. It covers the missing own-key path without a real API request.
- **Other movement:** the merge from main accounts for the already-reviewed E2F2 Terminal-restoration/runner changes; these were not re-reviewed as U9 changes. The U9 fix also moves the unsupported-language warning into a described paragraph, anchors the chevron to the input wrapper, left-aligns the helper link and keeps its punctuation together. Picker keyboard handlers and list roles remain intact. Harness, tests and screenshots correspond to these changes; no unrelated product behavior, guard, dependency or telemetry change appeared.

## Validation and limits

Passed locally: `cargo test -p dictation-core report::` (9 report tests), `cargo test -p dictation-transcribe models::` (22 model tests), and focused LanguagePicker/Onboarding Vitest (95 tests). Shell syntax checks passed; Perl reports syntax OK. A temporary process owned by this review served a small fixture: health, full body, resumed body and Content-Range passed; that process was stopped by its PID. The mandatory full gate runs through the normal push hook.

I did not rerun VM scenarios or build a release executable. Release isolation was checked through both parser and compile-time setup branches; the gate includes the parser's release-mode simulation test. This code approval does not replace the Tester's current-head PASS or the Lead's remaining native focus/design/manual checks, and does not authorize merging before those conditions hold.

## Checklist delta

Scope, correctness, privacy/security, architecture, Rust/TypeScript practice, licences, tests and Git/PR: OK for this round's changes. The existing review remains the record for unchanged U9 code.
