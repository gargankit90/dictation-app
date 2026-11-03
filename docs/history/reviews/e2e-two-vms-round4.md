Reviewer: APPROVED

# Two test VMs review, round 4

PR: #146, `e2e-two-vms`, reviewed at `5282be46382416acce1a6ace23baa4fe3d8ab928`.

Scope: only the R4 fix since `5269348`: preserving an unconfirmed start reservation after failed cancellation or interruption, recovery checks, and regression tests. Earlier approved work was not re-reviewed. No real VM was queried or used, and no merge was performed.

## Findings

R4 is resolved. No open blocker or major findings in this delta; no new minor findings.

`E2E_START_PENDING` is set before detached start and cleared only after visibility confirmation or successful cancellation. EXIT cleanup preserves `vm-start` while that flag is set, and failed cancellation forgets ownership without removing the reservation. The reservation records the selected VM. Explicit stale recovery refuses a live owner, failed or unreadable listing, a running reserved macOS VM, or two running macOS VMs. Manual recovery still requires the documented quiescence and operator resolution of the pending start; a listing alone cannot prove that an invisible pending start has been cancelled.

## R4 probes

Re-ran the round-3 R4 probes against the real shell libraries, with temporary lock directories and stubbed Lume calls. A pending file represents a detached start absent from the listing; successful stop removes it and failed stop leaves it intact. Each scenario starts with one unrelated running macOS VM. The TERM scenario uses a fresh isolated lock state.

| Scenario | First run | Reservation after exit | Second VM start | Pending starts plus unrelated VM |
| --- | --- | --- | --- | --- |
| Visibility timeout, successful stop | Exit 1 | Released | Allowed, then cancelled successfully | 1 |
| Visibility timeout, failed stop | Exit 1 | Preserved | Refused with stale-lock diagnostic, no second `lume run` | 2 |
| TERM immediately after detached start | Exit 143 | Preserved | Refused, no second `lume run` | 2 |

These reproduce the two previously failing R4 paths without exceeding the two-VM limit. The successful-stop control confirms the reservation remains releasable after confirmed cancellation. The focused regression suite additionally covers recovery refusal for the reserved VM, two other running VMs, failed listing, safe recovery after the reserved VM is stopped, and TERM after confirmed visibility.

Probe source and raw output are outside the repository at `/tmp/e2e-two-vms-round4-probes.py` and `/tmp/e2e-two-vms-round4-probes.log`; the durable results are recorded above.

## Validation

- `bash tests/e2e/test-vm-lock.sh`: **271 passed, 0 failed**.
- Full gate is delegated to the normal push hook; hooks are not bypassed.
- Real backend cancellation and VM acceptance checks were not repeated, per task scope. This verdict does not substitute for the Tester's verdict or the Lead's manual checks.

## Checklist for this delta

| Section | Result |
| --- | --- |
| 1. Scope | OK: R4 reservation handling, recovery, documentation, and regressions only. |
| 2. Correctness | OK: both prior failure paths preserve the reservation and block another start; successful cancellation releases it. Manual recovery retains documented quiescence limits. |
| 3. Security and privacy | OK: no new secrets, transcripts, network destinations, or capabilities; existing recovery allowlist remains. |
| 4. Architecture | OK: shell test infrastructure only; no crate changes. |
| 5. Rust and TypeScript practice | Not applicable to this shell-only delta. |
| 6. Licences and reuse | OK: no new dependencies or external code reuse. |
| 7. Tests | Independent R4 probes pass; focused suite result recorded above. |
| 8. Git and PR | Only this review report is changed by the reviewer; no product, test, specification, or guard edits; no merge. |
