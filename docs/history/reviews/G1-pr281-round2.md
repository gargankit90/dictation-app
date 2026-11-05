Reviewer: CHANGES REQUESTED

# G1 (PR #281) review, round 2

PR: #281 at ad0fab7
Reviewer: Claude Opus 5.5 (the author is Codex)
Verdict: CHANGES REQUESTED, for one major with a one-line fix

Every round 1 blocker, major and minor is fixed except part of M2. That part is
the one remaining major: `repos/$REPO/...` endpoints. Nothing else blocks.

## Command results

| Set | Commands | Wrong in round 1 | Wrong now |
| --- | -------- | ---------------- | --------- |
| Round 1, must block | 171 | 49 | 3 |
| Round 1, must pass | 91 | 4 | 0 |
| New this round, must block | 50 | not run | 11 |
| New this round, must pass | 78 | not run | 8 |

The 3 in the round 1 set: `"repos/$REPO/pulls/12/merge"` is R1 below. The other
two are outside the criteria: `disablePullRequestAutoMerge`, and
`orgs/o/rulesets/1` on a repository owned by a user.

The 19 in the new set: 7 are R1. Of the other 12, 10 are r1 and r2 below, and
2 are not findings: `gh pr merge-status` isn't a real command, and `perl -Ilib`
on a guard path only reads it. That last match comes from the `i` in `lib`.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| R1 | major | `.claude/hooks/guard-git.sh:62` | The normalisation only knows the name `GITHUB_REPOSITORY`. Any other single variable holding `owner/repo` still misses policy paths and hits false positives. Passes: `gh api -X PUT repos/$REPO/pulls/12/merge` (a real REST merge), `gh api -X DELETE repos/$REPO/rulesets/1`, `gh api -X DELETE repos/$REPOSITORY/git/refs/heads/x`, `gh api -X PUT repos/${REPO}/branches/main/protection`, `gh api -X PUT repos/$GH_REPO/pulls/12/merge`. Refused: `gh api repos/$REPO/issues -f title=test`, `gh api repos/$REPO/labels -f name=test`, because `repos/$REPO/issues` looks like a repository root. The checklist rates a missed ordinary spelling as major. No script or doc I searched uses this spelling today, so the Lead may choose to downgrade it. | Replace the line with `endpoint = re.sub(r"(repos/)\$\{?(?!OWNER\|USER)\w+\}?(?=/[^/${:]\|$)", r"\1owner/repo", endpoint)` (without the table's backslashes before the pipes). In a scratch copy this gave the right result for all 15 commands I tried, including `$OWNER/$REPO`, `$OWNER/r` and `$GITHUB_REPOSITORY`. Add those commands to the test table. |
| r1 | minor | `.claude/hooks/guard-git.sh:22` | The parent-directory protection added for m2 matches at any depth and misses a trailing slash. Refused: `rm -rf /tmp/build/scripts`, `rm -rf "$tmp/.claude"`, `echo x > /tmp/scripts`, `cp /tmp/foo.sh scripts`. Passes: `rm -rf scripts/`, `rm -rf .claude/`. | Match the bare parents with their own anchored pattern, `^(?:\./)?(?:scripts\|\.claude)/?$`, for `rm` and `mv` only. |
| r2 | minor | `.claude/hooks/guard-git.sh:37,42` | Less common spellings pass: `timeout -k 5 60 gh pr merge 12`, `env -u X gh pr merge 12`, `xargs -n1 gh pr merge`, `result="$(gh pr merge 12 2>&1)"`, `git -C . checkout -- scripts/pre-push`, `printf 'x'>'scripts/pre-push'`. | Accept them. Each fix adds parsing, and the unquoted and option-free forms are blocked. |
| r3 | minor | `.claude/hooks/guard-git.sh:11` | The quote regex is quadratic on many escaped double quotes outside a string: 60 KB took 7 s and 120 KB ran past my 25 s limit. Realistic large commands stay fast: 100 KB of `python3 -c` or a shell script with apostrophes took about 0.1 s. I did not test Claude Code's own hook timeout. | Pass very long commands unchecked: `if len(command) > 65536: sys.exit(0)`. |
| r4 | minor | `docs/history/milestones/G1-simple-guard.md:6,20` | Two figures are now tight or out of date. The hook is 68 lines against "about 30". The suite took 4.6-4.9 s here, while the PR reports 5.3-11.4 s under load, against "under 5 s". | The Lead decides whether to accept these or update the spec wording. See the assessment below; I would not hold the PR for either. |

## Round 1 findings

| # | Status | Checked by |
| - | ------ | ---------- |
| B1 | Fixed | The test now holds the 19 rules as a literal list, and the two required checks are green at ad0fab7. |
| M1 | Fixed | All continued `gh api`, GraphQL and `gh pr` forms are blocked. |
| M2 | **Partly fixed: R1** | `{owner}/{repo}`, `:owner/:repo`, `$OWNER/$REPO` and `$GITHUB_REPOSITORY` are blocked. `$REPO` is not. |
| M3 | Fixed | `then`, `do`, `(`, `{`, `$(`, `!`, assignments and the option-free wrappers are blocked. Wrappers with options: r2. |
| M4 | Fixed | A trailing comment no longer hides the next line. Quoted `#` stays data. |
| M5 | Fixed | Quoted-text redirects, `cp ... 2>/dev/null`, `cp ... > /dev/null`, and `git checkout`, `restore`, `rm`, `mv` are blocked. |
| M6 | Fixed | Method and endpoint come from real arguments, so comment bodies are data. A `-X GET` inside a field no longer changes the method, in either direction. |
| M7 | Fixed | 208 checks in the table, covering each of M1-M6. The `$REPO` spelling is missing: R1. |
| M8 | Fixed | `docs/history/milestones/G1.md` has no diff from `main`, this spec is in `G1-simple-guard.md`, and the old `G1-round1.md` is unchanged. |
| m1 | Fixed | `docs/review-checklist.md` no longer claims hook-bypass checks. |
| m2 | Fixed, with a side effect | `sed -Ei`, `perl -pi`, `>\|`, `rm -rf .claude`, `rm -rf scripts` and `--repo=` are blocked, and plain `xargs`. The side effect is r1. |
| m3 | Fixed | `python3 -I`, with a regression test using a local `shlex.py`. |
| m4 | Fixed | The stray line is gone. |
| m5 | Fixed | Restored. Removing the `unset` line in a scratch copy made `isolation: inherited GIT_DIR run passes` fail, so the test does catch it. |
| m6 | Fixed | Documented. The first hosted run stays a Lead step after merge. |

## Requested checks

- **Still simple and fail-open: yes.** Empty, invalid and wrong-type input,
  unterminated quotes, heredocs and a 3 MB command all exit 0. Canaries in plain,
  `$(...)`, backtick, endpoint and `-X` positions never fired. The hook has no
  `eval`, `exec`, `subprocess` or `open`, and reads no file named in a command.
  It can be slow on one pathological input (r3).
- **Growth: reasonable.** 41 to 68 lines, against 278 before this PR. Most of it
  is what round 1 asked for: redirect handling (9 lines) and the `gh api`
  argument loop (13 lines). It is simple code, though lines 22, 42 and 64 are
  long. I don't suggest cutting anything.
- **The `GITHUB_REPOSITORY` follow-up: no hole for that name.** With
  `$GITHUB_REPOSITORY`, bare and `${...}` in the tests, plus the quoted forms I
  added, rulesets, `pulls/12/merge`, `merges`, the repo root, `git/refs`,
  `branches` and `settings` are all blocked. Issue, comment and `update-branch`
  calls and GETs pass. The rewrite only runs after the method check and only
  changes the variable, so it can't turn a blocked path into an allowed one.
  The gap is other variable names (R1), which were already unblocked.
- **Push scripts: unchanged.** `scripts/guard-push.sh`, `scripts/pre-push`,
  `scripts/install-hooks.sh` and `lefthook.yml` have no diff from `main`. The deny
  list has 19 rules and keeps `Bash(gh pr merge:*)`.
- **Workflow and docs: unchanged and OK.** `scripts/check-workflows.sh` and
  `scripts/check-doc-links.sh` pass. `--require-bypass` still matches the live
  rulesets.
- **Behind `main`:** #283 touches only `tests/e2e` and a review file, and a trial
  merge has no conflicts. The branch still needs updating before merge, as
  `main-public` requires, and CI must pass again.
