Reviewer: APPROVED

PR: #180 at 16b4363d951d6c25569e62c68ac005f28264d141. Round 1.

## Findings

No blockers or majors. One minor:

| Severity | file:line | Finding | Suggested fix |
| --- | --- | --- | --- |
| minor | README.md:40; CONTRIBUTING.md:26 | Hosted CI is described as current, but this head still selects self-hosted runners (`ci.yml:33,95`); pushes trigger only on main, rather than every push. PR notes correctly acknowledge the pending PUB1 migration. | Land PUB1 before publication; qualify the migration if PUB2 merges first, and say pushes to main and PRs unless triggers expand. Tracked in [#181](https://github.com/gargankit90/dictation-app/issues/181). |

The wording mismatch does not block merging this docs/metadata PR into the still-private repository. It must be resolved by PUB1 or a documentation correction before public launch; this approval does not clear audit B1.

## What I checked

- Scope: read the entire `gh pr diff 180`, PR notes, PUB2 criteria 1–8, engineering rules, reviewer role/checklist and audit B3/items 9–10. Changes match the authorized licensing/public-documentation scope. AGENTS and milestone edits are explicitly required by this task/spec, rather than an unauthorized workflow change.
- Correctness: standard root MIT grant and specified owner/year; six original Cargo packages resolve to MIT with publishing disabled; npm remains private with MIT metadata. No runtime logic changed.
- Security/privacy: SECURITY defines relevant scope, a private GitHub reporting route and no response-time SLA. Its Keychain and no-plaintext-storage policy matches the project policy. No credential/private-machine disclosure introduced in the diff. This is a diff review, not a renewed whole-history disclosure audit.
- Publication prerequisite: repository API still reports private; private-vulnerability-reporting API returned 404, so the reporting button is not verified operational. GitHub documents this feature for public repositories: [configuration guidance](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository). The PR explicitly assigns enabling it to the owner at publication. Enable and verify it before accepting public reports; do not interpret this review as evidence that it is already enabled. No unsupported bounty, turnaround or platform-support promise was added.
- Architecture: no crate dependency edges, platform cfg, unsafe, IPC or capability changes.
- Rust/TypeScript practice: metadata only; no new suppressions or code paths.
- Licences/reuse: original-code grant is distinguished from upstream code in NOTICE/README. Existing credits remain; Handy-adapted code now has CJ Pais copyright plus full MIT permission/warranty text both in NOTICE and the shipped generated notice. Vendored crates, their licence files and metadata are unchanged. MPL exceptions, full notices and versioned crates.io source links remain; MIT does not relabel these dependencies (consistent with [Mozilla's FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/)). deny/about settings are unchanged apart from comments; GPL/LGPL/AGPL remain outside policy. No newly copied implementation code.
- Tests: independently ran `cargo deny check` (advisories, bans, licenses, sources all OK), `scripts/check-licences.sh` (passed), and `cargo metadata --no-deps` (all six originals MIT/unpublished). No behavioral tests needed for this diff; full gate remains enforced by pre-push.
- Git/PR: notes map all eight criteria and record deliberate path cleanup. AGENTS changes only the reference-repository location phrasing, retaining the sibling-folder instruction; CLAUDE is unchanged. Neither file contains remaining absolute home paths or personal machine account names. Owner workflow/certificate/VM/service names describe intentional project infrastructure, not hidden credentials. Only this report is committed by this review; pre-existing untracked research reports are excluded. No merge performed.

Audit B2 disclosure/visibility decisions and B1 runner isolation remain outside PUB2. Approval covers the reviewed licensing/documentation change, not permission to publish the repository.
