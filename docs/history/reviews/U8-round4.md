Reviewer: CHANGES REQUESTED

# U8 review, round 4

PR: #269 at a3065ef66583716a406572cf760b18975306a0d5
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | blocker | src/onboarding/ReadyStep.tsx:117 | The sharing wait only affects the footer and History button. With a paused/failed model, the download strip still offers Use my own key instead. Clicking it while sharing writes are queued calls `goTo("speech")` and unmounts Ready before the last choice is applied. For example, start off, queue on then off, and use the strip while on is outstanding: the step event can run before the queued off. | Gate every navigation path, including the strip, on the shared consent queue settling. Add a deferred-write regression with a paused model that attempts the strip exit between writes and checks no step command occurs until the final off is stored. |
| 2 | major | src/Onboarding.test.tsx:1224 | The new pending-write test asserts Start dictating and Back are disabled, but never checks History Change or attempts it. Removing `disabled={saving}` from History Change would leave every added test green. | Hold writes, click History Change, and assert no `leave_setup_for_settings` call and no navigation; release all writes and verify it then works. Also assert navigation remains blocked between successive queued writes. |

## Re-check

- Ready, Welcome and Settings reuse `useSharingSwitch`; no duplicate writer was introduced. `sharingBlock` also shares the primary-action error/saving policy.
- The main Ready footer and History control have the intended pending-write guards in product code. The strip bypass above prevents approval of the overall consent guarantee.
- Try it has the requested title and small key label. The fix wrapper scrolls on failure-kind, words and copy-status changes; the layout harness checks its position before manually scrolling content. No additional issue found in those changes.
- No new Rust code, dependency, telemetry property, secret handling, capability or licence change in this round.

## Checklist

1. Scope: OK for this round-only review.
2. Correctness: finding 1.
3. Security and privacy: finding 1; pending consent must cover every exit.
4. Architecture: OK; shared hook retained.
5. Rust and TypeScript practice: OK in the changed code.
6. Licences and reuse: OK; no new dependency or copied code.
7. Tests: finding 2. All 81 focused frontend tests passed (`Onboarding.test.tsx` and `onboarding/logic.test.ts`). VM layout scenario not rerun by reviewer; source assertion inspected. Full quality gate runs in the push hook.
8. Git and PR: only this review report edited; no guards changed. No merge authorized by this review.
