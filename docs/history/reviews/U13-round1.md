Reviewer: APPROVED

# U13 review, round 1

PR: #289 at 5d66d07
Verdict: APPROVED

## Findings

No blocker, major or minor findings.

## Focused review

- `SilenceWatch` latches speech at the existing non-quiet threshold (at least −50 dBFS), recovers from No sound immediately, and never calls a later pause silent. Explicit timestamps cover 2499/2500 ms and a five-second pause. `OverlayModel` constructs a fresh watch on every Recording event, resetting both fields even when the visible view is unchanged. Cancels return to Idle; busy presses and key repeats produce no new phase event, so they cannot reset an active hold. The next accepted hold resets the watch.
- Meter clamping and rounding give 4 px at/below −50, 11 px at −35, and 18 px at/above −20; the near-boundary samples −47.5/−22.5 round to 5/17. Seven bars, scrolling, transitions and the single pending read guard from #257 are unchanged; its stalled-read test remains intact.
- Done is exposed through the status role; its check is decorative. `paste_sent`, Rust variants, log and e2e sequence matches remain internal and unchanged. No former success-label string remains outside historical documents. The app's visible Pasted is confined to Try it; site badges are explicitly deferred to the Design Lead as criterion 5 permits.
- Skipped/empty transcription never enters Inserting, so Idle hides it without Done. Insertion failures and Secure Input errors clear the success flag and retain the error on Idle. Existing tests cover both failures and the following skipped dictation.
- The production shell uses `ERROR_VISIBLE_FOR` (6 s). Error replacement/restart tests now cross the six-second boundary; tray persistence is event-driven and has no four-second expiry. Searches found no stale error timing assumption outside historical documents. The e2e watch windows grow with the duration, and exact sequence, rendered text, expiry tolerance and replacement assertions remain enforced.

## Checklist

1. Scope: OK. Criteria 1–4 map to implementation and tests; criterion 5 is explicitly deferred as allowed. Criterion 6 remains the Lead/Design Lead manual sign-off; fixture videos do not establish live microphone acceptance.
2. Correctness: OK, including hold reset, cancellation/busy events, skipped/blocked paste and superseded timers.
3. Security and privacy: OK. No new data, permissions, logging payloads or destinations.
4. Architecture: OK. Pure core clock inputs, existing layering and typed views retained.
5. Rust and TypeScript practice: OK. No new suppression, unsafe, blocking callback or unchecked error handling.
6. Licences and reuse: OK. No new dependencies or adapted external code.
7. Tests: OK. Fake/paused clock coverage and existing overlap protection retained; e2e changes do not weaken assertions.
8. Git and PR: OK. Conventional commits and detailed notes; supplied Design Lead milestone included, no guard edits.

## Validation

- `cargo test -p dictation-core level::`: 7 passed.
- `cargo test -p dictation-pipeline overlay`: 28 unit tests passed.
- `npm test -- src/overlay/Overlay.test.tsx`: 52 passed.
- Required Linux quality gate and macOS platform CI, plus DMG CI: green at reviewed head.
- VM e2e was reviewed as code and developer evidence, not rerun by this reviewer. Merge still requires current Tester PASS and Lead manual checks.
