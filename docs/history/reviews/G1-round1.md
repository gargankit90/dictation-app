Reviewer: CHANGES REQUESTED

# G1 round 1 review: PR #186 (smaller Claude guard)

Reviewer: Claude Code (Opus 5.5), standing in for the usual Reviewer because
Codex wrote this PR. Reviewed head `0e7b4fa` against `docs/milestones/G1.md`
and `docs/review-checklist.md`. I did not merge.

Summary: the approach is sound. Inspecting shell tokens instead of matching
regexes against the whole command fixes every false positive the spec lists,
and the push rules the hook drops are covered by the rulesets and the pre-push
guard, except for force pushes and deletion of non-`main` branches (see
"Removed push blocks"). Four gaps in the checks the PR keeps let commands
through that the old hook blocked. The worst is that `gh api` parsing misreads
the endpoint in GitHub's own documented spelling, so the merge API, ruleset
deletion and branch deletion all pass. That is a blocker because ruleset
deletion switches off the server-side protection this PR now relies on.

## Blocker

### B1. `gh api` takes an option's value as the endpoint, so merge, ruleset delete and ref delete all pass

`.claude/hooks/guard-git.sh:144-156`. The loop treats only `-X/--method` and
`-f/-F/--field/--raw-field/--input` as options that take a value. Any other
option with a value (`-H/--header`, `-q/--jq`, `-t/--template`, `--cache`,
`-p/--preview`, `--hostname`) that comes before the path leaves its value as
the first non-dash token. Line 155 then records that value as `endpoint`, so
the real path is never checked against line 162 or line 164. GitHub's REST
docs show this exact order (`--method X -H "Accept: …" -H "X-GitHub-Api-Version: …" /repos/…`).
Every command below passes the new hook. The old hook blocked all except the
fourth.

```sh
gh api --method PUT -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" /repos/gargankit90/dictation-app/pulls/186/merge -f merge_method=merge
gh api --method DELETE -H "Accept: application/vnd.github+json" /repos/gargankit90/dictation-app/rulesets/26422642
gh api --method PATCH -H "Accept: application/vnd.github+json" /repos/gargankit90/dictation-app -f visibility=private
gh api --method DELETE -H "Accept: application/vnd.github+json" /repos/o/r/git/refs/heads/m15-foo
gh api --jq .sha -X PUT repos/o/r/pulls/1/merge
gh api -X PUT "repos/o/r/pulls/186/merge?x=1"     # query string defeats the `$` anchor on line 162
```

The second and third commands remove the server-side protection that this PR
makes the primary defence. Deleting a ruleset turns it off, and a private repo
on GitHub Free gets no ruleset enforcement.

Fix: add every `gh api` option that takes a value to the skip list (`-H`,
`--header`, `-q`, `--jq`, `-t`, `--template`, `--cache`, `-p`, `--preview`,
`--hostname`, and `-X`/`--method`, which are already there). Strip `?query`
and `#fragment` from the endpoint before matching. To fail safe, also run the
`merge`/`merges`/repos-settings patterns against every positional token, not
only the first one. Add the six commands above to `scripts/test-guards.sh`.

## Major

### M1. `gh pr merge` gets through common command wrappers and `bash -lc`

`.claude/hooks/guard-git.sh:69-80`. Only `env`, `sudo`, `command`, `exec`,
`time` and `!` are skipped as wrappers, and a nested shell is checked only
when the literal token `-c` appears. The commands below pass the new hook. The
old hook blocked all of them.

```sh
timeout 300 gh pr merge 186 --merge
nohup gh pr merge 186 --merge
echo 186 | xargs gh pr merge --merge
bash -lc "gh pr merge 186 --merge"      # also -ec, -xc, zsh -lc
eval "gh pr merge 186 --merge"
```

`bash -lc` and `timeout` are ordinary things for an agent to type. The
`Bash(gh pr merge:*)` deny rule in settings.json is a prefix match, so it does
not catch these either.

Fix: also skip `timeout <n>`, `nohup`, `nice [-n N]`, `xargs [opts]`, `caffeinate`
and `stdbuf`. Treat any `-[a-z]*c` flag on `bash`/`sh`/`zsh` as `-c`, and
inspect every argument of `eval` (and `source`/`.` if they get a literal
string) as a shell command. Add tests.

### M2. GraphQL merge and ref mutations are no longer blocked

