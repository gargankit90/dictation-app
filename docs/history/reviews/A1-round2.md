Reviewer: APPROVED

# A1 review, round 2

PR: #220 at 990cd63e426e641a638d75ed3cf8877785b10ec8

## Findings

No open blocker, major or minor findings in this re-review.

## Round 1 findings

1. **Blocker — extracted batches survive opt-out: fixed.** `crates/telemetry/src/lib.rs:175` takes a generation-tagged batch and subscribes to cancellation under the state lock. Disabling clears the queue/ID, increments the generation and signals subscribers. `send_batch` checks that generation and selects the request against cancellation, dropping the pending request on opt-out. Off/on also invalidates old-ID batches. Failure cleanup checks the generation so an old cancelled request cannot erase a new opt-in's queue. No network await holds the state mutex. A request already delivered cannot be recalled.
2. **Major — Save undoes pending opt-out: fixed.** `src-tauri/src/commands.rs:285` preserves the stored sharing choice when saving the ordinary form. Its read/write and the dedicated sharing command use the same write mutex on the blocking pool. Thus either ordering of the two commands preserves opt-out. `write_sharing` disables the adapter before persistence; enabling follows successful persistence. `src/Settings.tsx:277` uses only the dedicated command for the checkbox, so a stale form payload cannot re-enable analytics.

## Scope and checks

Re-reviewed only the diff from the round 1 review commit through `990cd63`, the two original findings and the changes' integration. The event schema and destination are unchanged. Checked the new typed binding, command registration and main-window-only capability; the overlay gains no permission. Checked queue cleanup across off/on and failure, no-key behavior, bounded delivery, and settings error propagation. No product/test/spec/guard files edited by the reviewer. No merge performed.

Validation:

- `cargo test -p dictation-telemetry`: 17 tests passed, including extracted-batch opt-out, off/on invalidation and cancellation of a pending request.
- `cargo test -p dictation-app commands::tests::sharing --lib`: three sharing tests cover field isolation and stale Save payloads.
- `npx --no-install vitest run src/Settings.test.tsx`: 68 tests passed, including Save during a pending opt-out reply.
- The required pre-push hook runs the full quality gate for this review commit. CI and the Tester's round 2 report remain independent merge prerequisites.

No new tech-debt issues.
