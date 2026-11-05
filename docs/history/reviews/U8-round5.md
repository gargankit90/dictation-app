Reviewer: CHANGES REQUESTED

# U8 review, round 5

PR: #269 at 76642b0fa514ab5fca7ee989d555bc2ab98dd459

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | blocker | src/Onboarding.tsx:66 | The queue survives step changes, but not as a shared writer across tray-driven view changes. Settings from the tray closes onboarding (`lib.rs:665`); App then unmounts Onboarding and mounts Settings, whose Privacy pane creates another independent queue. The old async loop continues after unmount. Starting on at Ready, queue off then on while the first off is held; open Settings through the tray while stored sharing is still on, and choose off there. An allowed completion order is old off, new Settings off, old queued on. The last user choice is off but the final stored state is on, and `sharing_turned_on` enables both adapters and records an event. `settings_write` serializes individual writes, not the two queues or their intent order. Thus the claim that the tray cannot send against the last choice is false. | Own one sharing queue above both views (or serialize/version user intents in Rust), or prevent tray view changes until outstanding sharing choices settle. Add deferred IPC coverage of the tray/focus view change and a later Settings opt-out overtaking an earlier queued opt-in; assert final sharing off and no stale re-enable. |

## Re-check

- Both round-4 findings are fixed for in-window navigation: `leave()` covers step changes, finish and History; the strip delegates to guarded `goTo`. Failure blocks all these exits.
- The four deferred-write cases actually attempt Start dictating, Back, History Change and the paused-download strip while pending and between writes, then verify the exit succeeds after the final off. Failure coverage attempts all four exits too.
- Removing only the guard or only disabling leaving these tests green is acceptable defence in depth: either layer independently preserves the observable behavior. The reported mutation removing both makes all four cases fail. Independent mutation sensitivity of both layers is not required for this behavior test. I inspected those layers and ran the tests; I did not modify product code to repeat the mutation.
- Tray Settings/Resume themselves emit no step event, but that is insufficient: a view change creates an independent writer, while the old writer can subsequently enable telemetry. Finding 1 is a source-level scheduling trace, not a claim of a VM reproduction.
- The download log contains only the resume enum, no private data. The revised VM scenario checks the server outcome rather than strip wording; frontend coverage retains wording checks. Safari's window wait introduces no app behavior change.

## Checklist

1. Scope: OK for the round's changes.
2. Correctness: finding 1.
3. Security and privacy: finding 1.
4. Architecture: OK; no dependency or capability change.
5. Rust and TypeScript practice: OK in changed code.
6. Licences and reuse: OK; no new dependency or copied code.
7. Tests: 86 focused tests passed (`Onboarding.test.tsx`, `onboarding/logic.test.ts`); missing cross-view consent regression in finding 1. VM execution remains with Tester. Full gate runs in the push hook.
8. Git and PR: only this report edited; no guard changes. No merge authorized.
