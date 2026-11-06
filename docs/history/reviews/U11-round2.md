# U11 review, round 2

PR: #300 at bd8c9f7c62f12472afdb429535d53062de2ada47
Verdict: APPROVED

## Findings

No open blocker or major findings. Round 1 finding 1 is fixed.

The Continue generation is invalidated by Back, Provider changes, key edits (including Replace), Test & Save and unmount. The saved-key path checks it after testing and before switching engines; completion also checks before showing a failure or advancing. A stale result no longer changes the selected engine or sends the user forward. An uninterrupted Continue still succeeds.

## Validation

- All 457 Vitest tests passed locally on the reviewed head.
- Verified regression sensitivity without editing tracked product code: a temporary Vite transform replaced the `dropped` generation predicate with `false`. Six of the seven deferred-key-test cases failed: Back/success, Back/late failure, Provider/success, Provider/late failure, Replace and unmount. The uninterrupted success case still passed. The failures assert observable engine calls or stale error text, not merely implementation shape.
- Reviewed the incremental fix commit eb4f44f and subsequent screenshot/scenario changes. The unrelated G2 changes arrived through main and are outside this focused re-review.
- No VM e2e rerun by this reviewer. The Developer reports passing layout, five-steps and redesign-screenshot VM scenarios; the Tester and Design Lead remain responsible for their current-head evidence and manual approval.

## Checklist

1. Scope: OK. The five Design Lead requests are implemented: stable Welcome keycap width, plain Ready model names, regular form button height, Try it baseline alignment, and selected-chip primary colours. The criterion 8 amendment records the explicit Design Lead exception for Ready naming (#299).
2. Correctness: OK. Round 1 asynchronous Continue finding resolved as above. Generation invalidation resets working so Back/provider changes do not leave the footer stuck.
3. Security and privacy: OK. No new secret exposure or telemetry path. U10 sharing queue, exit guard and save semantics are unchanged; the Ready diff only substitutes modelTitle for spokenName. Its consent switch, captions, failure and pending-save handling remain intact.
4. Architecture: OK. Same shared modelTitle function as Settings/Speech; obsolete catalogue-name helper removed. No new dependency or platform change.
5. Rust and TypeScript practice: OK on the changed paths. The test's thrown structured error models the Tauri IPC rejection and has an explanatory suppression context.
6. Licences and reuse: OK. No new dependency or external implementation.
7. Tests: OK. Deferred tests detect removal of the guard; layout checks are strengthened for stable keycap position, form-button size, row baselines and both Ready names. Five-steps retains its functional assertions and adds quit/resume at Ready for a dark screenshot. Consent tests pass unchanged.
8. Git and PR: OK. This commit contains only this review report. Normal fetch/rebase/push with hooks required. Approval is a code-review verdict, not authorization to merge without current Tester PASS, required CI and the Lead's manual checks.

## Smaller observations

Some retained Welcome screenshots are explicitly documented as round 1 and still show the old jump; current evidence and the strengthened layout test distinguish the fixed behavior. Prefer replacing stale images when assembling the final design evidence. The binary-evidence storage preference from round 1 remains nonblocking; no new minor finding or follow-up issue is needed for this focused re-review.
