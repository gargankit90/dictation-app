Reviewer: CHANGES REQUESTED

# Launch at login review, round 1

PR: #237 at 1b9c78d14ec838b04ee4e44269cce72513473900
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | tests/e2e/scenarios/e2e_launch_at_login.sh:119 | The 45-second `wait-text` is run through `ax_ok` → `ax` → `vm_terminal`, whose default `VM_TERMINAL_TIMEOUT` is 30 polling iterations (`tests/e2e/lib_full.sh:176,199-221`). No override is supplied here or by the runner. With responsive VM bookkeeping, a save taking the reported 38 seconds is cancelled with status 124 before the notice can appear. The increase therefore does not reliably cover the slow-save case motivating this change. | Set a bounded outer timeout above 45 seconds for this call, with room for Terminal startup and result collection, e.g. `VM_TERMINAL_TIMEOUT=60 ax_ok wait-text ... 45`. Verify success when the notice arrives between 30 and 45 seconds and failure when it never arrives. |

## Checklist

1. Scope: OK. The entire PR diff changes only the launch-at-login e2e scenario; no product code, guards, dependencies, or milestone specifications changed. PR notes explain both races and repeat-run evidence; slow product saving is separately tracked in #238.
2. Correctness: finding 1. Checkbox readiness is condition polling with early success and failure after 20 attempts; it does not accept an incorrect checkbox value. Its elapsed time includes each Terminal/VM round trip, so the comment's 20 seconds is approximate rather than a hard wall-clock deadline.
3. Security and privacy: OK. No new secrets, logging payloads, network destinations, capabilities, clipboard changes, or telemetry.
4. Architecture: OK; test-shell changes only.
5. Rust and TypeScript practice: OK; no Rust/TypeScript or suppressions changed.
6. Licences and reuse: OK; no dependencies or externally adapted code introduced.
7. Tests: finding 1. The saved-notice helper polls actual UI text every 250 ms and returns early. Existing assertions still check requested checkbox state, reopened checkbox state, OS enabled/disabled state, successful auto-start after enabling, and absence of processes/logs after disabling and rebooting. A real failure to change launch-at-login remains a scenario failure. The longer notice allowance does not remove those checks.
8. Git and PR: OK. Two small Conventional Commits and explanatory PR notes. No minor findings to track.

## Validation

- `bash tests/e2e/test-runner.sh`: 99 runner checks and 32 ci-run checks passed; no VM used.
- `bash -n tests/e2e/scenarios/e2e_launch_at_login.sh`: passed.
- Traced both wait paths through `ax.js`, `ax`, `ax_ok`, `vm_terminal`, and the VM command deadline; inspected all `VM_TERMINAL_TIMEOUT` references to confirm no launch-at-login override.
- No VM GUI run performed by this reviewer; the PR's reported three successful runs are author evidence, not independent verification of the 30-to-45-second boundary.
- Full quality gate runs through the required pre-push hook; its execution result is reported separately with delivery status.

No merge performed or authorized in this task.
