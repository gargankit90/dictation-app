Reviewer: APPROVED

PR: #198, round 1, reviewed head `69d8f80e61716279023de818e83a962d34416759`.

## Findings by severity

No blockers or majors. One minor:

| # | Severity | file:line | Finding | Suggested fix |
| --- | --- | --- | --- | --- |
| 1 | minor | AGENTS.md:17-19 | The new exception assigns Claude to review Codex-authored PRs, but CLAUDE.md:12 blocks Claude merges, the workflow assigns merging to the Reviewer, and reviewer.md names Codex. The exceptional handoff is unspecified. Ordinary Claude-authored PRs remain clear, and the hook prevents an unauthorized Claude merge. | Explain the alternate review/merge handoff and role-file selection while retaining independence and Claude's merge block. Tracked in [#199](https://github.com/gargankit90/dictation-app/issues/199). |

## Source-review coverage: areas D–E

Read both `.public-repo-review-codex.md` and `.public-repo-review-claude.md` supplied in the dispatch. Rows below identify findings by their original locations rather than importing their public-launch priorities as code-review severities.

| Source / finding | Status | Evidence or disposition |
| --- | --- | --- |
| Both E: Tester resets Developer VM | Fixed | tester.md step 6 uses `E2E_VM=dictation-vm-2 tests/e2e/run.sh`, links the runner runbook, and permits only `cua-vm-2` for GUI checks. Raw delete/clone/scp steps removed. |
| Both E: Antigravity identity / manual-permission assumption / model defaults | Fixed, with minor clarification | AGENTS, CLAUDE and reviewer role consistently name Codex for the normal workflow; model defaults are centralised in AGENTS. See finding 1 for the new exception. |
| Codex E: Developer allowlist/task-specific grants and checklist scope | Not fixed here; D3 follow-up | developer.md still has its restrictive baseline, and checklist section 8 still prohibits Developer AGENTS/spec edits. Outside D2's numbered criteria; do not claim this source finding is closed. No D3 spec is present in this checkout, so ownership is the dispatch's stated follow-up, not independently verified implementation. |
| Codex E: network policy versus release updater | Not fixed here; D3 follow-up | checklist:37 and ENGINEERING's security section still exclude updater endpoints. Existing contradiction, outside D2's numbered criteria; retain for D3 rather than treating D2 as a complete resolution of the public reviews. |
| Both E: G1 enforcement/checklist model | Fixed | Checklist describes server rulesets plus pre-push for push rules, and role/hook/guard/settings checks separately. Compared against the live hook. |
| Claude E: any-branch ruleset, required checks, strict up-to-date | Fixed | AGENTS lists all three rulesets, both required check contexts and update-before-merge instruction. Reviewer role requires reading AGENTS, so duplicating it in the merge section is unnecessary. |
| Claude E: post-merge e2e and worker models | Fixed | Workflow step 7 assigns local `scripts/e2e-main.sh <merge sha>` and PR result to Lead; role table supplies models. |
| Claude E: singular VM/server wording | Fixed | AGENTS and CLAUDE name both VM/server pairs. |
| Claude E: machine-specific reference path | Fixed | Checklist points to reference repos beside the repo, as AGENTS describes. |
| Claude E: external plan reference | Fixed | AGENTS points to ENGINEERING and NOTICE. |
| Both E: duplicate gate lists / M0.3 / stale engineering text | Partly fixed | AGENTS removes its detailed duplicated list; developer drops the obsolete conditional; ENGINEERING drops private/billing wording. Its shorter gate summary and `from M0.3` history remain; executable authority is now explicitly `scripts/gate.sh`. No new conflicting command introduced. |
| Claude E: explain Orca / human audience | Fixed for D2 | New top note identifies maintainer AI workers and points humans at CONTRIBUTING. A public Orca explainer link was optional in the source recommendation. |
| Claude E: inherited Claude hooks for outside contributors | Not a D2 edit; D1 owns CONTRIBUTING | Keep separate from maintainer role refresh. |
| Claude D: dead ci-check-tools.sh | Fixed | Deleted; no live caller. Remaining new mention is the D2 spec recording its deletion, not an executable dependency. |
| Claude D: ci-run.sh header; both known stale runner mentions | Fixed | Both runner headers now describe local Lead invocation through e2e-main.sh. #184's separate job-level permissions item remains open. |
| Both D: history/index/relocation | Not fixed here; deferred | No history moved or deleted, preserving evidence and existing links. D3/navigation follow-up rather than a D2 acceptance criterion. |
| Claude D: optional root about/config relocation | Not fixed; acceptable | Optional cosmetic restructuring, outside this focused PR. Tool-owned root configs remain valid. |
| Claude E/D: hook's stale “Tester merges” message | Fixed upstream / existing issue | Live command hook says Code Reviewer merges (G1). The separate push-guard wording remains tracked by #189; no guard edits in D2. |

