Reviewer: CHANGES REQUESTED

# G1 (PR #281) review, round 1

PR: #281 at f807d2c
Reviewer: Claude Opus 5.5 (the author is Codex)
Verdict: CHANGES REQUESTED

The accepted trade-off (fail open; a deliberately disguised command can pass)
is not re-argued here. Every miss below is an ordinary spelling that a
well-meaning agent would write. I ran 171 must-block and 91 must-pass commands
through the real hook. Results below are from those runs, except where a row
says otherwise. The `Bash(gh pr merge:*)` deny rule may stop some of the
`gh pr merge` forms; I did not test that, and it doesn't cover `gh api`, `gh
repo` or shell writes.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| B1 | blocker | `scripts/test-guard-commands.py:58` | The required check `Quality gate (Linux)` fails. CI's shallow checkout has no `origin/main`: `fatal: invalid object name 'origin/main'` in [run 42083299327](https://github.com/gargankit90/dictation-app/actions/runs/42083299327). The gate stops at `test-guards.sh`, so `check-workflows.sh` and later steps did not run in CI. A shallow local clone fails the same way. | Don't read a remote ref in a gate test. Compare the deny list with a list written in the test (the 19 expected rules). This also stops a moving baseline, where the test compares against whatever `main` holds. |
| M1 | major | `.claude/hooks/guard-git.sh:10` | A backslash line continuation splits the command. The escaped newline becomes a `"\n"` token, which line 21 treats as a separator, so `gh api`, `-X PUT` and the endpoint land in different segments. Passes: `gh api \⏎ -X PUT \⏎ repos/o/r/pulls/12/merge`, `gh api repos/o/r/pulls/12/merge \⏎ -X PUT`, `gh api graphql \⏎ -f query='mutation { mergePullRequest(...) ... }'`, `gh api \⏎ --method DELETE \⏎ repos/o/r/rulesets/1`, `gh pr \⏎ merge 12`. The old hook removed `\⏎` first. | Before lexing: `command.replace("\\\n", " ")`. |
| M2 | major | `.claude/hooks/guard-git.sh:10` | Without `whitespace_split`, shlex splits words on `{ } $ : ( ) , @`. The repo's own spelling is broken up, so the endpoint regex never matches. Passes: `gh api -X PUT repos/{owner}/{repo}/pulls/12/merge` (the form in `docs/agents/reviewer.md`), `gh api -X DELETE repos/{owner}/{repo}/rulesets/1`, `repos/:owner/:repo/...`, `repos/$OWNER/$REPO/...`. Also `echo x > $PWD/scripts/pre-push`. The form `"repos/$GITHUB_REPOSITORY/rulesets/1"` passes for a different reason: line 36 expects two path segments after `repos/`. | `lexer.whitespace_split = True` and add `()` to `punctuation_chars` and to the separator check on line 21, as the old hook did. Let line 36 accept one or two segments for the repo part. |
| M3 | major | `.claude/hooks/guard-git.sh:25` | Only `args[0]` is checked, so a command inside ordinary shell structure or after a prefix passes. Passes: `if gh pr checks 12; then gh pr merge 12; fi`, `for n in 1 2; do gh pr merge $n; done`, `(gh pr merge 12)`, `{ gh pr merge 12; }`, `x=$(gh pr merge 12)`, `! gh pr merge 12`, `GH_REPO=o/r gh pr merge 12`, `GH_TOKEN=x gh pr merge 12`, `env ...`, `command ...`, `time ...`, `timeout 60 gh pr merge 12`, `if true; then rm scripts/pre-push; fi`, `(rm scripts/pre-push)`, `FOO=1 rm scripts/pre-push`, `sudo rm scripts/pre-push`. | Strip leading `then do else elif if while until { ! time command env sudo nohup` and `NAME=value`, and `timeout <n>`, in one loop before reading `exe`. |
| M4 | major | `.claude/hooks/guard-git.sh:10` | shlex's comment handling consumes the newline, so the next line joins the commented line's segment. `git status # check first⏎gh pr merge 12` passes, because `exe` is `git`. Multi-line scripts with trailing comments are normal. shlex also treats a `#` inside a word as a comment. | Before lexing: `re.sub(r"(?m)[ \t]+#[^\n]*$", "", command)`. |
| M5 | major | `.claude/hooks/guard-git.sh:26-28` | Plain shell writes to real guard paths pass (criterion 2). (a) No space after a closing quote: the lookbehind on line 26 rejects `echo "x">.claude/settings.json` and `printf 'x'>scripts/pre-push`. (b) Line 27 checks only the last word for `cp`, `install` and `ln`, so `cp /tmp/x scripts/pre-push 2>/dev/null` and `cp /tmp/x scripts/pre-push > /dev/null` pass. (c) git writes to the work tree are not matched: `git checkout -- .claude/hooks/guard-git.sh`, `git checkout origin/main -- scripts/pre-push`, `git restore scripts/pre-push`, `git rm scripts/pre-push`, `git mv scripts/pre-push /tmp/x`. The old hook blocked `git checkout --`. | (a) Check that the `>` is outside quotes by comparing tokens, or use a lookbehind that only rejects an opening quote (`'>'`). (b) Drop redirections and their targets from `args` before taking the last word. (c) Match `git checkout`, `restore`, `rm` and `mv` with a protected path in the arguments. |
| M6 | major | `.claude/hooks/guard-git.sh:33,36` | False positive on the data this milestone set out to free. The endpoint regex searches the whole joined text, field values included, so a POST comment that mentions a policy endpoint is refused: `gh api repos/o/r/pulls/12/comments -f body='see repos/o/r/rulesets/1 for details' ...`, `... -f body='this is on repos/o/r in general'`, `gh api -X POST repos/o/r/issues/12/comments -f body='the endpoint repos/o/r/git/refs is wrong'`. `gh api` with `-f body=` is how `docs/agents/reviewer.md` posts inline comments. `-X` in line 33 searches the same text, so a `-X GET` inside a field value can win over the real method if it comes first. | Walk `args`: skip options and their values (`-H -q -t -f -F --jq --field --raw-field --input --hostname ...`), take `-X`/`--method` from real options and the endpoint from the first positional word. Run the regexes only on those. Keep the GraphQL check on the query value. |
| M7 | major | `scripts/test-guard-commands.py:10-43` | The table has no case for line continuation, `{owner}/{repo}` or `$VAR` endpoints, `then`/`do`/subshell/env prefixes, trailing comments, `-X` after the endpoint, a `-f body=` value that mentions an endpoint, `"x">path`, or `cp ... 2>/dev/null`. That is why M1-M6 were not caught. | Add the commands listed in M1-M6, and `-X` before and after the endpoint for each policy path. |
| M8 | major | `docs/history/milestones/G1.md:1` | This PR replaces the spec of the earlier G1 (PR #186). `docs/history/README.md:28` still describes that spec as "the push guard, simplified in three review rounds" and links it with its three reviews, so the link now points at the wrong document. The name clash came from the Lead's assignment; the Lead asked for it to be raised as a major. | Restore `docs/history/milestones/G1.md` from `origin/main` and move this spec to `docs/history/milestones/G1-simple-guard.md`. Update any references to the moved spec. This review is `G1-pr281-round1.md` so that the old `G1-round1.md` stays intact. |
| m1 | minor | `docs/review-checklist.md:14-16` | Stale: it says the hook checks "hook bypass" and calls a way around that major. Since this PR it does not. Criterion 8 doesn't list this file, but it misdirects later reviews. | One-sentence update in this PR. |
| m2 | minor | `.claude/hooks/guard-git.sh:27-31` | Less common spellings pass: `sed -Ei ... guard`, `perl -pi -e ... guard`, `echo x >| guard`, `rm -rf .claude`, `rm -rf scripts`, `gh pr --repo=o/r merge 12`, `echo 12 \| xargs gh pr merge`. The old hook blocked `rm -rf .claude` and `rm -rf scripts`. | Add the cheap ones: `-[a-zA-Z]*i` for sed, `>\|`, parent directories. Accept the rest. |
| m3 | minor | `.claude/hooks/guard-git.sh:5` | `python3 -` puts the current directory on `sys.path`. With a `shlex.py` in the working directory the hook imported it (tested), so that file ran and the check was skipped. The repo has no such file, and this does not run the inspected command. | `python3 -I -`. |
| m4 | minor | `scripts/test-guards.sh:43` | A stray `python3 .../test-check-rulesets.py` sits inside the "scratch repo resolves wrongly" safety branch, before the `echo` and `exit 1`. If it fails, `set -e` skips the explanation. | Delete the line; the call at the end of the file stays. |
| m5 | minor | `scripts/test-guards.sh` | The inherited-`GIT_DIR` isolation test was removed (see below). The `unset` it protects is still the only thing stopping this script from rewriting the real repo when pre-push runs it. | Restore it, limited to the scratch-repo section if time matters. |
| m6 | minor | `.github/workflows/rulesets.yml:4` | GitHub disables scheduled workflows in public repositories after 60 days without repository activity. The docs list other audit limits but not this one. The first hosted run with the real `GITHUB_TOKEN` also can't happen until after merge. | Add a sentence to `docs/ENGINEERING.md`. The Lead runs `gh workflow run rulesets.yml` once after merge and checks that it is green. |

## Requested checks

1. **Plain forms blocked: no.** Single-line forms with literal `o/r`, `-X` in
   any position, `--method`, `-XPUT`, lowercase methods, URL prefixes, `-R` and
   `--repo` placement, `gh repo delete/edit`, `gh release delete` and inline
   GraphQL mutations are blocked. So are `tee`, `mv`, `rm`, `unlink`, `touch`,
   `truncate`, `chmod`, `install`, `ln`, `sed -i` and redirects to every guard
   path. See M1-M5 for what passes. Running my prototype of the M1-M4 fixes (a
   few lines in a scratch copy) reduced the 49 misses to 20: M5, m2, the
   `$REPO` forms and two items outside the criteria.
2. **False positives: mostly gone.** Multi-line scripts with apostrophes, Vocet's
   `settings.json` outside `.claude`, read-only references, redirects from
   guards to `/tmp`, commit messages and `gh pr`/`gh issue` bodies mentioning
   blocked commands all pass. `gh api` GETs pass, including `--method=get`,
   `--paginate`, `-f` with `-X GET`, and `gh api graphql` queries.
   `resolveReviewThread` and `pulls/<n>/update-branch` pass, which the merge
   procedure needs. See M6 for the exception.
3. **Fail open: OK.** Empty, invalid and wrong-type input, unterminated quotes,
   heredocs, a missing `python3` and a 2 MB command (argument too long) all exit
   0. Nothing executed: canaries in plain, `$(...)`, backtick, heredoc and
   endpoint positions never fired. No regex slowdown on long input (under 0.15 s
   for every input tried). `cat` waits on stdin, which Claude Code closes, and
   the hook has no loop. See m3.
4. **Guards: OK.** `scripts/guard-push.sh`, `scripts/pre-push`,
   `scripts/install-hooks.sh` and `lefthook.yml` have no diff from `main`. The
   settings diff removes exactly the four `Bash(git push ...)` rules; `Bash(gh pr
   merge:*)` and all Edit/Write denies remain.
5. **Workflow: OK, not yet run on a runner.**
   - Failure cases: a difference, a disabled or missing ruleset, an extra
     ruleset, a missing field and an API error all exit non-zero. The comparator
     tests cover the first four; I read the last two in the code.
   - Limit stated in the three docs and the log warning. `contents: read`, no
     secrets, pinned checkout. `scripts/check-workflows.sh` passes locally.
   - Not permanently red: an unauthenticated read of the live rulesets matches
     the snapshot, with `bypass_actors` hidden on all three. Exact comparison
     does mean a new field added by GitHub turns it red, which is loud and fixed
     by one snapshot update.
   - Not silently always green, apart from the disclosed bypass gap and m6.
   - Expected file accurate: `--require-bypass` on the live repo printed "all
     visible policy fields match (including bypass actors)".
   - `--require-bypass` is safe locally: only `gh api` GETs, no token or file
     output, and it fails if the field is hidden.
6. **Docs: OK except m1.** AGENTS.md, CLAUDE.md and docs/ENGINEERING.md describe
   the trade-off and the audit limit. The rule against skipping hooks stays at
   AGENTS.md line 112. Their "matches plain ... commands" holds once M1-M5 are
   fixed.
7. **Suite speed: OK.** 3.4 s here, three runs in a row (one run during heavy
   load took 7.8 s). Lost coverage is listed below.

## Coverage lost from scripts/test-guards.sh

Retained unchanged: the 11 push-guard cases, the 2 pre-push cases and the
installed-hook and linked-worktree matrix.

| No longer tested | Still matters? |
| ---------------- | -------------- |
| Hook-bypass blocks (`--no-verify`, `-n`, `core.hooksPath`, `LEFTHOOK*`, `GIT_CONFIG_*`, `--config-env`, config scopes) | No: criterion 3 allows them, and CI enforces the gate. The new table tests a sample as must-pass. |
| Git alias and `gh alias` checks; push-command allowed cases | No: out of scope. |
| Patch inspection (`git apply`, `git am`, `patch`), `--input` and `-F query=@file` contents | No: criterion 1 forbids reading those files. |
| `sh -c`, `bash -c`, `eval`, `$(...)`, backtick unwrapping | Mostly no (treated as disguised). `x=$(gh pr merge 12)` is in M3 because the fix is cheap. |
| Fail-closed input cases | No: inverted on purpose and tested as fail-open. |
| Wrappers (`timeout`, `env`, `command`, `nohup`, `nice`, `xargs`) before `gh` | **Yes** for the common ones: M3. |
| Subshell writes, `cd scripts && cp ...`, `sed -E -i`, redirect after `2>&1` | **Yes** for subshells: M3. `sed -E -i` and `2>&1 > guard` pass the hook's blocks today (tested) but have no test. `cd scripts && cp` is relative-path disguise: no. |
| `git checkout -- <guard>` | **Yes**: M5. |
| `rm -rf .claude`, `rm -rf scripts` | Minor: m2. |
| `gh api` option permutations (`-H`, `--jq`, `-t`, `--input`) before the request; `?x=1#fragment`; URL prefixes | **Yes**, a few cases: the M6 rewrite of method and endpoint parsing needs them. |
| Allowed cases such as `gh pr comment --body "mutation { ... }"` | Partly kept. M7 adds what's needed. |
| The block message naming the Code Reviewer and the owner | Minor: one assertion would keep it. |
| Per-path Edit/Write deny assertions | Replaced by the baseline comparison, which fails in CI (B1). |
| Inherited-`GIT_DIR` isolation | **Yes**: m5. |

## Checklist

1. Scope: OK apart from M8; the changes stay within the milestone. Criteria 1-2 and 5 not met
   yet (B1, M1-M7); criteria 3, 4, 6, 7, 8 met, with m1 and m6 as notes.
2. Correctness: B1, M1-M6.
3. Security and privacy: OK; no secrets, no new network destinations in the
   product. See m3.
4. Architecture: not applicable; no crate changes.
5. Rust and TypeScript practice: not applicable.
6. Licences and reuse: OK; nothing adapted.
7. Tests: M7, m4, m5.
8. Git and PR: M8. Guard edits are owner-approved for G1. PR notes are
   complete, but report the gate as passing locally only; the required CI check
   fails (B1).