`.claude/hooks/guard-git.sh:138-166`. With `gh api graphql -f query=…` the
endpoint is `graphql`, so neither endpoint check matches. The old hook
(`origin/main:.claude/hooks/guard-git.sh:158-160`) blocked
`mergePullRequest`, `enablePullRequestAutoMerge`, `enqueuePullRequest`,
`mergeBranch`, `createCommitOnBranch`, `(create|update|delete)Ref` and
GraphQL queries read from a file. Now both of these pass:

```sh
gh api graphql -f query='mutation { mergePullRequest(input:{pullRequestId:"PR_x"}) { clientMutationId } }'
gh api graphql -f query='mutation { deleteRef(input:{refId:"x"}) { clientMutationId } }'
```

This undoes the merge role check (AC1 and focus item 2: "the merge API in all
`gh api` spellings").

Fix: when the endpoint is `graphql`, block if any argument matches the old
mutation list (add `deleteRepositoryRuleset|updateRepositoryRuleset|updateRepository`)
or reads the query from a file (`=@…`, `--input`). Plain queries such as
`query { repository { … } }` must still pass. Add tests for both.

### M3. Setting `core.hooksPath` through an environment variable or `--config-env` skips the pre-push guard, which is now the only local push check

`.claude/hooks/guard-git.sh:102`. The regex `^core\.hookspath` only matches a
token that starts with the key. Environment assignments are dropped before git
is checked (line 74), and `--config-env=…` doesn't start with `core.`. All of
these pass the new hook and were blocked by the old one (`hookspath` anywhere,
old line 149):

```sh
GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git push --force origin m15-foo
GIT_CONFIG_PARAMETERS="'core.hooksPath'='/dev/null'" git push origin +m15-foo
git --config-env=core.hooksPath=HP push --force origin m15-foo
```

Before this PR, Claude's hook stopped the force push itself, so a pre-push
bypass did not matter much. With push checks removed, these commands
force-push or delete any non-`main` branch with nothing local or server-side
in the way (see "Removed push blocks" below).

Fix: block when `core.hookspath` (case-insensitive) appears anywhere in any
token of a segment whose command is `git`, including its environment
assignments, unless it is a read-only `config --get`. Also block
`GIT_CONFIG_PARAMETERS`/`GIT_CONFIG_KEY_*` values that contain it, whatever
the command (for example `export`). Add tests.

## Minor (each filed as a `tech-debt` issue)

- m1 (#187). False positives in `protected()` (`guard-git.sh:36`) and the script
  check (`guard-git.sh:172`). `cp /tmp/new.sh scripts/` and
  `cp x .vscode/settings.json` are blocked because a bare `scripts`,
  `hooks` or any `settings.json` counts as a guard. `python3 tool.py .claude/hooks/guard-git.sh`
  (reading the file, which this review did to probe the hook) is blocked as
  "script editing guards". Backtick text inside a quoted heredoc body
  (`cat > f <<'EOF'`) is inspected as a command (`guard-git.sh:41-42`), so a
  markdown issue body that quotes a blocked command is refused. This review
  hit that while writing its issue bodies. Suggested fix: match a bare directory only for
  `rm`/`mv`/`rsync --delete` with a recursive flag, limit `settings.json` to
  `.claude/`, and check interpreter arguments only after `-c`/`-e`.
- m2 (#188). Abbreviated long options still skip hooks: git accepts
  `git commit --no-veri` and `git push --no-veri` (the old hook missed these
  too). Also, the settings.json Edit/Write deny rules don't cover
  `.git/hooks/pre-push`, so the Write tool can overwrite the installed hook
  (also true before this PR). Owner's file.
- m3 (#189). `scripts/guard-push.sh:2-6,25` comments and the refusal message still
  say "GitHub Free can't protect branches" and "the Tester merges it". This
  file was left unchanged on purpose; it is owner-only.

## Removed push blocks: what stops each one now

I checked both rulesets live (`gh api …/rulesets/26422642`, `…/26422652`):
both `enforcement: active`, `bypass_actors: []`, `current_user_can_bypass: never`.
I also read `scripts/guard-push.sh`, which applies to every ref.

| Removed check | Default branch `main` | Other branches | Tags |
| --- | --- | --- | --- |
| Force push (`--force`, `-f`, `--force-with-lease`, `+ref`) | Ruleset `main-public` (non_fast_forward) + pre-push | **Pre-push guard only** | Ruleset `immutable-tags` (`~ALL`: update, non_fast_forward) + pre-push |
| Deletion (`--delete`, `:ref`, `--mirror`, `--prune`) | Ruleset (deletion) + pre-push | **Pre-push guard only** | Ruleset (deletion) + pre-push |
| Direct push / refspec to the default branch (`HEAD:main`, `x:refs/heads/main`, `--all`) | Ruleset `pull_request` rule rejects direct pushes + pre-push | n/a | n/a |
| Tag move | n/a | n/a | Ruleset `update` + pre-push |
| Git aliases | Whatever the alias runs still meets the ruleset and pre-push | same | same |

What the owner should know: no ruleset covers branches other than `main`.
Force pushes and deletion of other branches (another worker's open PR branch,
for example) are now stopped only by the local pre-push guard. That guard is
missing when the push comes from a clone without `scripts/install-hooks.sh`,
and can be skipped with a hooks-path bypass (M3). The only remaining
Claude-side block is the four static settings.json deny rules for the most
common spellings. AGENTS.md's "never delete remote branches" rule is
therefore enforced only locally. Recommendation (the owner decides; not part
of this PR): add a third ruleset on `~ALL` branches with `deletion` and
`non_fast_forward`. That matches the hard rules exactly (no force push, no
remote branch deletion) and closes the gap server-side for every client.
Also note that `main-public` requires no status checks and no approvals, so
the hook's merge checks (B1, M1, M2) are the only thing between Claude and
merging a red PR.

## The four settings.json push deny rules: recommendation

Keep them. They cost nothing, they have no false positives on legitimate work
(none of our branch names start with `main`, and no agent needs `--force` or
`+ref`), and they are now Claude's only push check before git's pre-push hook,
which matters for non-`main` branches (above). They are prefix matches, so
they miss spellings such as `git push origin --force x`. Don't extend them;
fix that with the branch ruleset instead. If the owner adds the `~ALL`-branch
ruleset, the rules become pure redundancy and can be dropped then.

## AGENTS.md / CLAUDE.md

The hard-rules list is unchanged: never push to `main`, never force push,
never delete remote branches or move tags, never skip hooks, never edit the
guards, only the Code Reviewer merges. The rewritten enforcement paragraph
(`AGENTS.md:153-172`) describes both rulesets correctly against the live API,
and so do the new `CLAUDE.md:18-25` bullets. No rule's meaning changed. One
wording nit: "`main-public` … prevents force pushes and deletion of the
default branch" is right, but readers may assume the same holds for other
branches. Adding "other branches rely on the pre-push guard" would make the
gap above explicit. That is not required for this PR.

## What I checked

- `git switch --detach origin/g1-guard-simplify` at `0e7b4fa`. Read the full
  new `guard-git.sh`, the old one (`origin/main`), `settings.json`,
  `guard-push.sh`, `scripts/pre-push`, and the AGENTS.md/CLAUDE.md diff.
- `scripts/test-guards.sh`: **179 passed, 0 failed**.
- Fed 67 probe commands as PreToolUse JSON to both the new and the old hook.
  Kept blocks that work: plain `gh pr merge` (including `-R`, `GH_REPO=`,
  `bash -c`, after `cd &&`, `--auto`); `-X PUT pulls/N/merge` in its
  `-X`/`-XPUT`/`--method=put`/trailing-slash forms; `POST /merges` explicit
  and implied by `-f`; `DELETE rulesets`; `PATCH repos/o/r` (newly blocked;
  the old hook let it through); `--no-verify`; `commit -n`/`-anm`;
  `-c core.hooksPath=`; `git config core.hooksPath`; `LEFTHOOK=0`;
  `export LEFTHOOK=0`; `lefthook uninstall`; removing, overwriting or
  chmodding `.git/hooks/pre-push` (including the `$(git rev-parse --git-common-dir)` path);
  `>`, `sed -i` and `git checkout --` on guard files; `gh repo edit`;
  `gh release delete`; `gh alias set`.
- Spec false positives now pass: quoted "push to main / patch 2 / hooks /
  merges" commit messages, GET on `rulesets/<id>`, `rules/branches/main` and
  `git/refs/heads/main`, `--method GET` with `-f`,
  `git add crates/core/src/alias.rs`, `git log --grep="patch 2"`,
  `gh pr view --json mergeable`, `git config --get core.hooksPath`, and a
  `gh pr comment` body that mentions `gh pr merge`.
- Hosted CI at review time: Platform (macOS) passed; Quality gate (Linux)
  was still pending.
