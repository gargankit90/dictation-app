Reviewer: APPROVED

# T8 review, round 1

PR: #227 at e5f421b9ccd185b97d8191b12016e255076c840e
Verdict: APPROVED

## Findings

No blocker, major or minor findings.

## Checklist

1. Scope: OK. Both reliability changes match the Lead's T8 spec; the spec
   commit precedes implementation. The PR records five passing VM runs and
   the before/after macOS job times.
2. Correctness: OK. The readiness loop has ten attempts by default; each
   remote read also has the existing VM command deadline. Its advertised
   ten seconds is a polling budget, not a strict wall-clock deadline when
   VM commands are slow. Failed or unreadable document checks cannot pass
   readiness. The existing double activation in `textedit_fresh` handles
   the key-window setup before this additional check. Readiness failure
   persists in `SCENARIO_FAILED`; the later text and phase assertions
   remain mandatory, so a failed paste cannot be masked by the wait.
3. Security and privacy: OK. Hosted runners, top-level `contents: read`,
   pinned actions, checkout without persisted credentials and no secret
   reads remain intact. No privileged fork event or owner-machine access.
4. Architecture: OK. No production architecture or dependency changes.
5. Rust and TypeScript practice: OK. No Rust/TypeScript changes or new
   suppressions; the shell helper returns failure after exhausted retries.
6. Licences and reuse: OK. No dependency or adapted-code additions.
7. Tests: OK. Locally, runner tests passed 99/99 (including 32/32 CI-run
   checks), and the workflow policy checker passed both workflows. Direct
   shell probes passed for ready TextEdit, wrong frontmost app, and an
   unreadable document. Hosted run 41710496190 passed both required jobs,
   including all three added transcribe steps on Apple Silicon. The existing
   rust-cache precedes those steps and retains dependency build artifacts;
   macOS time is 4m39s versus 2m33s before. VM runs were reported by the
   author, not independently repeated in this code review.
8. Git and PR: OK. Focused Conventional Commits and complete PR notes; no
   guard changes or untracked minor findings. Review publication does not
   authorize merging; this task explicitly excludes a merge.

Review file: [T8-round1.md](T8-round1.md).
