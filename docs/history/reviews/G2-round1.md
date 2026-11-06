Reviewer: CHANGES REQUESTED

# G2 review, round 1

PR: #291 at 8bedb98acdc5c83e14d29d4296da343c2e1ec9cd

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | scripts/gate-lib.sh:48 | Receipt env omits `COVERAGE_*` and `LICENCES_*`, which change actual gate checks. A full pass with lowered `COVERAGE_MIN_CORE` can satisfy a subsequent default-threshold gate, and changing `LICENCES_GEN` to a nonexistent generator still reuses the receipt. Independently confirmed identical keys before and after setting `COVERAGE_MIN_CORE=101` and `LICENCES_GEN=/nonexistent` (tool-version probing stubbed identically). | Include all supported gate-control variables, including coverage thresholds/root/crates and licence overrides, or disallow reuse/write with overrides. Test transitions from weakened checks back to defaults. |
| 2 | major | scripts/gate-lib.sh:53 | The hidden npm lockfile describes an installation but does not prove it exists. Deleting an installed tool under `node_modules/.bin` leaves the install key identical; receipt reuse happens before `gate_check_node` and executable checks. A previously successful gate can therefore skip when an actual run would fail for missing tools. A throwaway fixture confirmed identical install hashes after deleting its prettier executable. | Validate installed dependencies before reuse; at minimum reject missing required executables/packages, and detect package-state changes rather than trusting only the hidden lockfile. Add a deletion regression. |
| 3 | major | scripts/test-gate.sh:13 | `GIT_TEMPLATE_DIR` is deliberately retained. `git init` copies executable hooks from that template, and fixture commits/pushes execute them. The top-level check does not prevent a copied hook from acting on the real checkout. This leaves an inherited-environment escape despite fixing `GIT_DIR`. | Use an explicit empty template for fixture init/clone and disable fixture hooks through isolated configuration; do not retain the caller's template. Add an inherited template with a sentinel hook and assert it never runs. This concerns throwaway fixture hooks, not bypassing the project's real hooks. |

## Safety and selection

The full self-test passed when invoked from this linked worktree with the real worktree's `GIT_DIR` and `GIT_WORK_TREE` exported; HEAD and tracked files remained unchanged. Unsetting the repository-selection variables fixes the reported incident for that environment. Finding 3 concerns the remaining executable-template input, not a repeat of that incident. No VM or real-repository mutation probe was used.

Receipt storage is per worktree, writes use same-directory rename after full success, and end-of-run key comparison rejects changed inputs. Dirty/staged/untracked nonignored files prevent reuse/write. CI bypasses receipt reading/writing and prose mode, and `--full` cannot be satisfied by a receipt. TTL rejects future/expired timestamps. Both gate scripts are included along with the tracked tree. Listed tools and prefixed flags are included; findings 1 and 2 are gaps in the claimed identical-input guarantee.

Prose selection is narrow and fail-closed: NUL-delimited paths, no rename detection (both paths count), deletions included, empty diff full, detached/shallow/missing-base/ambiguous-base full. The criss-cross test currently permits a fixture skip, but production code rejects multiple merge bases. Guards are unchanged; the docs retain gate-before-push and otherwise reflect selection behavior, subject to the receipt gaps above.

## macOS meter exclusion

The evidence supports a test scheduling limitation, not a demonstrated macOS level-meter defect. The test uses `MeteredAudio`, `ScriptedAudio`, and the shared `LevelMeter`; it never captures a real microphone. Its feeder is an ordinary thread sleeping 10 ms, while the pipeline consumes and resets the sample window every 50 ms. A delayed feeder can leave an empty window, correctly reported as -100 dBFS by `LevelMeter::take`; the assertion requires every reading near -20 dBFS and has no synchronization proving samples arrived in each window. Platform permission/device availability cannot explain this synthetic test. Scheduler starvation is an inference from code and failure, not measured instrumentation; this does not prove the real pill is bug-free under load.

Temporarily excluding this single named test on macOS is acceptable with #292 tracking deterministic replacement and removal of the filter. The other level tests, platform metering tests, Linux execution, and local full gate remain. The nextest substring filter currently matches only this test; use an exact fully qualified filter when replacing it if names expand. Workspace clippy/nextest/doctests are added, `DICTATION_NO_LAUNCH=1` is set, existing standalone platform/vendor checks remain, and required job names are unchanged. Mac coverage and Mac shell self-tests remain local as documented.

## CI cost

Compared run 42125197113 (main) with 42128921753 (PR). macOS grew from 2m12s to 8m18s; added workspace clippy took 1m46s, nextest including compilation 4m14s, doctests 4s. Linux grew from 9m40s to 13m06s, but its gate step decreased from 6m18s to 6m06s. Disk cleanup alone grew from 1m25s to 4m46s (3m21s); cache restore added 20s, while npm install saved 8s. The added self-test does have a recurring cost, but these runs do not attribute the observed Linux growth to it. Changing disk cleanup or relocating self-tests can be separate work; neither is necessary to approve G2 correctness once the findings are fixed.

## Checklist

1. Scope: findings 1–2 affect criterion 1; other criteria mapped to implementation/evidence.
2. Correctness: findings 1–3.
3. Security/privacy: finding 3; no new product data handling or secrets.
4. Architecture: OK; no product crate changes.
5. Rust/TypeScript practice: OK; no product source changes.
6. Licences/reuse: OK; no dependencies or copied third-party code added.
7. Tests: self-test passed under inherited worktree Git environment; targeted key probes reproduced findings 1–2; template-hook probe and full gate results recorded in the PR comment.
8. Git/PR: conventional commits and complete timing notes; no guard changes. AGENTS documentation edits are specifically required by G2 criterion 5.
