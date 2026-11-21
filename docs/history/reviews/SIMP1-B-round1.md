# SIMP1 PR B review, round 1

PR: #349 at 1c6c758d9c5325f95c77bae5b830ce3680a00995
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| R1 | major | crates/transcribe/src/openrouter.rs:218 | Removing the upload-limits module also removes the long-clip timeout retry cap. Clips of at least 60 seconds previously stopped after two timeouts; the default retry policy now permits three attempts. A five-minute recording can wait about 540 seconds instead of 360 seconds on a stalled upload. SIMP1 item 6 explicitly retains tested transport hardening, including long uploads. The removed long-clip retry integration test protected distinct behavior, rather than redundant coverage. | Keep the small timeout counter/cap in the engine while deleting the module, and retain an equivalent long-versus-short timeout retry test. |
| R2 | minor | crates/app/src/launch.rs:425 | The new concurrency test sleeps 50 ms to assume its setter has entered a 300 ms fake operation. Scheduling delays can put the read before the setter starts or after it finishes, failing otherwise correct code. | Use an entered/release barrier in the fake OS adapter. Nonblocking follow-up: [#350](https://github.com/gargankit90/dictation-app/issues/350). |

Inline discussion: [R1](https://github.com/gargankit90/dictation-app/pull/349#discussion_r4653449340), [R2](https://github.com/gargankit90/dictation-app/pull/349#discussion_r4653450417).

## Checklist

1. Scope: R1. The remaining changes match PR B's typed errors, items 6/7/10/12 and launch-at-login scope; PR notes include per-item counts and deferrals.
2. Correctness: R1. Typed codes propagate through coordinator, overlay, tray, key testing and diagnostics; engine switching preserves rollback on load/save failures. Release mode retains the lock and avoids binding the socket.
3. Security and privacy: OK in inspected changes. Error text shown to users comes from the fixed code table; provider-body parsing still avoids quoting the response. Keychain access remains in Rust, bounded and shared. Telemetry retains the closed Event enum; its changed engine-selection payload drops the provider field. Sentry remains separate. No new destinations, capability expansion, clipboard changes or audio persistence found.
4. Architecture: OK. Shared permission/error metadata and encoding logic live in core, OS work remains behind ports/platform, and overlay types are generated. No new workspace dependency, unsafe block or target-OS conditional introduced.
5. Rust and TypeScript practice: OK apart from R2's test reliability concern. Launch-at-login set/read commands use the blocking pool; Settings and onboarding disable the switch during saving. The unified keychain helper retains deadline, panic and stale-read handling.
6. Licences and reuse: OK. No new dependency or claimed external reuse; httpdate and its licence entry are removed.
7. Tests: R1 and R2. Silence-probability tests disappear with the unused Whisper path; encoding tests move to core; typed-code enumeration and mapping tests are present. Existing retry/jitter, no-redirect, long-upload timeout-growth, owner-kept UI and real-OpenRouter scenarios remain, except for R1's distinct removed protection.
8. Git and PR: OK. Conventional commits and detailed notes; no guard, AGENTS.md or milestone-spec edits. R2 has its required tech-debt issue. No merge performed.

## Validation

- `cargo test -p dictation-core -p dictation-ports -p dictation-pipeline --lib`: 422 passed (196 core, 25 ports, 201 pipeline).
- `npm test -- --reporter=dot`: 384 passed across 17 files.
- `cargo test -p dictation-platform single_instance --lib --quiet`: 42 passed.
- Targeted transport tests and the publication hook results are recorded in the PR comment.
- VM e2e and coverage are delegated to the parallel Tester/CI; this review does not claim those checks passed. Lead manual checks remain required before any merge.
