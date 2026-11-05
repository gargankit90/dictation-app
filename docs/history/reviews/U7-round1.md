Reviewer: CHANGES REQUESTED

# U7 review, round 1

PR: #267 at e5e0902d01d1216ea32d877559aedfd2eef9aeb3
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | crates/app/src/history.rs:114 | A failed read, invalid JSON, or unsupported newer file version becomes an empty, loaded history. The next dictation atomically replaces the original file, permanently losing recoverable entries; even a transient read failure is never retried. The damaged-file test explicitly asserts this destructive behavior. | Distinguish missing from unreadable/unsupported files. Preserve the original in a private recoverable file or disable persistence until recovery succeeds; surface a safe error and add regression tests proving the original survives subsequent dictation. Explicit Clear/Off must still delete retained history. |
| 2 | major | src-tauri/src/tray_menu.rs:468 | Copy last reads History on the main-thread tray callback before spawning its worker. History holds its state mutex throughout write_all, sync_all, rename and delete. A concurrent dictation or Settings write therefore makes this callback wait on disk I/O, freezing the UI for the duration of the filesystem operation. refresh also obtains this mutex and is callable from main-thread click handlers. | Move the history lookup into the copy worker and keep main-thread status reads independent of the persistence lock (for example a short-lived state lock plus serialized persistence). Test a paused store write while a status read completes. |

## Requested focus

- File creation uses create_new with mode 0600 before writing, syncs contents and renames within the same directory. Reads tighten an existing file; writes never reuse a leftover temporary file. No audio, app name or window field is introduced.
- Recording does not record SecureInput failures, for either enabled or Off history. Other failures have pasted=false. Entry Debug hides text and length; parser messages are discarded. History logs counts and limits only. Analytics and reporter input remain closed enums/codes; diagnostics uses an allow-list.
- The TRACE history test exercises successful and failed writes and malformed input, checks sentinel fragments and length, and has positive controls. The spawned test passes real history through Recording and checks stored text positively before checking logs, telemetry, reporter codes and report preview. It deliberately continues to allow the existing pipeline transcript-length debug logs, per the Lead clarification in PR notes. The existing Sentry envelope allow-list test supplies the payload-level protection; the history test itself checks reporter codes, not an actual envelope.
- State mutations serialize dictation, trim, Delete and Clear under one mutex. Settings writes serialize separately, preserve unrelated fields, and general Save preserves the stored history limit. Failed Clear/Delete retain the entries; failed Off stops recording and reports failure while disk cleanup awaits retry/relaunch. See finding 1 for load failure behavior and finding 2 for lock contention.
- Recording::finish forwards exactly once to Box<dyn TextInserter>::finish, which forwards exactly once to the real inserter. It replaces Remembering rather than nesting it, so there is no double restore. PR #265 edits the now-deleted last_dictation.rs: expect a modify/delete conflict when combining the branches; retain Recording and its forwarding regression test, without resurrecting Remembering. The normal pending-restore implementation remains unchanged.
- --debug-fail-paste is accepted only with TEST_FLAGS (cfg!(debug_assertions)); release parsing ignores it. Both parser settings are tested.

## Checklist

1. Scope: OK against U7; the spec was added separately in the Lead spec commit. Manual criterion 8 remains the Lead's responsibility.
2. Correctness: findings 1 and 2. Missing-file, limit, trim, Off, Clear and write-failure behavior otherwise inspected.
3. Security and privacy: OK on inspected production paths, subject to preserving 0600 and deletion semantics in the requested recovery fix. No new network destination or sensitive analytics field.
4. Architecture: OK. Logic in core/app, storage behind a port, OS permissions in platform; no new unsafe code.
5. Rust and TypeScript practice: finding 2 (main-thread blocking). Typed IPC and strict UI types retained.
6. Licences and reuse: OK. No new dependency or external adapted code.
7. Tests: finding 1 needs recovery regressions; finding 2 needs a concurrency regression. History/App UI tests: 32 passed. Rust history tests and push gate results recorded below.
8. Git and PR: OK. Conventional commits, detailed PR notes with explicit verification limits, no guard edits. No minor findings requiring follow-up issues.

## Validation

- `npx vitest run src/settings/History.test.tsx src/App.test.tsx`: 32 passed.
- Rust history tests and the mandatory push gate: pending at report creation; final command results will be posted with the verdict.
- VM scenarios were not rerun by this reviewer. This is a code review, not completion of the Lead's manual checks.
