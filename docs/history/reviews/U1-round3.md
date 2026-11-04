Reviewer: APPROVED

# U1 review, round 3

PR: #250 at 4b2f3e3105d24f3b504e4ec38b5ba98be1513c67

Rechecked only the two round-two majors and their fixes. Both are fixed; no new findings in these changes. Counts: 0 blockers, 0 majors, 0 minors.

## Findings resolved

1. **Troubleshoot Relaunch failure handling:** the handler now awaits the typed command result, displays a returned error inline with `role="alert"`, and catches a rejected IPC promise with fixed quit-and-reopen advice. The two new tests exercise a string rejection (mapped by generated `typedError` to `status: error`) and an Error rejection (caught by the handler), covering the actual distinction at the bridge. Retrying clears the prior error. The existing restart command is unchanged.
2. **Keychain privacy assurance:** the launch notice, CHANGELOG, tester guide and Settings test now use the Design Lead-approved wording. It accurately says the key stays saved in Keychain and is sent to the speech provider, replacing the false promise that it never leaves the Mac. The Rust copy assertion requires the provider wording and rejects “never leaves.” No marker/detection behavior changed.

## Validation

Targeted Settings and Troubleshoot tests: 93 passed, including both new relaunch failure cases. Full scripts/gate.sh passed, including 1,219 workspace tests, coverage thresholds, frontend checks/build, guards and shell helpers; no signal-test retry was needed. Review is source and automated-test based; no VM GUI was driven and no unrelated process was stopped.

## Checklist (changed scope only)

1. Scope: OK; only the requested failure handling, copy and associated tests changed.
2. Correctness: OK; returned errors and bridge rejection both provide recovery advice.
3. Security/privacy: OK; the copy accurately describes existing provider authentication; no secret data is exposed.
4. Architecture: OK; existing typed IPC and command paths retained.
5. Rust/TypeScript practice: OK; errors are handled in the UI.
6. Licences/reuse: OK; no dependencies or adapted code added.
7. Tests: OK; both error branches and revised notice copy covered.
8. Git/PR: review-only commit; no guards changed. Earlier findings are fixed, and their threads receive fixed replies and resolution. No merge performed.