## Hard-rule and runtime checks

Compared the complete AGENTS diff with its base. No weakening of default-branch/force-push/tag/deletion restrictions, hook requirements, owner-only guards, Reviewer-only conditional merge, role scopes, secrets, privacy, licences, or VM-only computer use. The VM rule is made more specific without permitting host-screen use. The report does not authorise a merge.

The Codex approval wording is accurate as an authorization policy: this dispatch itself authorises the report write, commit, branch push and PR comment. It does not require a separate approval prompt for each command and does not claim that Codex has Antigravity's Manual permission UI. This session has approval policy `never`; the text should be read as Lead/owner task authorization, not a technical per-command gate. A merge still requires its own authorised task and all three workflow conditions.

Read-only GitHub API checks: all three rulesets are active, with no bypass actors and current-user bypass `never`. `main-public` targets main, requires a PR with 0 approvals, strict required checks `Quality gate (Linux)` and `Platform crate (macOS)`, and blocks deletion/non-fast-forward updates. `immutable-tags` applies to all tags and blocks updates/deletion/non-fast-forward; `all-branches-no-force-no-delete` applies to all branches and blocks deletion/non-fast-forward. Repository default branch is main and `allow_update_branch` is true. The live ruleset also requires conversation resolution; AGENTS does not purport to enumerate every server setting, and nothing instructs bypassing it.

Read `.claude/hooks/guard-git.sh`, the pre-push entry point and role instructions. Their described responsibilities match G1. CLAUDE's shorter two-ruleset summary is not an exhaustive claim and imports AGENTS for the complete list.

## Checklist and validation

1. Scope: OK. All seven D2 criteria map to the diff; spec is Lead-owned task material, and the explicit dispatch/spec authorises this maintenance scope despite the old baseline Developer allowlist.
2. Correctness: OK for documentation/dead-code scope; no executable behavior changed beyond deleting an unused script.
3. Security/privacy: OK; preserved hard rules, no secrets, transcripts or new destinations introduced.
4. Architecture: OK; no dependency, OS or unsafe-code changes.
5. Rust/TypeScript practice: not applicable to this documentation-only diff.
6. Licences/reuse: OK; no code copied or dependencies added.
7. Tests: documentation/static checks appropriate; no new behavior tests needed. Tester independently checks acceptance criteria; pre-push runs the required gate.
8. Git/PR: OK; complete PR notes, conventional implementation commit, no guard/product/test-body edits; minor tracked in #199.

Read every changed file and relevant role/runbook/engineering context. Referenced concrete repo paths resolve; `<id>` and scenario placeholders are templates, not missing files. Scoped Antigravity search is empty. PR's claim that ci-check-tools search is wholly empty overlooks the new spec mention; there is no live caller and the deletion criterion is satisfied.

Clarity/size: implementation diff is 111 insertions / 119 deletions including the 40-line new spec; existing operational files shrink overall. AGENTS shrinks from 175 to 172 lines while adding models, current enforcement and the missing post-merge step. This is a modest net simplification, not the larger agent-contract redesign suggested for later work.
