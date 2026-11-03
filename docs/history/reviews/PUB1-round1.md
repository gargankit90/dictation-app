Reviewer: APPROVED

PR: #182 at 2227b56ef6db426ebfed1da6587381b440e31bff. Round 1, independent Codex review. No blocker or major findings; one minor.

## Findings

| # | Severity | File:line | Finding and suggested fix |
| --- | --- | --- | --- |
| 1 | minor | scripts/e2e-main.sh:63; scripts/test-e2e-main.sh:70 | ShellCheck 0.11.0 exits 1 with SC2329 for the indirectly invoked cleanup callback and SC2012 for listing controlled fixture filenames with ls. Runtime tests pass; these diagnostics do not show broken cleanup. Add an explained suppression for the trap callback and replace or explicitly justify the fixture listing. Follow-up: https://github.com/gargankit90/dictation-app/issues/183. |

## Checks and evidence

- Read the whole diff, PUB1 spec, reviewer role, engineering guide, review checklist, prior public audit and public-switch plan review. Only this review file edited in the checkout; no merge, VM use, keychain access or settings changes.
- Scope: criteria 1–6 map to the two hosted workflows, removed E2E workflow, local wrapper, documentation and gate checks. Criterion 7 remains a Lead check after publication. The milestone spec was added separately in commit c0f101e; no protected guard files changed.
- Correctness: `scripts/check-workflows.sh` passes both repository workflows; `scripts/test-e2e-main.sh` passes all 26 checks (rejection, selected commit, isolated worktree, cleanup, evidence retention and workflow negatives). `actionlint` passes. ShellCheck findings above. Reviewed error paths and inherited ci-run/run VM-lock handling; no product state-machine changes.
- Owner-Mac boundary: all three configured jobs are hosted; E2E Actions entry point is deleted. After repository runner registration is removed, these workflows cannot schedule the owner's Mac or access its keychain. Old branch workflow definitions and editable PR workflows mean runner removal, service shutdown and draining old jobs remain essential owner operations; this shell grep is a regression check, not that security boundary.
- Workflow hardening: contents-read top-level permissions, no job overrides, checkout credential persistence disabled, no privileged events or secrets reads, fork PR exclusion removed. Required check names are exactly `Quality gate (Linux)` and `Platform crate (macOS)`. DMG remains path-filtered and should not be a globally required check.
- Verified all five action pins against upstream GitHub tag/ref APIs, including dereferencing the annotated rust-cache v2.9.3 tag. They match the documented versions. Current actions/runner-images README maps macos-15 to arm64; CI additionally checks uname. Sources: https://github.com/actions/runner-images and upstream action repositories.
- Hosted readiness: checked Linux Tauri library installation, pinned Rust components/tools, Node 24 setup, cargo-about requirement for both gate and DMG, npm installation, cache placement, timeouts, ad-hoc DMG selection and clean-worktree builds. Cargo tree for x86_64-unknown-linux-gnu confirms the Mac-only audio dependency does not pull ALSA into the Linux workspace. No concrete first-hosted-run code failure identified by static review; this is not evidence that clean hosted builds pass.
- Current hosted runs have **not executed**: CI run 41545566565 reports empty runner names and no steps; its check annotation says account payments failed or the spending limit needs increasing. DMG also failed before execution. Resolve capacity/billing or complete the planned public switch, then explicitly dispatch new CI/DMG runs and verify both jobs and the artifact. Do not restore the owner runner as a workaround. Full gate was not independently rerun as a review check; the mandatory push hook runs it for the review commit.
- Local E2E wrapper: fetch and ancestry check precede npm/build execution; defaults to origin/main and accepts older main ancestors as specified. Uses a temporary detached worktree and delegates to ci-run.sh, whose run.sh path takes the existing VM locks. No upload command added; passing evidence is reduced to summaries, failed/skipped evidence stays local, worktree cleanup is trapped. Environment overrides are local test/operator controls, not remote workflow inputs. No VM or real build was invoked during review.
- Workflow checker rejects explicit self-hosted/owner-mac mentions, scalar macOS labels, runner arrays/expressions, unpinned actions, missing credential control, wider top-level permissions, privileged events and secrets-dot access. Existing tests cover representative negative cases. It is deliberately a constrained textual checker, not a general YAML security validator, and cannot secure a registered runner against a PR that changes the checker itself.
- Security/privacy, architecture, Rust/TypeScript practice, licences/reuse: no product, crate-dependency, capability, CSP, transcript/key handling or reused-code changes. No new product concern found. Tests: focused no-VM script tests pass; hosted and real-VM validation remain as qualified above. Git/PR: small conventional implementation commits and detailed notes; no merge performed.

## Nonblocking follow-up already identified by Developer

`scripts/ci-check-tools.sh` is now unused and describes the removed runner setup. Removing or updating it is housekeeping; it is not a reachable workflow or a new route to the owner's Mac.
