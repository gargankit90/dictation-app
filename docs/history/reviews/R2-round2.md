Reviewer: CHANGES REQUESTED

# R2 review, round 2

PR: #230 at 94049b4fa30d117b7eac20274064cd292a332c26
Verdict: CHANGES REQUESTED

## Re-check of round 1

- Major 1 fixed: `CHANGELOG.md:29` now distinguishes onboarding's required test (`src/Onboarding.tsx:295`) from Settings' independent Test key and Save actions (`src/Settings.tsx:196`).
- Major 2 fixed: `CHANGELOG.md:76` now limits reports to Rust panics and failed dictations, conditional on sharing and build support. This matches `crates/telemetry/src/errors.rs:297` and its panic hook; other crashes remain excluded and #226 remains linked.

## Findings on changed wording

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 3 | major | CHANGELOG.md:21 | “when you let go” promises the release-time focused app as the paste target. The app does not capture that target: transcription runs first, then insertion uses current focus (`crates/app/src/pipeline.rs:1020`, `crates/platform/src/insert/macos.rs`). Switching apps while waiting sends text to the newly focused app, as explicitly documented in `docs/ENGINEERING.md:172` and tested by `e2e_focus_switch_during_transcription`. | Replace “when you let go” with “when the text is pasted”, or use the revised TESTERS wording without a release-time guarantee. |

The revised `docs/TESTERS.md:112` says the app that has focus a moment later and is consistent with insertion behavior. No additional minor or blocker findings.

## Validation and scope

Re-reviewed only the changed key, crash-report and focus wording against the code and round 1 findings. No product behavior or tests changed; round 1 release-test evidence remains applicable. The normal push hook runs the full gate for this documentation commit. No publishing, owner Keychain access or merge was performed.

## Checklist

Scope/correctness: finding 3; original findings 1–2 fixed. Security/privacy: corrected error-report claim OK. Architecture, Rust/TypeScript practice, licences/reuse and tests: unchanged from round 1. Git/PR: review-report-only commit; no guard changes. No minors requiring issues. Original threads remain unresolved because this round is not approved, per the dispatch's conditional resolution instruction.
