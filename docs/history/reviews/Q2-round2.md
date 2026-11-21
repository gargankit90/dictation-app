# MQ2 review, round 2

PR: #336 at db42df993d381e88e50875f15f9d24e070b0d039
Verdict: APPROVED

## Findings

No open blocker or major findings. Re-reviewed only the four files changed since round 1.

| Round 1 finding | Status | Evidence |
| - | - | - |
| 1: incomplete cleanup generations accepted | Fixed | `parse_reply` requires `finish_reason: "stop"`; length, content_filter, error, null and missing reasons return no reply and therefore `CleanupError::BadReply`. Parser regressions and a local HTTP truncation regression pass. The existing pipeline regression confirms BadReply pastes raw text. |
| 2: corrected-number exception permits lost values | Fixed | Removed the exception and its cue scanner. Both numbers must remain, including the original `Print 20 copies, make that 30.` reproduction. Regressions reject loss of either value; the existing pipeline guard-fallback regression passes. |

Replied to both original inline threads confirming the fixes. The e2e fake now supplies `finish_reason: "stop"` for successful chat replies, preserving its intended successful-cleanup behavior.

## Validation

- Core cleanup tests: 15 passed.
- Transcribe cleanup unit tests plus `openrouter_q2` and pipeline `cleanup_q2` integration tests: 18 passed.
- `perl -c tests/e2e/fixtures/local-provider.pl`: syntax OK.
- `git diff --check 6356702..db42df9`: passed.
- Developer reported a passing gate; this review also runs the required gate before pushing its report. CI was pending when checked. The reported cargo-test-only shared-tracing flake is outside this corrective diff; nextest remains the gate's test runner.

## Checklist

1. Scope: OK; changes address the two blockers and update the affected tests/fake, reducing obsolete complexity.
2. Correctness: OK for the changed paths; incomplete generation and dropped corrected values now trigger raw fallback.
3. Security and privacy: OK; no request/filter/key/logging changes in this diff.
4. Architecture: OK; pure guard and client parsing remain in their existing crates.
5. Rust and TypeScript practice: OK; small parser check and deletion of obsolete scanning logic, no new dependencies or blocking work.
6. Licences and reuse: OK; unchanged.
7. Tests: OK; regressions cover both blockers and the existing fallback/deadline tests pass.
8. Git and PR: OK; corrective Conventional Commit, no guard changes or new minors. Approval is a code-review verdict; required Tester, Lead manual checks and CI evidence still govern merging. No merge performed.
