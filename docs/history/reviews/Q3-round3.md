# MQ3 review, round 3

PR: #338 at 26db40b358b64aa1d8ff86b3b738c0884171f823
Verdict: APPROVED

## Findings rechecked

| # | Prior severity | Status | Evidence |
| - | -------------- | ------ | -------- |
| 1 | major | Resolved | Shared canLeave awaits the cancellation decision before Settings Done/Esc and onboarding Back/local-only navigate. A committed save retains the surface and reports its real outcome. All four direct held-save exits have UI tests, in addition to the Cancel-first and Rust claim/save tests from round 2. |
| 2 | major | Q3 acceptance gap resolved; one nonblocking packaging-harness follow-up | Tester updated the onboarding/recovery and screenshot scenarios and added e2e_connect_openrouter. Q3-run1 reports eight passing native scenarios, including PKCE exchange, persisted Keychain key, dictation, 402 tray mapping, Disconnect and secret-file/log checks. The older DMG scenario still uses removed selectors; see minor follow-up below. |
| 3 | major | Resolved in round 2 | Initial saved-key lookup is awaited; no further change required. |

No open blocker or major finding remains for the Q3 changes.

## Minor follow-up

- tests/e2e/scenarios/e2e_dmg_install.sh:116: the legacy packaging guide scenario accesses the hidden key field directly and expects the old connected message. Track its selector update and native rerun in [tech-debt #341](https://github.com/gargankit90/dictation-app/issues/341). This is downgraded from the broader round-one harness concern because the required Q3 onboarding/native connection coverage is now supplied; it does not indicate a remaining product or Q3 acceptance failure.

## Focused checklist and validation

- Scope: inspected only the round-2-to-round-3 exit changes and Tester additions. No migrations or new framework; one shared exit guard keeps the fix small.
- Correctness: all four exits await the same decision, and local-only changes the selected mode only after leaving succeeds. Existing bounded save behavior and Rust claim arbitration are unchanged.
- Security/privacy and architecture: no new product network destination, permission, dependency, secret exposure or layering change. The fake validates S256 and code without logging either; callback target is restricted to loopback. Its made-up exchange key is removed from the control file after exchange.
- Tests: `npx vitest run src/Settings.test.tsx src/Onboarding.test.tsx`: 190 passed. Shell syntax check of e2e_connect_openrouter and Perl syntax check of local-provider passed. Native evidence is from the Tester; the Reviewer did not rerun VM scenarios.
- Native evidence limitation: Q3-run1 was run on 1a6fcb2 before the cancellation-only fixes; the report explicitly documents this. The new exit behavior is covered by the focused held-save tests. Lead real-account validation remains a merge prerequisite, and the post-merge main e2e run remains required.
- Git/PR: report only; no Reviewer product/test edits. Updated the original finding threads. No merge performed.

References: [Tester report](../test-reports/Q3-run1.md), [resolved exit thread](https://github.com/gargankit90/dictation-app/pull/338#discussion_r4650748366), [native coverage thread](https://github.com/gargankit90/dictation-app/pull/338#discussion_r4650748649).
