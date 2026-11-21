# MQ3 review, round 2

PR: #338 at 05db6cdee10fac4d73772ecc063f3cc880adf96b
Verdict: CHANGES REQUESTED

## Findings rechecked

| # | Severity | Location | Status | Remaining action |
| - | -------- | -------- | ------ | ---------------- |
| 1 | major | src/settings/connection.tsx:109; src/settings/Speech.tsx:255; src/onboarding/SpeechStep.tsx:227 | Partly fixed. The claim under the attempt lock correctly decides cancel versus save; winning cancellation saves nothing. Cancel-first correctly enters the saving state and preserves the real outcome. However, Done/Esc and onboarding Back/local-only still call cancellation without awaiting its answer and immediately hide the surface. The saving state is set only after that answer, too late to prevent navigation. | Await the cancellation decision before leaving; if cancellation loses to saving, retain the surface and report the actual save outcome. Add held-save tests for Done/Esc and Back without pressing Cancel first. |
| 2 | major | Native e2e scenarios and screenshots | Pending with the Q3 Tester, outside this focused code recheck. | Tester completes updated flows/screenshots and fake-provider Connect validation. |
| 3 | major | src/Onboarding.test.tsx:1082,1125 | Resolved. The helper waits for the initial key lookup, and the explainer assertion awaits KEY_REASON. | None. |

Thread updates: [remaining exit-path issue](https://github.com/gargankit90/dictation-app/pull/338#discussion_r4650502960), [resolved test race](https://github.com/gargankit90/dictation-app/pull/338#discussion_r4650503126).

## Focused checklist

- Scope and architecture: only the fix delta inspected; no new migration or framework. The attempt number and claim lock are small and appropriate.
- Correctness: finding 1 remains open for exit paths. The new Rust tests cover both claim orderings, replacement, slow successful saves and failing saves; the new UI tests cover Cancel-first success/failure but miss direct navigation.
- Security/privacy, licences and typed IPC: no new concern in the fix delta; the cancellation return type is updated in generated bindings. Round-one conclusions otherwise unchanged.
- Tests: Settings and Onboarding together passed all 186 tests locally. Targeted Rust connect tests are recorded in the PR status comment after completion.
- Git/PR: report only; no product/test edits by Reviewer. No merge. Finding 2 stays pending with the Tester and must not be interpreted as approved.

The Developer's reported gate pass is independent of the remaining user-visible exit-path error. Approval requires finding 1 to be completed and finding 2 to receive the requested Tester evidence.
