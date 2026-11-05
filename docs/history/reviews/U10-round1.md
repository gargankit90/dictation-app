Reviewer: CHANGES REQUESTED

# U10 review, round 1

PR: #279 at 725361325247341cdb07b30b9c29d41360102699
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | src/onboarding/ReadyStep.tsx:192 | With sharing stored Off, reach Ready and turn On. The caption immediately says reports “have been on since setup began”, although the stored choice correctly prevented that. The same happens after Run setup again with stored Off. This misstates the privacy behavior required by criterion 7. | Use truthful wording for enabling here, or distinguish continuous sharing from a later opt-in; cover stored-Off → On in a caption test. |

Inline: https://github.com/gargankit90/dictation-app/pull/279#discussion_r4483617847

## Checklist

1. Scope: criterion 7 has finding 1; other criteria map to implementation and tests. Owner decisions respected: Ready switch, no Welcome notice, first-screen sending.
2. Correctness: OK for sending and opt-out. Stored choice is read before starting adapters; unreadable settings fail closed, which is the appropriate privacy fail-safe because an opt-out may be present. Run setup again does not re-enable adapters. Off closes the gate before persistence, including on read/write failure. Finishing does not reopen it.
3. Security and privacy: finding 1 concerns disclosure accuracy. All 19 SetupProblem reasons are fieldless fixed enums, serialized with only the reason; onboarding_left carries only a closed step. Error and download details cannot enter these fields. Commands record from Rust; the new webview command takes no arguments. docs/privacy.md lists both events and all reasons. Both adapters retain cancellation generations, queue clearing and install-ID removal; Sentry follows the same switch. No new external destinations or secret access.
4. Architecture: OK; no new dependency, unsafe block, OS conditional, or coordinator I/O. Capability addition is restricted to the existing settings window and generated IPC is consistent.
5. Rust and TypeScript practice: OK; no new production panic or unexplained suppression. No new blocking network work on the UI thread.
6. Licences and reuse: OK; no new dependencies or adapted code.
7. Tests: OK apart from missing caption-transition coverage in finding 1. Removed Rust tests enforce the reversed Welcome hold; queue, stale-save, pending-exit and last-choice protections remain. UI race tests moved to Ready. The e2e removals concern only Welcome sharing checks; Speech persistence and first-run flow remain. VM execution belongs to the Tester; no VM checks performed in this review.
8. Git and PR: OK; Conventional Commits and detailed notes explain U8 replacements, e2e edits and site/PR #275 follow-up. Milestone spec is the Lead's separate commit; guards unchanged.

## Validation

- `cargo test -p dictation-app u10_ --lib`: 18 passed, including real-adapter cancellation, ID deletion, stored Off and error reports.
- `cargo test -p dictation-ports setup_problem`: 2 passed.
- `npx vitest run src/Onboarding.test.tsx`: 80 passed on rerun. Initial run had one timing-sensitive failure in the unchanged Try-it hotkey-ready assertion at line 1217 (79 passed); recorded rather than treated as a U10 regression.
- `git diff --check origin/main...HEAD`: passed.
- No product or test files modified by the Reviewer. No merge requested or performed.
