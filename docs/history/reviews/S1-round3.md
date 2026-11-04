Reviewer: APPROVED

# S1 review, round 3

PR: #255 at 9800a993d6315604f2bb0341493dad3befb8bba9

Scope: R3 and the changed completion/start handoff only. R3 is fixed; no new blocker, major, or minor findings. R1 and R2 remain resolved from round 2. No merge performed.

## R3 re-check

The completed change updates the known result and finishes its store callback before taking the pending mutex to decide idle versus queued work. When idle, `running` and `applying` are cleared before any waiter is notified. Thus an answered caller with no newer request sees the completed operation as idle, removing the observed `Some(false)` versus `None` race.

A request during the store still queues behind the existing worker. A request racing with the final mutex acquisition either queues before the idle decision (the same worker continues), or observes `running = false` afterward and starts a new worker. In that latter case the old worker has no remaining OS or store work; it only notifies its captured waiters and exits, so it cannot overtake the newer store or clear the newer pending state. The UI's existing choice counter rejects older results if a new choice was made in that interval.

The new `an_answered_caller_sees_the_idle_state` test performs 300 sequential request/await cycles, checking both `pending() == None` and that each result was stored before notification. It asserts the completion invariant without a timing sleep. The original delayed-request test retains the assertion that caught R3. Developer-reported stress evidence is 500 iterations of both tests passing; this reviewer independently runs the focused launch tests and the normal push gate.

## Checklist delta

Correctness, synchronization, and tests: OK for the changed lines. Scope, architecture, security/privacy, licences, and UI behavior have no new relevant changes. Reviewer changes are limited to the historical round-2 report rebased onto this head and this round-3 report. No minors requiring follow-up issues.

## Validation

- `cargo test -p dictation-pipeline launch:: -- --nocapture`: passed (20 launch tests).
- Hosted CI was pending when inspected; no independent VM run for this synchronization-only change.
- Publication uses the full normal pre-push gate; any signal-test retry follows the Lead's #258 instruction without skipping hooks.
