Reviewer: CHANGES REQUESTED

# U4 review, round 2

PR: #251 at a071d4fe4f96778bfc95cfc1ceb228b50e8b2eda
Scope: changes since round 1 only; no merge.
Verdict: CHANGES REQUESTED — 0 blockers, 1 major, 0 minors.

## Previous findings

| # | Severity | Status | Evidence / required change |
| - | -------- | ------ | -------------------------- |
| 1 | blocker | Fixed | The Lead/Design Lead-approved “Paste sent” wording is consistent across model, payload, UI and e2e checks. It means the inserter returned success, not confirmed target delivery. The 800 ms timer, error suppression and nothing-to-insert paths remain covered. |
| 2 | major | Open | `src-tauri/src/overlay.rs:185` calls `levels.take()` before `emit_to`; `take` clears the in-flight flag before WebKit consumes the script. Main-thread scheduling is now bounded, but submissions to a busy WebContent process remain unbounded by the dispatcher. Hold delivery in flight until JavaScript evaluation completes, retain one latest pending sample, and invalidate pending work across recording generations. Test a responsive native executor with JavaScript completion withheld. |

[Finding 1 reply](https://github.com/gargankit90/dictation-app/pull/251#discussion_r4465349258); [finding 2 reply](https://github.com/gargankit90/dictation-app/pull/251#discussion_r4465349470). Threads remain unresolved because the overall review still requests changes.

## Why finding 2 remains

The fix correctly coalesces offers while the **native main thread** is busy. However, that thread and the page's JavaScript execution are different completion boundaries. Installed Wry 0.57.1 `wkwebview/mod.rs:842` calls `evaluateJavaScript_completionHandler` and returns without waiting; Tauri's `emit_to` path uses evaluation without a completion callback. Moving that call onto the main thread does not wait for JavaScript consumption.

The remaining sequence is: offer → native task runs → take clears scheduled → submit script to WebKit → next offer schedules another task. If WebContent is stalled while the native thread remains responsive, this repeats at 20/s (up to about 6,000 submissions during five minutes). Closing drops the Rust pending sample but cannot remove scripts already submitted. They can replay on recovery and delay the visible phase change. This is a source-level queueing finding, not a measured VM memory regression.

The new five-minute test stops the native delivery before `take`, so it proves the improved native queue bound but misses this sequence. Tauri 2.12.5 exposes `Webview::eval_with_callback`; completion-based delivery can preserve the overlay's listen-only capability without adding a page command. The exact implementation is the Developer's choice.

## Focused checks

- Final pending level before Transcribing: dropping it is correct; the meter no longer belongs to the active view.
- Stop/restart: `close` clears pending while retaining the scheduled flag; the old native task can drain a new recording's latest sample, or nothing if no new sample exists. This avoids a second queued native task and does not retain an old Rust sample. Already-submitted WebKit scripts remain finding 2.
- Main-thread load: the dispatcher bounds waiting native tasks and recovers from scheduling failure. No new audio-thread work, focus changes, capabilities, logging, persistence, analytics or dependencies were introduced.
- E2e changes: the underscore parser and `paste_sent` expectations match the payload. Partial Listening normalization and blank-before-hidden normalization retain checks for unexpected intervening states. Target-text assertions remain in the general paste scenario; the pill scenario honestly records delivery uncertainty.
- Checklist scope/correctness/practice/tests: finding 2. Privacy, architecture, licences and Git scope: unchanged/OK for this delta. No new minor issues.

## Validation

- `cargo test -p dictation-pipeline overlay::`: 29 passed.
- `npx vitest run src/overlay`: 56 passed across 2 files.
- `bash -n` on both changed overlay scenarios and `git diff --check`: passed.
- Full gate runs through the mandatory push hook. VM execution is left to the Tester/Lead; no owner-screen interaction or product/test edits made.
