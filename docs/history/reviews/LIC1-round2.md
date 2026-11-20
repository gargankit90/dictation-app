# MLIC1 review, round 2

PR: #333 at 33157c981e4a3a77ab4e360f801a973aee87eccb
Verdict: APPROVED

## Findings

No open findings. Re-reviewed only the changes since e6b15e8 and closure of round-1 findings.

| # | Previous severity | Resolution |
| - | ----------------- | ---------- |
| 1 | major | Fixed by ccb6d2e: site/index.html:2624 now identifies FSL-1.1-ALv2. |
| 2 | major | Fixed by ccb6d2e: NOTICE:136 and THIRD-PARTY-LICENSES.txt:148 identify our code as FSL-1.1-ALv2 and preserve the MIT history through 0.1.0. The generated notice diff changes only that own-code statement; all third-party grants and credits remain intact. |

Replied to both inline threads confirming closure.

## Checklist

1. Scope: OK; both requested fixes, round-1 review and tester report only.
2. Correctness: OK; remaining own-code MIT statements identified in round 1 are corrected.
3. Security and privacy: OK; unchanged from round 1.
4. Architecture: OK; unchanged from round 1.
5. Rust and TypeScript practice: OK; unchanged from round 1.
6. Licences and reuse: OK; third-party grants unchanged, no dependency-policy changes. LICENSE and metadata unchanged from the approved checks in round 1. The Lead explicitly retains docs/design/specs/U6.md:32 as historical design documentation.
7. Tests: OK; tester report LIC1-run1.md records full gate PASS at ccb6d2e (the final product-text change), and Tester: PASS is on the PR. No tests rerun for this review of text corrections. CI checks at 33157c9 were pending when inspected.
8. Git and PR: OK; Conventional Commits, no guard changes, no new minor findings. Lead manual PASS is on the PR.

No merge performed under this review dispatch. Required CI checks must pass and the normal merge procedure still applies.
