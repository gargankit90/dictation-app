Reviewer: APPROVED

# U8 review, round 3

PR: #269 at 8202794e6046af77f92666cdc281499f6c204422
Verdict: APPROVED

## Findings

No new blocker, major or minor findings in this round. Round 2's blocker is fixed.

## Re-check

- Welcome and Settings share `useSharingSwitch`: only one setter is outstanding; subsequent clicks replace the queued choice. Continue stays blocked through the queue and after a failed final write. The deferred IPC test starts off, clicks on then off, and releases newest first: overlapping old setters would apply off before on and fail the final stored-false assertion. A second test checks coalescing and that Continue remains blocked between writes. The existing backend tests check that leaving Welcome obeys saved consent rather than stale adapter state.
- U7 integration retains `Recording` and its existing retention policy. `attempted` is a separate memory-only value, not a serialized history field; Off still saves nothing, ordinary failed pastes follow U7, and Secure Input clears the attempt without recording its words. Clear forgets it too. No new text/length logging, event field or reporting path. Practice IPC returns words only while the onboarding view is active. Tests cover Off, failure, Secure Input and Clear.
- `leave_setup_for_settings` is in the main-window capability only; main is the shared Settings/onboarding window. Overlay has no grant, and the capability tests enumerate every command and check refusal from overlay. It closes the onboarding view without finishing setup, then shows Settings; the frontend selects History. The real limit and Off wording have tests.
- The ax.js password-field fix uses Command-A/Delete so WebKit fires input events and React cannot restore the previous value before typing. Its bounded wait still demands exactly the expected character count, prints no secret and fails on mismatch. It weakens no assertion. VM first_run re-execution remains the Tester's responsibility.
- Inspected the U7 merge resolutions: history ownership, clipboard finish forwarding, tray copy, both CSS sections, typed command registration and capability lists are preserved.

## Checklist

1. Scope: OK for the requested round-only re-check.
2. Correctness: OK; serialized consent writes and final-choice regression coverage.
3. Security and privacy: OK; U7 retention and Secure Input exclusion preserved; main-window ACL only.
4. Architecture: OK; no new cross-layer dependency or platform code.
5. Rust and TypeScript practice: OK on changed code.
6. Licences and reuse: OK; no new dependency or adapted third-party code.
7. Tests: 77 focused frontend tests passed (Onboarding and onboarding logic). Full gate runs through the push hook; result recorded in the PR comment. VM scenarios not rerun by reviewer.
8. Git and PR: OK; only this report edited by reviewer; no guards changed. Approval does not authorize merge before current Tester PASS and Lead manual checks.
