Reviewer: APPROVED

# T6 review, round 1

PR: [#167](https://github.com/gargankit90/dictation-app/pull/167), reviewed at `e0720a972dc546a55360f195e0d5ca33c4fd2734`.

No blocker or major findings. Two non-blocking minors, tracked below. No merge performed.

## Findings by severity

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | minor | `.github/workflows/e2e.yml:24` | The comment promises every push runs in order with none cancelled. The constant concurrency group keeps one pending run by default: while A runs, C replaces pending B. The Lead explicitly confirmed during this review that coalescing is desired to avoid adding roughly 21 minutes of runner occupancy per merge and delaying PR checks; C still tests B's changes. This is therefore a documentation issue under the clarified policy. | Keep the policy; explain pending-run replacement and failure-attribution limits in the workflow comment and README. [Tech debt #169](https://github.com/gargankit90/dictation-app/issues/169). |
| 2 | minor | `.github/workflows/e2e.yml:19` | `workflow_dispatch` permits manual runs from another branch, beyond the spec's push-to-default-only scope. It does not automatically run for PRs. | Remove manual dispatch, or document Lead approval for the additional scope and restrict its ref if the default-branch boundary is intended. [Tech debt #168](https://github.com/gargankit90/dictation-app/issues/168). |

GitHub's [concurrency reference](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#concurrency) confirms the default single pending slot. The Lead's queue policy clarification is recorded in #169. Findings were also posted inline on the PR.

## What I checked

- Read the full PR diff, PR notes, T6 spec, engineering rules, reviewer role and checklist. Confirmed the repository default branch is `main` through the GitHub API.
- Workflow: automatic trigger is push to `main`; no PR, fork, nightly or `pull_request_target` trigger. PR checks list only the quality gate and platform job. Manual dispatch is finding 2. Token is `contents: read`; no secret expression or API key is supplied. Actions are pinned. Evidence upload uses `always()` and 14-day retention.
- Pool and locking: the wrapper permits only the two working VM names, prefers `dictation-vm-2`, and delegates acquisition to `run.sh`. It does not remove a lock or invoke stale-lock recovery. Inspected `lib_vm.sh`'s atomic lock acquisition, refusal/release paths and host-wide start reservation: the existing protections against stealing a live VM or starting a third VM remain in force. Golden images/backups are not wrapper pool candidates. CI waits up to 2700 seconds plus the polling interval for busy VMs, then reports `skipped: no VM free` and exits 0.
- Failure and evidence: a normal scenario failure returns 1 without trying the other VM, preserving SHA, VM, results and collected logs/screenshots. Build failure also returns 1 with `build.log` and a failure summary. Text copied from scenario output directories has key-shaped strings hidden. Build and runner stdout logs are copied directly; CI supplies no real key, excludes real-key/real-mic scenarios and resets the guest, so those paths have no identified real-secret source. Screenshots/UI dumps can show canned fixture text; no real dictation is enabled. No new application logging or transcript collection was introduced.
- Existing `ci.yml` gate/platform jobs and `dmg.yml` are byte-for-byte unchanged. No application code, dependencies, guards, capabilities, CSP or licence changes.
- Dev-certificate signing is **not covered by the DMG workflow**: `.github/workflows/dmg.yml` explicitly sets `DMG_ADHOC: "1"`, while `src-tauri/tauri.conf.json:39` specifies `dictation-app Dev` for this build. The Lead reports that Actions runs as a LaunchAgent in the owner's GUI session with the login keychain available. Signing still has not been exercised under Actions; a missing/unavailable certificate or inaccessible private key would fail the build before VM acquisition, set `FAILED: the build failed`, return 1 and upload `build.log`. This first-push validation remains an acknowledged operational limitation, not a demonstrated defect.

## Validation

- `bash tests/e2e/test-ci-run.sh`: **32 passed, 0 failed**.
- `bash tests/e2e/test-runner.sh`: **94 passed, 0 failed**, with its nested CI wrapper suite also **32 passed, 0 failed**. These use isolated fixtures and fake OS/VM tools.
- Independently copied the wrapper/tests into a temporary Git repository and ran four mutants. The baseline passed all 32 checks; all four mutants failed: reverse VM preference (11 failed checks), make scenario failure exit 0 (1), remove real-key exclusion (1), remove copied-log redaction (1). No repository product or test files were changed for this exercise.
- Reviewed the Developer's reported local wrapper run: 32 passed, 0 failed, 0 skipped, with real Groq/mic excluded. I did not run a VM or use either screen; the parallel Tester owns real-run verification. CI/gate results and the Tester's verdict remain separate merge prerequisites.

## Checklist

| Section | Result |
| ------- | ------ |
| 1. Scope | OK with minor 2 and the Lead's clarified queue policy; all five acceptance criteria map to implementation and documented/test evidence. |
| 2. Correctness | OK; bounded busy wait, unchanged lock safety, real failure propagation; minor 1 corrects the queue documentation. |
| 3. Security and privacy | OK within the fixture-only CI scope checked above; no real keys supplied or real provider/mic scenario selected. |
| 4. Architecture | OK; shell/workflow/docs change, crate boundaries unchanged. |
| 5. Rust and TypeScript practice | Not affected; no Rust/TS changes or new suppressions. |
| 6. Licences and reuse | OK; no new dependencies or copied application code. |
| 7. Tests | OK; wrapper and runner tests pass, four independent mutants rejected; real VM run delegated to Tester. |
| 8. Git and PR | OK; detailed PR notes, conventional implementation commit, separate preceding spec commit, no guard changes; both minors have tech-debt issues. |
