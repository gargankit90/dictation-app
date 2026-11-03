Reviewer: CHANGES REQUESTED

# G1 round 2 review: PR #186 (smaller Claude guard)

Reviewer: Claude Code (Opus 5.5). This round re-checks B1, M1, M2 and M3
from round 1 at head `1500141`, plus everything changed in
`git diff e005df9..1500141 -- .claude/hooks/guard-git.sh scripts/test-guards.sh`.
I did not merge.

B1, M1 and M3 are fixed, and M2 is fixed when the endpoint is spelled
`graphql`. The changes add no new false positives that matter. One problem
remains: the GraphQL check runs only when the endpoint is exactly `graphql`.
`gh` also sends `/graphql` and the full URL to the GraphQL API, so a merge
mutation still gets through. That is the same impact as M2, so I'm still
requesting changes. The fix is one line plus tests.

## Major

### M2 (still open). GraphQL mutations get through when the endpoint is written `/graphql` or as the full URL

`.claude/hooks/guard-git.sh:155` checks `if endpoint != "graphql": continue`.
Endpoints keep their leading slash and host (only `?`/`#` and a trailing `/`
are stripped), so these commands pass the new hook. The old hook blocked them.

```sh
gh api /graphql -f query='mutation { mergePullRequest(input:{pullRequestId:"PR_x"}) { clientMutationId } }'
gh api https://api.github.com/graphql -f query='mutation { mergePullRequest(input:{pullRequestId:"PR_x"}) { clientMutationId } }'
```

I checked that `gh` really runs GraphQL for both spellings, using a harmless
read: `gh api /graphql -f query='query{viewer{login}}'` and the full-URL form
both returned `{"data":{"viewer":{"login":"gargankit90"}}}`. Merging, deleting
refs and deleting rulesets through these spellings therefore works. The
pre-push guard does not stop any of them, and the `main-public` ruleset
requires no checks or approvals.

Fix: normalise each endpoint before the comparison. Strip a leading
`https?://[^/]+`, then leading `/`, then treat both `graphql` and
`api/graphql` (GitHub Enterprise) as GraphQL, for example
`re.sub(r"^(?:https?://[^/]+)?/*(?:api/)?", "", endpoint) == "graphql"`. Add
both commands above as block cases and `gh api /graphql -f query='query{viewer{login}}'`
as a pass case in `scripts/test-guards.sh`.

## Minor (not blocking)

- `watch -n 5 gh pr merge 9` gets through because `watch` is not in the
  wrapper list (`guard-git.sh:68-79`). Add `watch` with `-n`/`--interval` as
  a value-taking option when you fix M2.
- New false positive: the `core.hookspath` check (`guard-git.sh:210`) now
  matches any git argument that contains the text, so
  `git log -S core.hooksPath` is blocked. (`git log --grep=hooksPath` and
  commit messages still pass.) Adding this to #187 is enough.
- Still open from round 1, already filed: #187 (`cp x scripts/`, any
  `settings.json`, heredoc bodies inspected; this round hit the heredoc case
  again while writing probes) and #188 (`--no-veri`).

## Round 1 findings: status

| Finding | Status | Evidence (all fed to the new hook as PreToolUse JSON) |
| --- | --- | --- |
| B1 `gh api` option values taken as the endpoint | **Fixed** | Blocked: GitHub's documented `--method PUT -H … -H … /repos/…/pulls/186/merge`, `--method DELETE -H … rulesets/…`, `--method PATCH -H … repos/o/r -f visibility=private`, `DELETE -H … git/refs/heads/…`, `--jq … -X PUT …/merge`, `?x=1` and `#x` suffixes, `--header=…`, `-HAccept:x -XDELETE`, `--cache 1h`, `-p nebula`, the full `https://api.github.com/repos/…` URL. |
| M1 `gh pr merge` inside wrappers | **Fixed** (except `watch`, minor) | Blocked: `timeout 300`, `timeout --foreground -s KILL 30`, `nohup`, `nice -n 5`, `xargs`, `xargs -I{}`, `env -S "…"`, `sudo -u me`, `bash -lc`, `bash -euo pipefail -c`, `zsh -lc`, `sh -xc "timeout 5 gh pr merge"`, `eval "…"`, `cd /tmp;gh pr merge`, `true&&gh pr merge`. |
| M2 GraphQL merge/ref mutations | **Partly fixed** | Blocked with endpoint `graphql` or `graphql/`: `mergePullRequest`, an aliased `x: mergePullRequest` with variables through `--raw-field=query=`, `deleteRef`, `deleteRepositoryRuleset`, `updateRepository`, a mutation in a `--input` JSON file, a mutation in a `-F query=@file`, and `--input -`. **Not blocked:** `/graphql` and the full URL (above). |
| M3 `core.hooksPath` via environment or `--config-env` | **Fixed** | Blocked: `GIT_CONFIG_COUNT/KEY_0/VALUE_0` (also with `CORE.hooksPath`), `GIT_CONFIG_PARAMETERS`, `export GIT_CONFIG_KEY_0=…`, `--config-env=…` and `--config-env …`, `-c CORE.HOOKSPATH=`, and `-c core.hooksPath=/x config --get core.hooksPath` (a read used as cover). |

## No new false positives (apart from the minor above)

These pass: read-only `gh api` calls with `-H` headers (rulesets, and a GET on
`pulls/186/merge`), `--jq … --paginate`, `-X GET search/issues -f q="mergePullRequest rulesets"`,
GraphQL `query { … mergeable … }`, a GraphQL search whose string contains
`mutation mergePullRequest(`, `--input` with a query-only JSON file,
`/graphql` with a query, commit messages and `gh pr comment` bodies that
mention `core.hooksPath`, `LEFTHOOK=0`, `eval`, `xargs`, `timeout`, `bash -lc`
and `gh pr merge`, `echo "eval gh pr merge"`, `git config [--get] core.hooksPath`,
`timeout 600 cargo nextest run`, `xargs -n1 echo`, `bash -lc "npm ci && npm run build"`,
`git log --grep=hooksPath`, and a non-hooks `GIT_CONFIG_PARAMETERS`. All the
spec's false-positive cases from round 1 still pass. With the new
`whitespace_split`, `echo x>file` and `>>file` without spaces are still
caught.

## What I checked

- Read the full hook diff `e005df9..1500141` (`unwrap()`, `environment()`,
  `api()`, the hooksPath change, `whitespace_split`).
- Re-ran all 67 round-1 probes plus 37 new spellings against both the new
  hook and the old one from `main`. Apart from the M2 and minor items above,
  every miss was a known round-1 minor (#187, #188).
- `scripts/test-guards.sh` at `1500141`: **435 passed, 0 failed**.
- Hosted CI at `1500141`: Quality gate (Linux) passed, Platform crate (macOS) passed.
