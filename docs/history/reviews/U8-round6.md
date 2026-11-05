Reviewer: APPROVED

# U8 review, round 6

PR: #269 at 11827a39089dcfdf07ba7a3ab289201d616e030e

## Findings

No open blocker or major findings in this re-check. Round 5's cross-view sharing race is fixed.

## Window lifecycle and fix

- `src-tauri/tauri.conf.json` creates one `main` window. `window::show` in `src-tauri/src/window.rs` retrieves that existing webview; it resizes, shows and focuses it without navigation or recreation.
- Tray Settings calls `show_settings` (`lib.rs`), closes the logical onboarding view, and shows `main`; Resume setup calls `show_onboarding` and shows the same `main`. `App.tsx` swaps React views there. The close handler prevents destruction and hides the window. These paths do not reload the webview.
- The separate overlay is not another sharing writer: its capability grants only event listening and the level command. Other window builders in shell command tests are test-only. Onboarding and Settings do not run concurrently in separate production webviews.
- `sharingQueue.ts` owns the pending intent and drain loop outside either component. Only one IPC write runs at a time; later choices replace the waiting choice, and choices arriving during the settings reread cause another drain. Unmounting a view neither resets nor cancels the queue. Every sharing switch uses this writer.
- `useSyncExternalStore` exposes the pending state, latest choice and failure to either view. Onboarding's existing exit guard consequently waits for writes originating in Settings too. A failed last write continues to block exits.
- Backend stale-write rejection is not required for these view transitions. A future additional settings webview or a reload while writes are pending would need a cross-webview ordering mechanism; this approval does not claim the module queue covers those lifecycles.

## Regression coverage

The new deferred-IPC test unmounts onboarding with its opt-in held, mounts Settings Privacy, chooses off, and verifies that the off write is not dispatched concurrently. Releasing the old write then the final write leaves stored sharing off and records calls in on/off order. It would catch the former independent per-view queues. Additional tests verify that a pending Settings write blocks onboarding Continue and that a mounted view learns the final stored state.

## Checklist

1. Scope: OK; re-check limited to round 5's blocker and changed code.
2. Correctness: OK; shared queue and production window lifecycle inspected.
3. Security and privacy: OK; no independent sharing writer in the overlay, no new data or logging.
4. Architecture: OK; no dependency, capability or backend change.
5. Rust and TypeScript practice: OK in changed code; typed commands and external-store subscription.
6. Licences and reuse: OK; no new dependency or copied code.
7. Tests: focused frontend results recorded below; full gate runs through the push hook. VM checks remain with Tester.
8. Git and PR: only this report edited; no guard changes or merge performed.

Validation: `npx vitest run src/Onboarding.test.tsx src/Settings.test.tsx src/onboarding/logic.test.ts` passed all 142 tests across three files.
