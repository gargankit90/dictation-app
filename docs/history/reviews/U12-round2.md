Reviewer: APPROVED

# U12 review, round 2

PR: #290 at 021c205. Re-review of the two round-1 findings, U9 integration, and subsequent Design Lead changes.

## Findings

No open blocker or major findings. Both round-1 majors are fixed.

1. **Provider switching:** Settings enables `draftProvider`; a popup change without a saved key retains the working engine. A rejected test cannot persist the key or switch providers; Done drops the draft. A successful Test & Save switches when appropriate, while configuring a key from the local engine does not activate cloud unexpectedly. Saved-key selection retains the existing policy of trusting a previously saved key. The asynchronous lookup checks its generation before switching. New tests cover missing/rejected keys, cancellation, successful save, stale lookup and local-engine configuration; restoring the old immediate switch would fail these tests.
2. **Removal reconciliation:** removal is a committed write. Done is disabled, Escape is ignored, and field/provider edits wait until completion. The result is displayed, key status is reread and engine status is refreshed. A module-level removal promise makes a newly mounted panel's key lookup wait for the outstanding deletion. Deferred tests cover dismissal attempts, success with close/reopen, failure with the saved key retained, and Speech remounting while deletion is pending. Restoring the old dismissal or stale-generation early return would break this coverage.

## U9 merge and report

- `hotkey_setting.rs` retains U9's keycap conversion and pending-Accessibility handling, with U12's dictation-key wording.
- `Settings.test.tsx` retains U9's pending notice, automatic disappearance and normal listening cases, alongside U12's compact recorder and key-sheet tests.
- `Dictation.tsx` keeps the pending notice below the row and its polling, together with the compact recorder and revised caption.
- `validation.ts` keeps the keycap example for an empty setting and U12's dictation-key wording.
- The report renders `dictation_key` using the parsed keycap label. Malformed values become unknown; the issue-link test covers UTF-8 encoding. The allow-list still excludes API keys and arbitrary text. `docs/privacy.md` correctly describes the keycap label. No old-label parser was introduced.

## Design Lead changes

Reviewed the persistent Back footer and independently scrolling body on Report/Troubleshoot, Add key/Change key state, Delete model wording, paused chip, generic speech-model download line, language popup sizing and narrow-window recorder layout. Actions and state transitions remain intact; the layout changes keep Back outside the scrolling body. Light/dark token values retain the previous contrast guarantees. No new blocking regression found. Screenshot evidence is supplied in the PR; this review does not substitute for native VM/manual sign-off.

## Validation

- `npx vitest run`: 16 files, 387 tests passed, including the new deferred key-sheet regressions and retained U9 tests.
- `npx tsc -b --noEmit`: passed.
- `cargo test -p dictation-core report::`: report tests passed (including keycap rendering, invalid input, allow-list and issue-link encoding).
- The report-only push runs the normal required quality gate; no hooks are bypassed.
- No real-provider or VM computer-use checks performed in this re-review.

## Checklist

Scope, correctness, security/privacy, architecture, Rust/TypeScript practice, licences/reuse, tests and Git/PR: OK for the re-reviewed changes. Main's unrelated changes were not re-reviewed. No new tech-debt finding; round 1's fixed-delay negative-test observation remains nonblocking and no flaky failure reproduced here. Merge still requires current Tester evidence and the Lead's manual/Design Lead sign-offs.
