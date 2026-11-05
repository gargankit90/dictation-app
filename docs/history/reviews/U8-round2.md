Reviewer: CHANGES REQUESTED

# U8 review, round 2

PR: #269 at e077fb0fb2c447d3f112a8cc16ed4744bc23c936
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | blocker | src/onboarding/WelcomeStep.tsx:40 | Round 1's opt-out ordering requirement is only partly fixed. The switch stays interactive while saving, and `useSave.track` starts every write immediately; its generation counter guards only the displayed save status. Toggle off → on → off while writes are pending. If the final off finishes before the earlier on, Continue unlocks even though on is still pending. The blocking pool/settings mutex provides mutual exclusion, not invocation order: the stale on can then overwrite the final opt-out and enable sharing. Continuing reads that stale true value and sends events. Stale callers also still execute `onStored` and `setShown(null)`. The added tests hold only one write, so miss this case. | Serialize switch writes in user order (or prevent another toggle until the current write settles), and keep Continue blocked until the final choice succeeds with no older write outstanding. Add deferred multi-write coverage proving stale opt-in cannot overtake the final opt-out or re-enable adapters/events. |

## Re-check of round 1

1. **Partly fixed; remains blocking.** Continue is disabled during one save and after failure. The two new deferred-write tests would fail on the old Welcome code. `leave_welcome` now reads saved settings under the writer's lock and treats unreadable settings as off; its fake-port tests cover stale adapter state. Neither layer orders multiple switch invocations, as described above.
2. **Fixed.** `Resume` is determined from HTTP 206 versus 200 and retained in every progress callback, including completion. `LocalModelInfo` carries the accepted offset/restart fact and `stripView` renders it without guessing from byte counts. The HTTP-200 test checks every callback remains Restarted after progress exceeds the original partial; frontend tests assert the restart wording remains at 400 MB. The old implementation cannot satisfy these assertions.
3. **Fixed for the reported quit/reopen cases.** `SpeechChoice` is non-secret persisted setup progress, exposed through typed IPC and restored independently of saved keys. The frontend tests cover own-key without a saved key, local with a saved cloud key, and discarded unsaved key text; the backend round-trip test now checks the choice at every step. Old code lacks the persisted selection and fails these expectations.
4. **Fixed.** The new standalone harness renders the real onboarding with pseudo-long strings at 680×492 content pixels (680×520 minus the title bar). It measures button/reason clipping, overlap, hit testing, rail/strip overflow and scrolling on six views in both appearances. The old non-wrapping footer cannot meet these constraints for the expanded labels. Developer WebKit screenshots are committed. This reviewer inspected the harness and evidence; did not rerun the VM scenario.

## Auto-resume and merge

The auto-resume fix removes a cancellable zero-delay timer after `resumeChecked` has already become true. A rerender previously cancelled that timer and prevented any retry. The guarded microtask now starts the selected download once; paused, error, Welcome/Speech and already-running/ready cases retain their guards. The fake backend now returns fresh model objects, exposing the dependency rerender that its former shared objects hid; the existing resume tests exercise the fix.

The origin/main merge has no conflict-resolution diff (`git show --remerge-diff`). Its inserter finish forwarding and hidden-window focus changes are preserved alongside U8's practice capture and onboarding sizes. No new merge regression found on inspection.

## Checklist

1. Scope: OK for this round; criterion 2 still blocked by finding 1.
2. Correctness: finding 1; download provenance, persistence and auto-resume otherwise OK on inspection.
3. Security and privacy: finding 1; new persisted values contain no key or transcript, no new destination or telemetry field.
4. Architecture: OK; typed IPC, pure core selection enum, downloader owns HTTP provenance.
5. Rust and TypeScript practice: OK on changed code; existing persistence error policy is unchanged.
6. Licences and reuse: OK; no dependency or external adapted code added this round.
7. Tests: 73 focused frontend tests passed; missing multi-write regression is finding 1. Browser harness inspected, not executed by reviewer.
8. Git and PR: OK; focused Conventional Commits, detailed limitations, no guard edits. No merge authorized by this verdict.

## Validation

`npm ci` and `npx vitest run src/Onboarding.test.tsx src/onboarding/logic.test.ts`: 2 files, 73 tests passed. The full quality gate runs locally and again through the push hook; its final result is recorded in the PR comment. No product or test files changed by the reviewer.
