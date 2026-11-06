Reviewer: CHANGES REQUESTED

# U12 review, round 1

PR: #290 at 36d97b4b4fc8219c78e89294a180d91004293e98

## Findings

| # | Severity | Location | Finding | Suggested fix |
| - | -------- | -------- | ------- | ------------- |
| 1 | major | `src/settings/ownKey.tsx:111`; `src/Settings.test.tsx:801` | With Groq working, open Change key and select OpenAI with no saved OpenAI key. `changeProvider` immediately calls `useCloudEngine`, persisting OpenAI and replacing the working engine before any successful test. Done leaves dictation broken; a rejected test does not restore Groq. The new test explicitly expects this premature switch. Although inherited from the previous panel and disclosed in the PR, it violates this review's explicit requirement that the sheet cannot switch the engine before a key works. | Keep the provider popup as a draft; retain the working engine until the chosen provider has a usable key, and switch after successful Test & Save when appropriate. Replace the premature-switch assertion with tests for no key, rejected key, cancellation, and successful save, while retaining generation protection. |
| 2 | major | `src/settings/ownKey.tsx:350`; `src/settings/Speech.tsx:266` | Done/Escape are blocked only during saving, so the sheet can close during Remove key. Closing calls `abandon`, which increments `keyRun`; when deletion finishes, `removeKey` returns at its stale-generation check before refreshing key status or reporting failure. The same mounted Speech hook still says saved when the sheet reopens, even after successful deletion; a deletion error is also silently lost. Opening the sheet does not refresh key status. | Treat removal as an in-flight committed write for dismissal purposes, or reconcile its result and refresh saved-key state independently of abandoning typed input. Add deferred deletion tests covering close/reopen on both success and failure. |

## Implementation checks

- **Navigation / #277:** consumed section requests are cleared by `onOpened`; monotonically increasing request IDs still distinguish repeated requests. A request received on Troubleshoot or Report returns to Settings and applies its section. Back uses the remembered section rather than replaying History. The two regression tests would fail if consumption were removed. Report has its own preview/actions and returns to its actual source; page titles use a restricted typed command.
- **Speech:** missing, partial, downloading, loading and failed rows remain represented. Radios for incomplete downloads are disabled with reasons; local selection preloads through the existing backend before persisting. Existing key generation and lookup guards remain; closing the sheet invalidates an unfinished test and clears typed input. Keys stay in the password input during entry, are not read back from Keychain, and no new logging or report exposure was found. Findings 1 and 2 cover premature switching and deletion reconciliation.
- **Automatic sheet:** opening once again after returning from Troubleshoot is a consequence of a new Settings mount. It is limited to Speech with the cloud engine selected and no key; it neither switches an engine nor replays an old tray request. This does not violate a stated cross-mount persistence requirement, so it is not a blocker.
- **Menu / updates:** the normal complete-setup menu matches the approved rows. Incomplete setup retains Resume setup; attention rows still map through distinct fix IDs. Removed report/update/troubleshoot IDs have no action mapping (the separate fix-troubleshoot ID remains valid). The launch updater still offers available updates via its native dialog; General's Check now invokes `UpdateCheck`, wired to the same updater. No removed menu-item lookup is needed.
- **Recorder:** `compact` defaults to false and is supplied only by Settings. Setup's Try it recorder keeps its existing branch and behavior.
- **Tokens / docs / report:** contrast assertions cover light/dark primary, hover and selected text; light values alias the old values. Website changes only add unused copies of the new token definitions, consistent with the existing shared-token convention, and do not alter its rendered colors. TESTERS, releasing and privacy edits describe the actual menu, update wiring and allow-listed report. Repository search found no consumer parsing the old `hotkey:` report label; internal settings fields remain `hotkey`.
- **Keychain notice:** confined to Speech with a saved key; Got it writes localStorage and its initializer reads that value on subsequent mounts. Storage failures fall back to dismissal for the current mount.

## Adjacent PRs

Read-only `git merge-tree --write-tree` simulation against the fetched heads:

- **U9 #285:** conflicts in `crates/app/src/hotkey_setting.rs`, `src/Settings.test.tsx`, `src/settings/Dictation.tsx`, and `src/validation.ts`. Preserve U9's keycap rendering and pending-Accessibility notice together with U12's dictation-key wording and compact recorder. In particular, retain U9's notice under the row rather than resolving the entire `below` property to U12's SaveStatus alone. Regenerate/check bindings and run both sets of tests after resolution.
- **U10 #279:** simulation merges cleanly. Shared bindings, command/capability registration, CSS and privacy docs still warrant the normal combined gate; no specific semantic collision found.

## Validation and test quality

- Installed dependencies with `npm ci` because this fresh worktree had no node_modules.
- `npx vitest run`: 16 files, 359 tests passed; `npx tsc -b --noEmit`: passed.
- Full `scripts/gate.sh`: passed, including Rust tests, coverage thresholds, frontend build, 359 Vitest tests, guards and script checks. No rlib-format failure occurred.
- No VM computer-use or real-provider testing performed by this review; those remain Tester/Lead evidence.
- No reproduction of the previously unidentified loaded-machine Vitest failure. The key-race tests use fixed 50 ms waits before negative assertions (`src/Settings.test.tsx:796,831,865,887,908`), which is a timing-sensitive test pattern and weak evidence that every obsolete continuation has completed; replace with an awaited completion barrier when revising those tests. This is an observation, not an identified failing test or an additional blocking finding.
- Model state tests, navigation regression tests, key-sheet save/failure tests, menu assertions and calculated contrast checks exercise the changed behaviors. Finding 1's test currently protects the wrong behavior and must be corrected.

## Checklist

1. Scope: finding 1; owner-approved menu, labels and wording accepted.
2. Correctness: findings 1 and 2; remaining examined paths OK.
3. Security and privacy: OK; no new destinations or sensitive fields.
4. Architecture: OK; inward dependencies and platform boundaries preserved.
5. Rust and TypeScript practice: OK in reviewed changes.
6. Licences and reuse: OK; no new dependency or apparent copied implementation.
7. Tests: findings 1 and 2; validation limits and timing observation above.
8. Git and PR: OK; no guard edits, screenshot evidence supplied, merge remains gated on Tester/Lead/Design Lead sign-off and the corrected implementation.
