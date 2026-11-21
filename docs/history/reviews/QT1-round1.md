# MQT1 review, round 1

PR: #340 at 01b35699cef5b142bb8c81553fe6270e6577a810
Verdict: APPROVED

## Findings

No blocker, major or minor findings.

## Checklist

1. Scope: OK. QT1 adds cleanup coverage and the outcome log; existing Q2 tests cover status mapping, truncated replies and raw fallback. The milestone is a separate spec commit preceding implementation.
2. Correctness: OK. The cleanup result and fallback mapping are preserved, including the cleanup-off early return. Cancellation is checked by dropping the pending future and successfully cleaning the next dictation.
3. Security and privacy: OK. The new info event contains only a static outcome name and elapsed milliseconds. No text, key, provider error, identity or path enters that event. HTTP leak tests exercise successful and failing speech, cleanup and key-check calls, including provider bodies echoing an explicitly fake key.
4. Architecture: OK. No dependency, OS conditional, unsafe block or Tauri boundary change.
5. Rust and TypeScript practice: OK. Production changes are small and typed; test-only unwraps remain in test code. No TypeScript changes.
6. Licences and reuse: OK. No new dependency or apparent external adaptation.
7. Tests: OK. Tests use the real pipeline, guard and HTTP clients with controlled ports/server replies, asserting inserted text, wire requests and cancellation. The new pending cleanup test waits for the actual timeout rather than sleeping to race completion. Existing harnesses are reused; transcribe leak tests share one capture helper. The app capture writer stays local to its separate crate. The Perl fixture preserves default echo behavior and the existing speech hold events; provider startup clears scenario control files. VM scenarios check exact TextEdit contents. The eight-second VM bound includes recording, process launches and remote assertions; the pipeline test separately exercises cancellation at its configured deadline.
8. Git and PR: OK. Conventional Commits and acceptance mapping are present; no guard changes. Full VM-suite acceptance and the Lead's real-key manual check remain separate from this review. Do not merge on this review alone.

## Validation

- Perl fixture syntax and changed shell script syntax: passed.
- Focused Rust checks: 37 passed across `cleanup_q2`, `cleanup_guard`, `openrouter_wire`, `openrouter_q2`, `engine_leaks` and `openrouter` using `cargo nextest run`.
- No VM or real-provider run performed by the Reviewer; those belong to the Tester and Lead.
