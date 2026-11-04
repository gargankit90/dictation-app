Reviewer: CHANGES REQUESTED

# S1 review, round 2

PR: #255 at 1f0c79ad0e383fcc8c1921bd3a373570847c88a2

Re-review scope: changes since round 1, especially the store/new-request race and the checkbox after persistence failure. R1 and R2 are fixed and their threads are resolved; one new major finding R3 prevents approval. No merge performed.

## Findings

| Finding | Severity | Location | Evidence and suggested fix |
| - | - | - | - |
| R3 | major | crates/app/src/launch.rs:359 | Replies are delivered before clearing `running`/`applying` under the pending lock. An awakened caller sees a completed response while `pending()` still says the old operation is applying. This failed the full push gate, and a 100-iteration stress run reproduced it twice. Finalize idle-or-queued state under the lock before delivering the captured waiters, keeping OS and storage I/O outside the lock; preserve the new-request handoff and add deterministic completion-order coverage. |

[R3 inline thread](https://github.com/gargankit90/dictation-app/pull/255#discussion_r4466063685).

## Earlier findings

- R1 fixed: the extra OS status read is removed, and both OS operations and the store callback run outside the pending lock. The cached state comes from the change outcome. The new gated-read regression test targets the old third-read path.
- R2 fixed: the store callback returns a write error, which becomes `NotStored` without discarding `Outcome.enabled`. Both on/off messages state the actual OS result and offer retry or Troubleshoot. The technical reason is excluded from user-facing text. Tests cover callback failure, an unwritable settings directory, and successful storage on a later request.

## Race and UI analysis

- During storage, `running` remains true. A new request queues its choice and waiter instead of starting another worker; it cannot overtake the store. The idle/queued decision uses the request mutex, so the handoff otherwise avoids losing a request. R3 concerns delivery before finalizing that state.
- Waiters captured before the store get its outcome; callers arriving during the store get the later outcome. The UI's choice counter rejects older replies and their asynchronous recovery reads, so an earlier store result cannot overwrite a newer toggle.
- On `NotStored`, the OS operation succeeded, so the user's choice matches the OS. The error path re-reads Settings using the pending choice, bounded OS read, or cached real result. If reading the config itself fails, the UI keeps the successful choice and shows the persistence warning. A relaunch may temporarily use an old stored flag while the OS is slow, but the persistence failure is now explicitly disclosed.
- The e2e Applying wait is 150 seconds with an outer 180-poll bound. The independent five-second Settings saved assertion remains unchanged.

## Checklist delta

Scope, architecture, error reporting, security/privacy, licences, and capabilities: OK for the changes reviewed. Correctness and tests: R3. Git: reviewer edited only this report; no guard edits. Zero new blockers, one major, zero minors requiring issues.

## Validation

- `cargo test -p dictation-pipeline launch:: -- --nocapture`: 19 passed.
- Initial full pre-push gate: 1209 Rust tests passed, one failed (`a_request_returns_before_the_slow_os_call_ends`); push rejected by the hook.
- Focused `cargo nextest run -p dictation-pipeline launch:: --failure-output immediate`: 19 passed.
- `cargo nextest run -p dictation-pipeline a_request_returns_before_the_slow_os_call_ends --stress-count 100 --failure-output immediate`: 98 passed, two failed (iterations 49 and 72). At launch.rs:536, `pending()` returned `Some(false)` instead of `None` after the outcome was awaited. This confirms a scheduling race rather than a slow-machine duration assertion.
- No independent VM run; timeout adjustment checked against the Tester run-1 evidence. Hosted CI was pending when inspected.
