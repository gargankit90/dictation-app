# T6 test report, run 1

PR: #167 at e0720a9
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Push to the default branch only | Read `.github/workflows/e2e.yml`; `git diff origin/main...HEAD -- .github` | PASS | `on:` is `push: branches: [main]` plus `workflow_dispatch` (manual only); no `pull_request`. `e2e.yml` is the only workflow file touched; `ci.yml` and `dmg.yml` are unchanged. |
| 2 | Pool order, wait, skip; never golden | `test-ci-run.sh` (32 passed); 9 mutants; real `run.sh` with both locks held | PASS | See below. |
| 3 | Excludes real-key/real-mic; passes a real run | Narrowed real run through `ci-run.sh` | PASS | See below. |
| 4 | Failure evidence | `test-ci-run.sh` failure case; evidence folder of the real run | PASS | The failing case keeps the summary, SHA, VM, logs and hides key shapes. |
| 5 | Docs and `run.sh` header | Read the diffs of README, ENGINEERING.md, `run.sh` | PASS | The "Not part of CI" wording is gone; the pool, timeout, skip, evidence and stale-lock steps are documented. |

## Criterion 2: mutants against `test-ci-run.sh`

Each mutant was applied to `tests/e2e/ci-run.sh` locally and reverted (not committed).

| Mutant | Result |
| ------ | ------ |
| Pool order swapped (`dictation-vm` first) | killed (11 failed checks) |
| Timeout ignored (waits forever) | killed (the test hung and hit my 90 s alarm; a hang is a failure, but the test has no timeout of its own) |
| Timeout ends with exit 1 instead of "skipped" | killed |
| Scenario failure reported as `PASSED` | killed |
| Scenario failure exits 0 | killed |
| Golden image added to the pool | killed |
| Golden image allowed in `CI_VM_POOL` | killed |
| Excludes removed | killed |
| Key hiding removed | killed |

## Criterion 3: lock behaviour with the real `run.sh`

- Fake locks (live `sleep` pids) for both VMs in a temporary `E2E_LOCK_DIR`, real `run.sh`, `CI_VM_WAIT=8`, `CI_VM_POLL=3`: tried dictation-vm-2 then dictation-vm, found both "isn't free", waited, retried, and ended after 9 s with exit 0 and `skipped: no VM free (waited 8s)`.
- The fall-back to dictation-vm when only dictation-vm-2 is locked was proved with the fake `run.sh` (case "vm-2 held"). I did not run it for real, because that would take the VM the M5.4 Developer uses.
- Nothing touched the real lock directory (empty before and after), and I did not take or clear the M5.4 Developer's lock.

## Criterion 3 and 4: narrowed real run

`E2E_RUN` pointed at a one-line wrapper that appends three scenarios to `run.sh`'s arguments (`ci-run.sh` has no scenario argument). Everything else was the real path: real `npm run tauri build`, real `run.sh --no-build --exclude e2e_real_groq --exclude e2e_real_mic`, the real lock and VM reset on dictation-vm-2. Exit 0, about 2 minutes.

```
PASS     e2e_startup
PASS     e2e_press_release
PASS     e2e_quit_during_transcription_then_relaunch
3 passed, 0 failed, 0 skipped
```

Evidence folder: `about.txt` (commit e0720a972dc5…, vm: dictation-vm-2, status: PASSED), `results.txt`, `build.log`, `run-dictation-vm-2.log`, three app logs and three screenshots. No `gsk_`/`sk-` shapes anywhere. The workflow uploads this folder as `e2e-evidence-<sha>`.

## Notes

- `workflow_dispatch` is an extra, manual-only trigger. It is documented in the workflow and the README and doesn't violate "no PR run". Minor.
- The workflow's upload step was not run here (needs GitHub); only the folder it uploads was checked. The first real run after merge is the proof.
- Did not repeat the full suite (the Developer's 32/32 stands) or the gate.
