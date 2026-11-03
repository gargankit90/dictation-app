# D3 run 1: Tester report

PR: #201 (branch `d3-history`, head `f9ff34d`). Spec: `docs/history/milestones/D3.md`.
Verdict: **PASS**. No blockers, majors or minors.

| # | Criterion | Result | Evidence |
| - | --------- | ------ | -------- |
| 1 | Rename-only move commit, history kept | PASS | `git show --stat -M 13baa5f`: 266 files changed, 0 insertions, 0 deletions; `--name-status`: 266 x `R100`, nothing added or deleted. `git log --follow` on `docs/history/test-reports/D1-run1.md` and `docs/history/reviews/D1-round1.md` lists the move commit and then the original commit (`9f2a266`, `9e6a720`). |
| 2 | `docs/history/README.md` | PASS | Explains the three folders, how id/run/round/PR connect, a 6-row "read first" table, one line on the team linking `AGENTS.md`. All 16 links resolve, including every read-first pick (M5.9, T4a, G1, T7, R1, D3). |
| 3 | Old-path references updated, link check proves it | PASS | `git grep -nP '(?<!history/)(test-reports/\|reviews/\|docs/milestones)' -- . ':!docs/history'` finds nothing. A looser grep for bare `milestones`/`test-reports`/`reviews` finds only unrelated words ("reviews the code"). No old root folder is tracked. No active-instruction file points at an old path. `scripts/check-doc-links.sh`: 41 relative links ok. I changed the `docs/TESTERS.md` link in `README.md` to `TESTERSX.md`: the script printed `broken link: README.md -> docs/TESTERSX.md` and exited 1. Restored with `git checkout`: ok again, exit 0. |
| 4 | New work goes to the new place | PASS | `docs/agents/tester.md` (may-edit list and step 7), `docs/agents/reviewer.md`, `.claude/agents/developer.md`, `AGENTS.md` (Lead step 1, Tester, Reviewer), `docs/review-checklist.md` all name `docs/history/{test-reports,reviews,milestones}/`. |
| 5 | Gate passes, nothing else changes | PASS (CI) | Hosted CI on #201: Quality gate (Linux) pass, Platform crate (macOS) pass, DMG (macOS) pass. I did not run the full gate locally (the push hook runs it). |

Other checks from the task:

- README layout table has a `docs/history/` row and `CONTRIBUTING.md` points at `docs/history/milestones/`; both match the new tree (49 specs, 95 test reports, 91 reviews under `docs/history/`).
- Test merge of `origin/main` into `origin/d3-history` on a temporary local branch: "Already up to date", no conflicts (branch is already based on current `main`). Branch deleted, nothing pushed.

Limits: `check-doc-links.sh` checks tracked `.md` files only and skips links in code spans and fences, so path mentions in backticks (such as those in the role files) were checked by grep, not by the script. Historical reports still mention their own old paths, which is expected.
