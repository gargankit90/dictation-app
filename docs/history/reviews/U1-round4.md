Reviewer: APPROVED

# U1 review, round 4

PR: #250 at bafab17800238dd17791a58952501b1d3f2aac3f

Focused review of the merge against its main parent a1d128b and its U1 parent da34091, including the explicit conflict resolutions (`git show --remerge-diff`). No blocker or major merge regression found. Counts: 0 blockers, 0 majors, 1 minor tracked as tech debt.

## Finding

| Severity | file:line | Finding | Follow-up |
| -------- | --------- | ------- | --------- |
| minor | tests/e2e/README.md:237 | The historical pre-rename bundle and the path retained during the in-app update are now described as Vocet.app. The scenario correctly uses dictation-app.app for both, before the later DMG-style move. | Restore the two historical path names; tracked in [#261](https://github.com/gargankit90/dictation-app/issues/261). [Inline thread](https://github.com/gargankit90/dictation-app/pull/250#discussion_r4467963969) resolved after tracking. Non-blocking. |

## Merge checks

- **U4:** meter rendering, polling, level transport, the overlay-only level command/capability and Paste sent remain. OverlayContent and parseView match the merged main parent. Rust overlay changes against main remove the error code and update copy/tests; they retain U4's state machine. The error sentence can wrap to three lines as U1 requires.
- **P1:** crates/platform/src/insert and crates/app/src/pipeline.rs are identical to the merged main parent. Clipboard markers, restoration, deferred completion and shutdown behavior were not edited by the resolution. The paste-failure sentence retains main's clipboard/⌘V recovery advice.
- **S1:** the merged shell constructs LoginItem::new with the config handle. The existing set_launch_at_login command and immediate checkbox update are preserved; Settings differences against main are the U1 notice and Vocet/arrow copy. The obsolete combined-save rollback test removed in the conflict resolution was already absent from main's S1 implementation.
- **U2/U3:** picker implementation/tests, language catalogue, tokens.css and token tests are identical to the merged main parent. Both entry-point imports and the hex-colour lint remain. The targeted tests cover them.
- **U1:** progress and onboarding retain completion-count-based success, rather than textarea contents. The pipeline's successful InsertDone-to-Idle path remains the source of completion; insertion errors prevent it. Troubleshoot still hides Test connection for local mode and shows readiness, and retains the fixed relaunch failure handling. Error payloads/rendering omit codes; diagnostics/report/analytics identifiers remain stable. The name scan and inspection distinguish internal identifiers/hidden overlay title from user-visible copy.
- **Transition setup:** tr_write_settings now clears last-run-app-name before creating the old install's settings, removing the earlier-scenario marker that suppressed the first-launch notice. The scenario still uses the genuinely old bundle path; this change only resets fixture state. No VM GUI was driven for this review.

## Validation

Targeted frontend run: 200 tests passed across overlay, onboarding, Settings, Troubleshoot, LanguagePicker and design-token tests. Full scripts/gate.sh passed, including 1,282 workspace tests, coverage thresholds, frontend checks/build, name tests, guards and shell helpers. No signal-test retry was needed.

## Checklist (merge scope)

1. Scope: OK; preservation checks above, with one tracked documentation minor.
2. Correctness: OK; no blocking conflict-resolution defect found.
3. Security/privacy: OK; P1 insertion code and U1 privacy fixes preserved; no secret/transcript exposure introduced.
4. Architecture: OK; main's level transport and login-item wiring retained.
5. Rust/TypeScript: OK on inspected resolutions and typed command registration/capabilities.
6. Licences/reuse: OK; no new adaptation or dependency in these resolutions.
7. Tests: targeted behavior tests pass; full gate run before pushing.
8. Git/PR: review-only commit; no guard edits or merge performed; minor tracked in #261.
