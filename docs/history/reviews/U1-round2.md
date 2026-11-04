Reviewer: CHANGES REQUESTED

# U1 review, round 2

PR: #250 at 5a0ef8868cbcc2aaae38a9534d4fd2d5305b3902

Rechecked the round-one findings and the new recovery, notice and transition-test changes. The earlier blocker and major are addressed; two new major findings remain. Counts: 0 blockers, 2 majors, 0 minors.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | src/Troubleshoot.tsx:218 | Relaunch Vocet discards the command's typed error result and does not catch IPC rejection. If preparing the onboarding resume marker fails (for example a non-writable config directory), the app does not restart and its useful quit-and-reopen error is hidden. The new test only checks invocation. | Handle both returned errors and rejected promises, display the recovery instruction inline, and test a failed restart preparation. |
| 2 | major | crates/app/src/rename.rs:15 | The new privacy assurance “your key never leaves your Mac” is false for cloud transcription and Test connection. openai_compat.rs sends the saved key in the Authorization header to the selected provider. The same false assurance appears in CHANGELOG.md, TESTERS.md and the notice tests. | Describe storage and use accurately: the key stays saved in Keychain and is sent to the selected provider to authenticate requests. Update all copies and tests. |

Finding 1: [inline thread](https://github.com/gargankit90/dictation-app/pull/250#discussion_r4466743852). At review time GitHub's PR API still advertised 1dadcc5 while the fetched branch was 5a0ef88; the API rejected an inline comment on the new rename.rs file because that older PR head lacks it. Finding 2 is recorded here and in the PR review comment.

## Prior findings

- Round 1 blocker: fixed. The microphone sentence names the correct System Settings pane and explicitly says to relaunch Vocet; the semantic assertion retains that instruction. Troubleshoot now offers the button, with its error handling covered by new finding 1.
- Round 1 major: addressed. The new scenario builds a genuinely pre-rename commit and the new tree, signs both with the same identity, installs through the old app's updater, verifies the updated version, settings, grants, one bundle and one enabled login item at the installed path, and checks subsequent updater discovery. It also exercises the documented DMG-style path change. mainBinaryName preserves the executable name.
- Read U1-update-path-evidence.md and the same-name control scenario. The retained Keychain item is checked by metadata; the transition does not prove the updated app can read its old key after Always Allow, and the DMG phase stores a new dummy key. The control reproduces the prompt without a rename, so this is an explicitly disclosed existing self-signed-update limitation rather than evidence of a rename regression. Likewise the login item is checked through status and BTM, not a reboot; the old displayed name is documented as cosmetic.

## New-path checks

- First-launch detection: the settings loader does not create settings on read. An old settings file with no marker produces the notice; the marker records Vocet so normal later launches suppress it. Fresh installs write the name without showing an upgrade notice. The launch-scoped Tauri state is stable across Settings mounts. A marker write failure deliberately repeats the notice and logs a warning. No every-launch/never-launch defect established under normal writable storage.
- Relaunch routes through the existing typed command and restart implementation, including its onboarding resume marker. New finding 1 concerns its ignored failure result.
- Copy uses Settings → Transcription, matching the UI. Error codes remain in diagnostics/report and stable analytics values. The added notice command returns no secrets and is main-window-only.

## Validation and checklist

Targeted Onboarding, Settings and Troubleshoot frontend tests: 119 passed. Full scripts/gate.sh passed: 1,219 workspace tests, coverage thresholds, 221 frontend tests, production build, guards and shell checks; no signal-test retry was needed. No VM GUI or owner processes were touched in this review; VM conclusions above are source review plus the committed developer evidence.

1. Scope: OK on rechecked changes; prior migration-test gap addressed.
2. Correctness: finding 1.
3. Security and privacy: finding 2 (inaccurate assurance, not a newly introduced key leak).
4. Architecture: OK; marker I/O remains in app, with thin Tauri state/command integration.
5. Rust/TypeScript practice: finding 1; no new unsafe, OS conditional or dependency issue.
6. Licences and reuse: OK on this round's changes.
7. Tests: finding 1; marker branches and microphone semantics covered; VM evidence limits stated above.
8. Git and PR: review-only commit; no guard changes or merge. No minors requiring tech-debt issues.
