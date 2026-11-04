Reviewer: APPROVED

# Launch at login review, round 2

PR: #237 at 558a1c85d7a4308cb0686ecd6b4bca1a340f6ff6
Verdict: APPROVED

## Scope and finding disposition

Re-checked only round 1 finding 1 and its fix in 558a1c8, including the new runner regression case. Finding 1 (major) is fixed: the saved-notice call now supplies `VM_TERMINAL_TIMEOUT=60`, which reaches `vm_terminal` through `ax_ok` and `ax`, including the command-substitution subshell. The 45-second condition wait can finish before the outer 60-poll deadline.

Both bounds remain finite. The UI helper polls for the actual notice and returns early; it fails when the notice is absent at its deadline. `ax_ok` preserves that failure status, and the scenario's `expect` records it as a failure. The outer harness still cancels jobs that do not finish. The comment correctly describes the outer bound as polls rather than an exact wall-clock limit.

The existing checkbox, reopened Settings, OS registration, and reboot assertions remain unchanged. The fix therefore does not accept a launch-at-login setting that failed to change. No product code changed.

## Findings

No open blocker, major, or minor findings in the re-reviewed change.

## Checklist

- Scope, correctness, and tests: OK for the timeout override and regression test; round 1 finding 1 closed.
- Security/privacy, architecture, Rust/TypeScript practice, licences/reuse, and Git/PR: unchanged from round 1; no new concern in this test-only fix.

## Validation

- `bash -n tests/e2e/scenarios/e2e_launch_at_login.sh tests/e2e/test-runner.sh`: passed.
- `bash tests/e2e/test-runner.sh`: 102 runner checks and 32 ci-run checks passed. The new `term_slow_save` case checks that a 35-second job fails with status 124 at 30 polls and completes at 60 polls; it also checks the scenario applies the override.
- Inspected the absent-notice error propagation through `waitFor`, `ax_ok`, and the scenario; no swallowed failure or unconditional success was introduced.
- Tester reports 3/3 VM scenario passes on this head, with off-save notices after 39, 13, and 12 seconds; the 39-second run exercises the boundary from the original finding. This is tester evidence, not a reviewer VM run.
- The required pre-push hook runs the full quality gate before delivering this review commit; delivery status is reported separately.

No merge performed.
