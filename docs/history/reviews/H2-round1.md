# MH2 review, round 1

PR: #307 at f8694611af542e28a1db12a81447ead3a494fdf7
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | crates/app/src/ui_events.rs:88 | Coalescing retains an old NoSound while dropping subsequent Recording phases. If the newest phase is Idle after a cancelled or short recording, OverlayModel shows the stale notice and Idle preserves it. | Invalidate queued notices on a subsequent Recording, including during coalescing; add a queue-to-overlay regression test ending with a cancelled recording. |

Reproduced against the compiled public UiSender and OverlayModel: queue
NoSound, Idle, 31 pairs of Recording/Idle (64 entries), then Idle to trigger
coalescing. Draining into the model ends on Error with the no-sound message,
although later recordings were cancelled. A valid stream can also use
NoSound, Idle, 30 Recording/Idle pairs, Recording, Transcribing, Idle: the
last short silent clip triggers coalescing and resurrects the first notice.

## Checklist

1. Scope: H2 behavior and drop boundaries implemented; manual design evidence remains for Lead.
2. Correctness: finding 1; normal job identity filtering and cancellation are sound, and a new Recording clears the expiry deadline.
3. Security and privacy: OK; no analytics event, device transmission, error-history entry or new attention state.
4. Architecture: OK; coordinator remains pure, no new dependencies or platform code.
5. Rust and TypeScript practice: OK.
6. Licences and reuse: OK; no reuse or dependency changes.
7. Tests: boundary and transition tests inspected; e2e reads exact released-message AX text, so removal fails, and retains the never-inserts assertion. Queue invalidation regression is missing (finding 1).
8. Git and PR: notes map criteria; milestone included in feature commit, identified as owner-approved; no guard edits.

Validation: ran core/pipeline nextest suite; final outcome recorded in PR comment.
VM execution delegated to the separately dispatched Tester; no VM run here.
