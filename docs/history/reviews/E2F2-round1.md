Reviewer: APPROVED

# E2F2 review, round 1

PR: #283 at 62cf5dedf022a71342b8a6ca6c01428943ee1033
Verdict: APPROVED

## Findings

No blocker or major findings in the E2F2 diff. No product files changed.

## Requested checks

1. Restoration is disabled by SSH after `wait_for_vm` in `reset_vm`, and explicitly after `wait_for_vm` on the `--no-reset` path, before installation or any scenario. No earlier step launches Terminal. `lib_vm.sh` restricts the target to the two clones and rejects the golden image before these calls. The obsolete saved-state removal is gone. The default Terminal timeout remains 30 seconds; the quitter's pre-existing 60-second override is unchanged.
2. The 800-process / 100-login limits have substantial margin over the observed healthy full-suite peaks (303 / 0), and stop well before the known 1333-process limit and failing 1198 / 470 observation. They cannot be a mathematical guarantee for every future healthy workload, but are justified for this suite and configurable. `test-runner.sh` covers healthy load, each exceeded limit, overrides, padded counts, unreadable counts, abort reporting and restoration-write failure. Counts are boundary samples, not continuous peaks.
3. The ready file is freshly removed before launch and written by the Terminal job before polling; dictation waits for it. The wait status is now checked. Paste, sentinel restoration, and the quit-path restore log assertions remain intact. In the supplied app log, Inserting is 18:38:05.206119Z, tray Quit is 18:38:05.780424Z (574 ms later), and the quit-path restore is 18:38:06.712333Z (1506 ms later). Quit therefore arrived during the pending 1.5-second restore; restoration at approximately the due time is correct, not evidence that Quit missed the window. The scenario supports closing #270 for this reproduced failure. The poller's eventual timeout still falls through to Quit, but a late ordinary timer restore would not satisfy the retained quit-path assertion.
4. `accessibility_revoked` passes in the full suite with zero Terminal timeouts. This credibly supports the same harness cause: its earlier failures were Terminal-job timeouts and E2F2 changes no Accessibility behavior. One successful run supports that explanation, not a proof against every independent permission race.
5. The screenshot test is wrong to require the literal `Download paused` after every Cancel. U8 requires real progress, Cancel, retained partial data where implemented, and honest resume/restart claims; it does not prescribe that literal message for a missing file. `src/onboarding/logic.ts:113` chooses `Download paused` for `partial` and `isn't downloaded yet` for `missing`, with Resume available in either case. `src/Onboarding.tsx:192` persists the user's paused choice before cancelling. `ModelStore::status` reports partial whenever a part file exists (including an empty one); cancellation before a part exists legitimately reports missing. The observed message therefore does not establish a user-facing U8 bug. Fix the test by ensuring a partial file exists before testing the partial state, or explicitly checking the legitimate missing state and Resume behavior when no data was fetched. A controlled download fixture would also avoid its network/timing assumptions. This pre-existing screenshot-test repair need not be bundled into the narrowly scoped E2F2 PR.

## Evidence and limits

Reviewed `/tmp/e2f2-full-0955.log` and the developer worktree's `tests/e2e/out/20261105-095821-dictation-vm` logs, plus both prior root-cause reports. Full-suite result: **48 passed, 1 failed, 2 skipped**; peak boundary samples: **303 processes, 0 login**; no Terminal job timeouts. This meets criterion 6's Terminal-specific requirement, but is not a green full suite.

The PR description overstates the screenshot scenario: after the paused-text assertion fails, a later wait for `Downloading Parakeet English` also fails and ends the scenario early. Its app log records `model download failed ... network error: couldn't connect` at 18:41:38.639043Z. Dark-mode, resume-after-quit and later screenshots are consequently not validated by this run; it is not true that everything else in that scenario passed. Track the screenshot fixture/network repair separately and correct the PR's evidence description.

No VM use or product changes by the reviewer. No merge performed; Lead disposition and the project's merge prerequisites remain separate.

Local checks: runner tests **118 passed, 0 failed** (including ci-run tests **32 passed, 0 failed**); VM-lock tests **271 passed, 0 failed**, using fakes only.

## Checklist

- Scope: OK; E2F2 criteria mapped above (milestone authored by Lead).
- Correctness: OK for changed harness paths; unrelated screenshot limitation above.
- Security and privacy: OK; no new secrets, product logs, destinations or capabilities.
- Architecture: OK; shell harness only.
- Rust and TypeScript practice: OK; no Rust/TS changes.
- Licences and reuse: OK; no new dependencies or external adapted code.
- Tests: OK for changed behavior; developer full-suite evidence reviewed; local harness checks recorded below.
- Git and PR: OK; no guard edits or hook bypasses; PR evidence correction requested above.
