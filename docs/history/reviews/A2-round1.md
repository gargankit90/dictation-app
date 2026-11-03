Reviewer: CHANGES REQUESTED

# A2 review, round 1

PR: #228 at d78b535ef6eecb5b451f1fbc11933cf87cd14985.

## Findings

### Major — R1: a concurrent panic can recreate the queue after opt-out

`crates/telemetry/src/errors.rs:296-316` (write at line 316), together with
`set_enabled(false)` at lines 238-242.

A background transcription worker can panic while Settings turns sharing off.
`record_panic` checks `enabled` at line 297 but releases the mutex before building
and writing the envelope. The opt-out can then set `enabled = false`, delete the
pending file and return; the panic hook subsequently writes that file back.
Worker panics need not terminate the app (the pipeline handles panicking jobs).
Turning sharing on later reads and sends this old report at lines 233-236,
although opting out promised to discard it. The pending report also remains on
disk while sharing is off. This violates criterion 4 and the deletion promise in
Settings and docs/privacy.md; the existing tests cover only sequential writes
and opt-out.

Suggested fix: serialize the final enabled/generation check and pending-file
write with opt-out deletion, so an old-generation panic cannot commit its file
after deletion. Keep the potentially expensive stack parsing outside that
critical section. Add a controlled concurrent regression test that pauses panic
recording after its first enabled check, opts out, resumes recording, and checks
that neither a pending file nor a later upload survives.

No blockers or minors raised; no new tech-debt issues.

## What I checked

- Scope: A2 criteria 1-7 mapped to the diff and PR notes; native minidumps are
  explicitly deferred to #226 because their memory cannot be scrubbed.
- Correctness: R1; reviewed DSN gating, scripted-launch suppression, error
  forwarding, off/on, request cancellation, single-attempt delivery, four
  in-flight requests and the 30-attempt cap. No unbounded retry queue found.
- Security and privacy: R1; ordinary errors flow only through E1 ErrorCode and
  A1 ErrorType. The panic hook does not pass the panic payload to the envelope
  builder; checked stack/location path handling, event fields and non-redirecting
  TLS client. No error-chain, Debug, transcript or key serialization found.
- Architecture: sibling ErrorReporter port and fake preserve inward dependencies;
  no new OS cfg, unsafe or third-party dependency introduced.
- Rust and TypeScript: reviewed the whole diff, typed settings wiring and shared
  switch. Network sends are spawned rather than awaited by dictation or quit.
- Licences and reuse: no new external crates or apparent copied implementation.
- Tests: all nine `cargo nextest run -p dictation-telemetry errors::` tests passed.
  They do not cover R1's concurrent interleaving. All release-script stub checks
  and 68 Settings tests passed; no owner's Keychain read. The full gate result
  is recorded below.
- Git and PR: reviewed Conventional Commit history, acceptance table and risks;
  no product, test, spec or guard edits made by the reviewer. No merge performed.

Validation: `scripts/gate.sh` passed in this worktree (exit 0), including
coverage, dependency licences, all workspace tests, frontend checks and the
release/guard/runner stub suites.
