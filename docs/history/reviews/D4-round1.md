Reviewer: APPROVED

# D4 review, round 1

PR: #260 at efbfca99ac53103022ee8b796393c0e6f170bf54
Verdict: APPROVED

## Findings

None: zero blockers, majors or minors. No inline threads opened. No merge performed.

## Evidence

- The diff contains only the Clipboard rule in `docs/ENGINEERING.md` and the historical S1 run-2 report; no product code changed. The rule reproduces the Lead-approved proposal on PR #241, with line wrapping only.
- Checked against the merged P1 implementation (the insert directory is identical to `origin/main`): `RESTORE_DELAY` is 1500 ms, scheduled after the chord on `ThreadLater`. The timer and next insertion serialize pending restoration under the same mutex; the next insertion restores before snapshotting. The adapter records and checks both the session token and post-write change count, rechecking after rebuilding saved items. A changed clipboard discards the saved snapshot.
- `MacPasteboard` snapshots items and available representations, excluding stale dictation sessions, and dictation writes carry all three privacy markers. Secure Input is checked before snapshot, before writing and immediately before sending keys; a refusal after writing performs the conditional restore immediately. Other chord failures retain the transcript, with the existing paste-failure recovery hint.
- `finish()` closes insertion, waits for an in-flight insert and honors the pending due time. Pipeline cleanup awaits that work with `FINISH_BOUND` of two seconds and continues after timeout. The stated two-second bound concerns waiting for clipboard finish, rather than all unrelated shutdown work. The paste key is cached at initialization and refreshed on input-source changes; pasting reads the cache without a main-thread wait.
- No target Accessibility read determines restoration. An unread paste still expires after the fixed delay; Copy last dictation and history provide recovery. A target reading after restoration can receive the original clipboard, as the rule explicitly acknowledges. These are the accepted P1 trade-offs.
- Compared the complete S1 report body from “Changes since run 1” through the final paragraph against [the PR #255 Tester comment](https://github.com/gargankit90/dictation-app/pull/255#issuecomment-6628340549): exact equality after trimming outer whitespace. The file adds its normal report title, PR/head and PASS header; the historical evidence and limitations are unchanged.
- `scripts/check-doc-links.sh`: 50 relative links OK. Product/VM tests are not independently rerun for this documentation-only change; publication runs the normal full pre-push gate.

## Checklist

1. Scope: OK; only the two requested documentation changes.
2. Correctness: OK; statements checked against merged code and accepted trade-offs.
3. Security and privacy: OK; no secrets or new data collection, and clipboard protections accurately described.
4. Architecture: OK; no code or dependency changes.
5. Rust and TypeScript practice: OK; no code changes or suppressions.
6. Licences and reuse: OK; no code reuse or dependency changes.
7. Tests: OK; historical report preserved verbatim and documentation links checked.
8. Git and PR: OK; docs-only Conventional Commit, complete PR notes, no guard/spec edits; reviewer edits only this report.
