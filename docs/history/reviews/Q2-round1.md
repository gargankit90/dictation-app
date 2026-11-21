# MQ2 review, round 1

PR: #336 at 8aedebf6031e37b90664f62026dcc8a028808514
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | blocker | crates/transcribe/src/cleanup.rs:187 | The parser ignores `finish_reason`. A token-limited or filtered response with partial content is accepted as successful cleanup. Raw `Please send the report and cancel the meeting.` and partial reply `Please send the report` passes the guard and loses the second instruction. [Inline finding](https://github.com/gargankit90/dictation-app/pull/336#discussion_r4649225115). | Require a successful completed generation; map incomplete/filtered replies to `BadReply` so raw text is pasted. Test these response envelopes and fallback. |
| 2 | blocker | crates/core/src/cleanup.rs:128 | The inherited corrected-number exception permits deleting values despite v4 requiring both values and correction words. `Print 20 copies, make that 30.` → `Print 30 copies.` is accepted and pasted. [Inline finding](https://github.com/gargankit90/dictation-app/pull/336#discussion_r4649225189). | Remove the exception and its unused cue scanner; change inherited tests to reject deletion of either value and verify raw fallback. |

## Validation

- Ran the focused `cleanup_q2` and `openrouter_q2` integration binaries: 12 tests passed, including timeout/drop, next dictation, raw fallback, filters, attribution, status mapping and secret-free errors.
- Compiled a standalone harness importing the actual guard source without editing product code: both raw/reply pairs above returned `Accepted`.
- Independently compared the prompt's Git blob with `eval/cleanup-offline/prompts/v4.txt` on `eval/cleanup-offline` through GitHub: both are `801c395fd13566e63c810e8867af48b3aa4b6a94`.
- Core suite: 273 tests passed. Full gate and VM/manual checks are separate evidence; no real API requests or owner-screen interaction performed.

## Checklist

1. Scope: findings 1–2 prevent safe text preservation. Applied the owner overrides: no migrations and no automatic Voxtral fallback. Model catalog, settings fallback, key panel and cloud replacement otherwise map to the requested scope.
2. Correctness: findings 1–2. Deadline wraps the awaited cleanup future, dropping it at timeout; cursor read remains after cleanup. Focused cancellation/next-dictation tests pass. No new paste-target change found.
3. Security and privacy: OK in inspected paths. Speech and key-test requests retain `data_collection: deny`; cleanup adds `zdr: true`; redirects are disabled and retries retain filters. Error statuses exclude provider bodies, secrets are redacted, keychain account is OpenRouter-only, telemetry carries closed outcomes. Clipboard implementation unchanged.
4. Architecture: OK. Pure guard in core, cleaner trait in ports, HTTP in transcribe, deadline/fallback in app, thin shell wiring. No new platform-conditional or unsafe implementation found.
5. Rust and TypeScript practice: OK in changed production paths. No new blocking operation in cleanup, locks are released before awaits, boxed cleaner bridges the existing port style. Finding 2 also identifies avoidable obsolete complexity; no broader redesign requested.
6. Licences and reuse: OK. Prompt/echo idea credited to Blurt (MIT) in NOTICE; existing experimental guard is project code. Dependency changes remove multipart and add a test-only SHA dependency; no new GPL reuse found.
7. Tests: findings 1–2 need regressions; existing inherited tests explicitly accept dropping corrected values. Focused integration tests pass but do not cover these losses. Coverage/full gate remains CI/Tester evidence.
8. Git and PR: OK for this review scope. Conventional commits and detailed PR notes present; CI pending when inspected. No guard changes in the diff. No minor findings needing follow-up issues. Do not merge until blockers are fixed, re-reviewed and the required Tester/manual/CI evidence is complete.
