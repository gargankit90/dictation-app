# MLIC1 review, round 1

PR: #333 at e6b15e80c203f55db81b285dc67853f9e4a9fb88
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | site/index.html:2624 | Footer still says “© 2026 Vocet Labs · MIT”, contradicting LICENSE and criterion 2. | Change the footer licence label to FSL-1.1-ALv2. |
| 2 | major | NOTICE:136; THIRD-PARTY-LICENSES.txt:148 | Both still claim “dictation-app's own code is MIT (see LICENSE).” This describes our own code, not third-party code. | Update that sentence in NOTICE and regenerate THIRD-PARTY-LICENSES.txt, preserving third-party grants and credits. |

Inline findings: [1](https://github.com/gargankit90/dictation-app/pull/333#discussion_r4647511567), [2](https://github.com/gargankit90/dictation-app/pull/333#discussion_r4647511781).

## Checklist

1. Scope: findings 1–2 prevent criterion 2. The official template has only two placeholders, so omitting the spec's nonexistent Software placeholder is correct. The spec explicitly authorizes the AGENTS.md licence clarification.
2. Correctness: findings 1–2; otherwise wording and metadata agree. No runtime logic changes.
3. Security and privacy: OK; no changes to permissions, logging, secrets or network behavior.
4. Architecture: OK; no dependency or architectural changes.
5. Rust and TypeScript practice: OK; Rust Link documentation and generated TypeScript match.
6. Licences and reuse: findings 1–2. Downloaded https://fsl.software/FSL-1.1-ALv2.template.md on 2026-11-20 and diffed against LICENSE: only the copyright line differs, with year 2026 and licensor Ankit Garg (Vocet Labs). NOTICE and THIRD-PARTY-LICENSES.txt are unchanged; no third-party grant is removed. about.toml and deny.toml change comments only, with no broader dependency allowance. Workspace crates inherit FSL and publish=false; npm metadata agrees. No copied code or new dependency.
7. Tests: About test asserts FSL is present and MIT absent. Attempted the targeted Vitest case, but this worktree lacks vite and @vitejs/plugin-react, so it failed before running tests. Full gate not rerun by reviewer; developer reports a full pass, and CI checks were pending when inspected. Tester and CI must confirm execution.
8. Git and PR: OK; Conventional Commits and detailed notes; no guard changes. No minor findings. The milestone records owner authorization for relicensing. Do not merge: changes requested; Tester PASS and successful required checks remain necessary.

## Verification limits

This review verifies the text and metadata against the official template, not legal advice or an independent audit of all historical copyright ownership. The Lead's manual-check comment was present; findings above still require fixes and re-review.
