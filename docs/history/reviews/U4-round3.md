Reviewer: APPROVED

# U4 review, round 3

PR: #251 at 1ada0cce428ba16dcea9792b82d6bdc88dbbab81
Scope: round 3 delta against the remaining dispatch major, ACL, lifecycle and crash paths only.
Verdict: APPROVED — 0 blockers, 0 majors, 1 tracked minor. No merge.

## Findings and previous findings

| # | Severity | Status | Evidence / follow-up |
| - | -------- | ------ | -------------------- |
| 1 | blocker | Fixed | The accepted “Paste sent” wording and insertion-error suppression remain unchanged. [Fixed reply](https://github.com/gargankit90/dictation-app/pull/251#discussion_r4466206027). |
| 2 | major | Fixed | `LevelDispatch` and the pushed `overlay-level` stream are removed. A single atomic stores only the newest reading; a stalled page issues no continuing stream of requests or scripts. The Lead explicitly authorized polling in place of spec AC2's original no-polling condition. [Fixed reply](https://github.com/gargankit90/dictation-app/pull/251#discussion_r4466206436). |
| 3 | minor | Tracked | `src/overlay/Overlay.tsx:42`: `waiting` is scoped to an effect lifetime. An unanswered read followed by Transcribing and another Recording permits another read before the first settles. The old answer cannot modify the new meter, and this does not restore the continuous pushed-level backlog. Preserve a shared in-flight guard across lifetimes and test a deferred reply across stop/restart: [tech-debt #257](https://github.com/gargankit90/dictation-app/issues/257). |

Both original threads are resolved. The minor's inline thread is resolved because it is tracked in #257.

## Focused review

- **ACL and payload:** the application ACL manifest now includes the separate `OVERLAY_COMMAND_NAMES`; only `capabilities/overlay.json` grants `allow-get-overlay-level`. Main retains its explicit settings command list. The command takes no user arguments and returns only the atomic numeric level, without the device name, text, audio, key or settings. Permission tests exercise real IPC acceptance/refusal, not just JSON text matching.
- **Polling and lifetime:** animation frames gate requests at least 50 ms apart, and `waiting` prevents overlap within a metering lifetime. Listening → No sound keeps the same loop. Transcribing, errors, hidden and unmount cancel the scheduled frame, mark the old callback inactive and flatten the next meter. A late reply is ignored. A document reload destroys its JS loop and starts hidden until the existing listener-title handshake replays the current view. Minor #257 records the cross-lifetime exception to the global one-outstanding claim.
- **Atomic and new recordings:** stores/loads are single-word operations; relaxed ordering is sufficient for a standalone numeric value. The overlay runner serializes state and level handling, only stores levels while its model is recording, and clears the atomic on non-recording views. Cleanup clears the page's bar history. No lock, channel queue, audio allocation or work on the real-time capture thread was added by this delta.
- **Crash check:** `get_overlay_level` performs one atomic read. Its `OverlayLevel` state is managed before creating the window; it does not unwrap an overlay handle, take a mutex, schedule main-thread work or perform native window calls. New production code contains no panic/expect/unwrap path. The build-time `Box::leak` creates the static ACL name list once at build time, not per request. Finite readings originate from the existing level maths; the page rejects non-finite/malformed responses. I found no source-level explanation for the Developer's one-off VM exit and cannot rule out a separate native/platform crash without its crash report. The Tester/Lead should capture DiagnosticReports and the last process log if it recurs; three reported non-reproducing runs do not establish a cause.
- **Checklist:** scope/correctness/tests are OK with the tracked minor and explicit Lead override. Privacy, architecture, licences, focus construction and error/success behavior have no new concern in this delta. No guards or product/test files edited by the reviewer.

## Validation

- `npx vitest run src/overlay`: 60 passed across 2 files, including rate, unanswered read, stop/error/hide/unmount, ACL source checks and malformed replies.
- `cargo test -p dictation-pipeline overlay::`: 28 passed, including latest-value and concurrent-reader checks.
- `cargo test -p dictation-app commands::capability_tests`: 12 passed, including the overlay-allowed/main-refused level command check.
- Full quality gate runs through the mandatory push hook before publication.
- `git diff --check`: passed. No independent VM execution in this review; Developer's VM results are supporting evidence, not claimed as my runs.
