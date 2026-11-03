Reviewer: APPROVED

# G1 round 3 review: PR #186 (smaller Claude guard), M2 only

Reviewer: Claude Code (Opus 5.5). This round re-checks M2 and the
endpoint-normalisation change at head `b7ab54d`
(`git diff 0d725ff..b7ab54d -- .claude/hooks/guard-git.sh scripts/test-guards.sh`:
+2 lines in the hook, +18 in the tests). I did not merge.

**M2 is fixed.** `api()` in `.claude/hooks/guard-git.sh:151-152` now strips
`https?://host`, leading slashes and `api/` from every endpoint before both
the REST and the GraphQL checks. It is case-insensitive, and `?`/`#` and
trailing `/` were already stripped. Every spelling that GitHub actually runs
as GraphQL is now checked, and so is every REST path written with a leading
slash or as a full URL. The change reopens none of B1, M1 or M3 and adds no
false positives. The blocker and all three majors are closed, so I approve.

## Which spellings GitHub runs as GraphQL (checked live, read-only)

I sent the harmless `query{viewer{login}}` to each spelling to see which ones
reach GraphQL:

| Endpoint spelling | Runs GraphQL? |
| --- | --- |
| `graphql`, `/graphql`, `https://api.github.com/graphql`, `HTTPS://API.GITHUB.COM/graphql`, `graphql?x=1`, `--hostname github.com /graphql` | yes |
| `/GRAPHQL` (502), `./graphql`, `graphql/.`, `//graphql`, `%67raphql`, `/api/graphql` (404) | no |
| `-X GET graphql -f query=…` | returns the schema; GitHub runs no query over GET, and mutations never run over GET |

## Probes against the new hook (PreToolUse JSON, compared with the old hook)

- **GraphQL, blocked as expected:** 7 endpoint spellings (`graphql`,
  `/graphql`, `graphql/`, the full URL in lower and upper case,
  `graphql?x=1`, `"/graphql#x"`) times these mutations: `mergePullRequest`,
  `deleteRef`, `updateRef`, `updateRepository`, `deleteRepositoryRuleset`,
  `updateRepositoryRuleset`, `deleteBranchProtectionRule` and
  `enablePullRequestAutoMerge`. Each spelling was also tried as
  `-X POST … --input <mutation.json>`, as `--hostname github.com … -F query=@<mutation.graphql>`,
  and as `-H … --method=post … --raw-field=query=…`. Also blocked: the
  mutation inside `timeout 5 …` and inside `bash -lc "…"`. All 77 were
  blocked.
- **GraphQL read-only queries, passing as expected:** the same 7 spellings
  with `-f query=`, `--input <query.json>` and `--hostname … -F query=@<query.graphql>`,
  and a query whose search string contains `mutation mergePullRequest(`. All
  22 passed.
- **REST with a leading `/`, `//`, `https://api.github.com/` or
  `HTTPS://api.github.com/`, blocked as expected:** `-X PUT …/pulls/9/merge`,
  `-H … --method PUT …/merge`, `POST …/merges`, `DELETE …/rulesets/1`,
  `PATCH repos/o/r -f visibility=private` and `DELETE …/git/refs/heads/x`.
  `api/v3/repos/…` and the GHES full URL were blocked too. The matching GETs,
  including ones with `-H` headers, all passed.
- **B1, M1 and M3 not reopened:** all 67 round-1 probes plus the wrapper and
  hooksPath spellings (`nohup`, `eval`, `xargs`, `GIT_CONFIG_KEY_0`,
  `--config-env=`) give the same results as in round 2.
- **No new false positives:** a `gh api -X GET search/issues` query, a
  `gh pr comment` body and a commit message that mention
  `/graphql mutation mergePullRequest` all pass, and so does
  `gh api /repos/o/r/rulesets --jq …`.

In total, 211 probe cases. The only misses are the known minors: #187
(`cp x scripts/` and `cp x .vscode/settings.json` wrongly blocked) and #188
(`--no-veri`).

## Minor (not blocking)

- #191 (new, filed this round): the two round-2 minors. `watch -n 5 gh pr merge`
  still gets through because `watch` is not unwrapped, and the read-only
  `git log -S core.hooksPath` is wrongly blocked.

## Still for the owner (from round 1, unchanged)

- No ruleset covers branches other than `main`, so force pushes and deletion
  of those branches rely on the local pre-push guard only. I recommend a third
  ruleset on `~ALL` branches with `deletion` and `non_fast_forward`.
- Keep the four `settings.json` push deny rules for now.
- AC6: the owner reads the `guard-git.sh` diff before the merge.

## What I checked

- `scripts/test-guards.sh` at `b7ab54d`: **496 passed, 0 failed**.
- Hosted CI at `b7ab54d`: Quality gate (Linux) passed, Platform crate
  (macOS) passed.
