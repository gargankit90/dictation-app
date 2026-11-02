Reviewer: CHANGES REQUESTED

# T5 review, round 2

PR: #163 at `76ab5a6`. Scope: only the R1 fix since `7d2598b`; unchanged implementation was not re-reviewed. No merge performed.

## Findings

R1 is fixed: independently removing only the deadline cancellation `pkill` in an isolated copy now produces `FAIL: and the job is killed`, exits 1, and reports **72 passed, 1 failed**. The unmodified runner passes **73/73**. The 600-second lifetime is well beyond the 30-second exit wait and preserves #149's scheduling separation, deadline status, diagnostic, elapsed bound, and delayed-completion coverage.

| ID | Severity | Location | Finding | Suggested fix |
| --- | --- | --- | --- | --- |
| R2 | Major | `tests/e2e/test-runner.sh:98` (cleanup at lines 16–18) | The new 600-second fixture is not always cleaned up. With the supported `KEEP=1` mode, the unmodified runner exits 0 with 73/73 checks but leaves `$tmp/longsleep 600` alive: KEEP replaces the EXIT cleanup with an echo-only trap. Keeping diagnostic files must not keep this new ten-minute process running. | Always terminate the fixture processes during cleanup; make KEEP conditional only around directory removal. Ensure the same cleanup is reached on handled interruption, and verify success, failed assertion, and interrupted runs without leaving the fixture alive. |

## Validation and reproduction

- Ran the unmodified checkout with `bash tests/e2e/test-runner.sh`: 73 passed, 0 failed.
- Copied `tests/` to an isolated temporary root and changed only the deadline `pkill` to `:`. The intended cancellation assertion fails: 72 passed, 1 failed.
- In a separate run of the unmodified isolated source, launched `KEEP=1 bash tests/e2e/test-runner.sh`, observed its unique `longsleep` PID, waited for runner exit, and checked that temporary directory's process names. Exit was 0 and all 73 checks passed, but PID 28333 still ran `/var/folders/32/3wybk1rs3wg0v_5pnjtntv5h0000gn/T/tmp.dnKTvQox0N/longsleep 600`. Explicitly terminated the remaining fixture processes after recording evidence.
- A TERM delivered while the unmodified deadline case was running did not leave the long sleep after the working deadline cancellation completed. A direct INT delivered only to the parent was ignored and the suite completed; this is not evidence of successful interruption cleanup.
- No product, test, specification, or guard files were changed in the review checkout. Mutations and review harnesses remained outside it.

## Checklist (changed scope only)

| Section | Assessment |
| --- | --- |
| 1. Scope | R1 fix is focused; R2 prevents the requested fixture lifecycle guarantee |
| 2. Correctness | R2: new long-lived fixture survives supported diagnostic-mode success |
| 3. Security and privacy | OK: no new secrets, data collection, or network access |
| 4. Architecture | OK: shell test fixture only |
| 5. Rust and TypeScript practice | Not affected |
| 6. Licences and reuse | OK: no dependencies or copied external code |
| 7. Tests | R1 sensitivity restored; R2 cleanup gap reproduced independently |
| 8. Git and PR | Focused test commit; no guard or specification changes; no minor findings |
