Reviewer: CHANGES REQUESTED

# U10 review, round 2

PR: #279 at 7acfa3b3fb42a297a26970356a7adf2a2372c81c
Verdict: CHANGES REQUESTED

Re-check limited to changes since round 1 and the earlier caption finding.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | src-tauri/src/commands.rs:469 | Off now calls note_sharing_off before write_sharing disables the gate and reporting adapters. The note synchronously creates directories, writes and renames the progress file. On the first Off in a walk, slow or stalled progress-file I/O can keep sending active, with queued events and requests still live, after Off is selected. This regresses round 1's disable-before-persistence ordering and the documented immediate opt-out. | Disable both reporting adapters before persisting the caption-history note, preserving serialized writes. Test that a blocked progress write cannot delay opt-out. |
| 2 | major | crates/core/src/onboarding.rs:192 | Old progress files lack sharing_was_off, which defaults to false. If an unfinished walk was Off then On before upgrading, settings now say On, setup_opened does not mark it, and Ready falsely claims On since setup began. Missing history does not prove continuous sharing. The requested old/unreadable progress caption tests are absent. | Treat unknown historical sharing conservatively, or avoid an absolute historical claim. Add legacy and unreadable progress tests through loading and caption selection, alongside fresh-install and Run setup again cases. |

Inline findings: [1](https://github.com/gargankit90/dictation-app/pull/279#discussion_r4484910157), [2](https://github.com/gargankit90/dictation-app/pull/279#discussion_r4484910406).

## Re-check

- Round 1's ordinary stored-Off → On case is fixed: setup_opened notes Off, the persisted boolean survives relaunch, and Ready selects the new caption. Off → On on Ready is also handled immediately by local offHere state before polling refreshes. History from earlier versions remains finding 2.
- Run setup again resets the boolean for a new walk; setup_opened then notes the current Off choice. The new Rust persistence test covers saving/reloading the flag and resetting it, and the usage test covers stored Off. The transition tests do not cover legacy or malformed progress history.
- The sharingQueue and Onboarding exit guard are unchanged. Ready still passes the same switch callbacks, saving state and finish handler. Adapter cancellation, ID deletion and error-report behavior are unchanged once write_sharing is reached; the new preceding persistence step is finding 1.
- Both new disclosure tests click the actual rendered summary and assert the native details.open state, without manually setting open. Ready also checks it remains open through polling. These are meaningful DOM-control tests; they do not replace the Tester's real-WebKit/VM verification.
- sharing_was_off is only a boolean in local setup progress. It is exposed as a boolean to the UI, is not a telemetry property, and holds no paths, text, keys or provider details. docs/privacy.md and the closed event schema are unchanged.
- Ready's row/CSS changes preserve labels and descriptions. The layout harness adds a usual-size Ready-with-download-strip case while retaining the existing minimum-size cases. No new dependencies, guards, secret handling or network destinations changed.

## Validation

- `npx vitest run src/Onboarding.test.tsx`: 86 passed.
- `git diff --check 5993288..HEAD`: passed.
- Focused Rust U10 tests and the full pre-push gate are run before pushing this report; their results accompany the PR comment.
- No product or test files changed by the Reviewer. No merge performed.
