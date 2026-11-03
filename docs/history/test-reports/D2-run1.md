# D2 test report, run 1

PR: #198 at 69d8f80 (the branch head only adds the reviewer's round 1 file)
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | AGENTS.md: roles table, Model column, no Antigravity, post-merge e2e step, enforcement section, no outside-repo refs, VMs, top note, no gate duplication | Read the diff and the file; `gh api` rulesets 26422642, 26422652, 26425594 | PASS | Table names Codex (gpt-6.1-sol) as reviewer and merger; step 7 = Lead runs `scripts/e2e-main.sh <sha>` and posts on the merged PR (script usage matches: `[<sha>]`). Rulesets match live: `main-public` (PR, 0 approvals, checks `Quality gate (Linux)` + `Platform crate (macOS)`, strict up-to-date, no force/delete), `immutable-tags`, `all-branches-no-force-no-delete`; all bypass lists empty. "Update branch" and hosted-runner lines present. "the plan's" reference gone. Top note points to `CONTRIBUTING.md` (exists). Gate list replaced by `scripts/gate.sh`. |
| 2 | CLAUDE.md matches | Read the diff | PASS | Codex merges; `cua-vm` = dictation-vm, `cua-vm-2` = dictation-vm-2. |
| 3 | reviewer.md, tester.md, developer.md | Read the diff | PASS | reviewer.md says Codex; tester step 6 uses `E2E_VM=dictation-vm-2 tests/e2e/run.sh` and `cua-vm-2` only, says never `dictation-vm`; lume/scp commands gone; developer.md step 4 uses `scripts/gate.sh`, M0.3 text gone. |
| 4 | review-checklist.md | Read the diff | PASS | Guard paragraph matches G1 (rulesets + pre-push; guard-git.sh = merging, hook bypass, guard edits, destructive settings; bypass major, else minor). `~/Documents` path replaced. |
| 5 | ENGINEERING.md | Read the diff | PASS | Hosted-CI-needs-public line removed; Windows adapter "planned". |
| 6 | Dead code | `git grep -n ci-check-tools`; `bash -n`; `scripts/check-workflows.sh`; `scripts/test-guards.sh` | PASS | File deleted; only hits are docs/milestones/D2.md, reviews/, test-reports/ (history). check-workflows ok (2 files), test-guards 496 passed 0 failed. `ci-run.sh` and `run.sh` headers point to `e2e-main.sh`; both pass `bash -n`. |
| 7 | Links and antigravity | `git grep -n -i antigravity` outside test-reports/ reviews/ docs/milestones/; script checking every markdown link and backticked path in the 9 edited files | PASS | grep empty. No broken markdown links. The only unresolved backticked names are bare file names in ENGINEERING.md (`ci.yml`, `dmg.yml`, `e2e.yml`, `latest.json`, `run.sh`, `tauri.conf.json`), unchanged by this PR and given without their directory. |

## Extra checks

- **Hard rules before/after** (`git show origin/main:AGENTS.md` vs branch): never push to main/force/delete/move tags; never skip hooks; never edit guards; only reviewer merges; guards-change rule; stay in "may edit"; computer use only in VMs (widened to name both VMs, same meaning); secrets; privacy; licences; ask Lead before outward-facing. All 11 present with the same meaning. Workflow steps 1-6 unchanged; step 7 added.
- **Consistency** across AGENTS.md, CLAUDE.md, role files, checklist: VM names (dictation-vm/cua-vm Developers, dictation-vm-2/cua-vm-2 Testers), Codex merges, gate = `scripts/gate.sh`, severity rules agree. No contradictions found.
- **Gate script references**: nothing in gate.sh, test-guards.sh, lefthook.yml or .github references the deleted script. Prettier and typos clean on the edited docs. I did not run the full gate.
- **CI** (`gh pr checks 198`, run 41591080602): Quality gate (Linux) pass 7m31s; Platform crate (macOS) pass 1m50s.

## Findings (all minor, none block)

1. AGENTS.md reviewer paragraph adds new policy text ("the Reviewer is always the other model family from the PR's author ... a Claude worker (Opus 5.5) reviews Codex-written PRs"; "the Lead (or the owner) approves its writes, pushes and merges"). Criterion 1 says no rule's meaning changes. This is plausibly just the intent of "different model family" made explicit, but the Lead should confirm it. reviewer.md still says only "different model family from the Developer", which is consistent but less specific.
2. `main-public` also sets `required_review_thread_resolution: true` and allows merge, squash and rebase; AGENTS.md lists neither. Agents hitting an unresolved thread would not find it documented.
3. docs/ENGINEERING.md still has "(from M0.3)" at lines 107 and 182, the same kind of leftover D2 removed elsewhere (outside the listed criteria).
4. Cosmetic: some edited lines are now longer than the surrounding wrap (AGENTS.md pre-push bullet, tester.md and reviewer.md lines). Prettier accepts them.

## New-agent read-through (AGENTS.md + role file)

Clear: roles, models, workflow including the post-merge step, which VM to use, the gate command, and the up-to-date-branch rule. Mildly confusing: the Commands list no longer shows plain `cargo build`/`cargo nextest` (the gate covers them; fine). A new Tester is told "CI and lefthook already run the gate" but hosted checks take about 8 minutes, so the verdict should wait for them (this run did).
