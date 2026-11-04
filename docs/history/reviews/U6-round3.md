Reviewer: APPROVED

# U6 review, round 3

PR: #262 at 845aa75 (key lifecycle fix 2a3850d)
Verdict: APPROVED

## Findings

No open blocker, major, or minor findings in this focused re-review.

## Earlier findings

- Round 1 finding 1 / round 2 lifecycle case: fixed in 2a3850d. keyRun now lives at module scope, so leaving Speech and returning does not create a separate generation. Every valid test start, provider change, key edit, and removal increments that same counter. The pre-write generation check therefore refuses the old test after removal or a newer test/save in a subsequent mount. Invalid/empty input does not start a test or write. Writes already sent remain subject to LiveEngine's shared KeyWrites mechanism, which also covers deletion. A page reload destroys the old JavaScript continuation along with the module. Two added tests cover remount followed by removal and remount followed by a newer successful save.
- Round 1 finding 2: remains fixed; restoring the engine from stored settings on a failed local-model selection write is unchanged.

## Focused U1 merge check

- Compared the merge with both parents. U1's rename, onboarding progress/diagnostics commands, runtime name/title, and first-launch Keychain notice are retained alongside U6's commands and ACL entries.
- The notice is read by Settings and displayed above the section content, including Speech. Tests cover a first-launch notice and its absence on other launches.
- The native main window retains Vocet Settings, 720×520, minimum 640×460, and resizing. U1's product/binary name and stable bundle identifier are retained.
- Source search across crates/, src/, and src-tauri/src found no Settings → Transcription strings. U1 diagnostics and the matching overlay/tray expectations now use Settings → Speech; the U6 guard remains in place (criterion 7b).
- No further scope, correctness, security/privacy, architecture, Rust/TypeScript practice, licence, or Git issues found in the changes reviewed since round 2.

## Validation and remaining integration

Focused Vitest: 98/98 passed across Settings, App, Onboarding, and Troubleshoot, including the two remount regression tests and U1 notice checks. The full quality gate runs through the mandatory pre-push hook. No product code or tests edited by Reviewer.

Criterion 7 (Show log file / Run setup again) remains the explicitly pending U5 integration and must be completed and checked before merge. This approval covers the reviewed head and fixes; it is not authorization to merge before that integration and the required Tester/Lead evidence. No merge performed.
