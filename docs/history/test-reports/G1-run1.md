# G1 test report, run 1

PR: #186 at 0e7b4fa
Result: FAIL (hosted CI red; the hook's behaviour itself is correct)

| # | Criterion | Test(s) | Result | Notes |
| --- | --- | --- | --- | --- |
| 1 | Kept blocks only | 65 own cases (below), `scripts/test-guards.sh` | PASS | All kept blocks exit 2 in many spellings. |
| 2 | No push/alias inspection; quoted text and GET pass | own cases | PASS | `git push origin main`, `--no-verify` or `gh pr merge` as commit-message text, rules/rulesets/git/refs GETs all exit 0. |
| 3 | Messages name Reviewer / owner | read hook | PASS | "The Code Reviewer merges; the owner changes guards. Ask the Lead." |
| 4 | Consolidated suite, m02 retired | `scripts/test-guards.sh` locally; mutation | PASS locally, FAIL in CI | Local: 179 passed, 0 failed. `tests/guards/` is gone. See finding 1. |
| 5 | AGENTS.md / CLAUDE.md | diff read | PASS | No rule's meaning changes: the Hard rules section is untouched. The enforcement paragraph is reworded for the public repo with server-side rulesets; pre-push hook text unchanged; the Claude-hook bullet now lists the reduced checks. CLAUDE.md only adds notes. |
| 6 | Owner diff read | n/a | not tested | Manual. |

## Finding 1 (blocker): hosted CI "Quality gate (Linux)" fails

`gh pr checks 186`: Quality gate (Linux) fail, Platform crate (macOS) pass.
Log (run 41578178357), step `scripts/test-guards.sh`:

```
./scripts/install-hooks.sh: line 9: lefthook: command not found
FAIL: installed hooks + linked worktree push matrix (expected exit 0, got 1)
FAIL: isolation: run with an inherited GIT_DIR passes (expected exit 0, got 1)
test-guards: 177 passed, 2 failed
```

The new suite runs `scripts/install-hooks.sh` (which calls `lefthook`) in a
temp repo, but the CI runner has no lefthook on PATH. Locally lefthook exists,
so it passes (179/0). Main's CI was green, so this comes from the PR.
Fix: install lefthook in the CI job, or make those two cases skip or stub when
`command -v lefthook` fails (the file already guards one other lefthook use
that way).

## Own hook cases: 65 run, 65 pass, 0 fail

Method: pipe `{tool_name:"Bash",tool_input:{command}}` JSON into the hook.

Block (exit 2): PR merge with flags before and after, with `-R o/r` and
`--repo o/r` in both positions, `--auto`; `gh api -X PUT .../pulls/1/merge`
(also `--method PUT`, `--method=PUT`, `-XPUT`); `gh api -X POST .../merges`;
`gh alias set/import`; `git -c core.hooksPath=/dev/null commit`;
`git config core.hooksPath`; `git commit -anm x`, `-n`, `--no-verify` (commit
and push); `LEFTHOOK=0 git push`, `LEFTHOOK_EXCLUDE=...`, `env LEFTHOOK=0`;
`lefthook uninstall`; guard edits by `sed -i`, `tee`, `>` (hook and
guard-push.sh), `cp`, `rm`, `mv`; the merge command inside `bash -c`, `$(...)`
and `&&` chains; `gh repo edit/delete`; `gh release delete`;
`gh api -X DELETE .../rulesets/1`, `-X PATCH .../rulesets/1`,
`-X DELETE .../git/refs/tags/m1`, `-X DELETE .../branches/main/protection`;
`git apply <missing file>` (fails closed).

Allow (exit 0): commit message with "push to main, patch 2, hooks, merges"
and an echo of the same; `gh api repos/o/r/rules/branches/main`, `.../rulesets`,
`.../git/refs/heads/main`, `-X GET .../branches`; `gh pr view 1 --json
mergeable`, `gh pr checks`, `gh pr comment --body-file`;
`git add crates/core/src/alias.rs`; `git log --grep="patch 2"`;
`git push origin <branch>` and `git push origin main` (intended: no push
inspection); `git config --get core.hooksPath`; commit messages mentioning
`--no-verify`, `LEFTHOOK=0` and the merge command as text, including
`-m "$(cat <<'EOF' ...)"` and `-F - <<'MSG'` heredocs; `cat`/`grep` of the
hook; `git diff` of guard-push.sh; `git apply --check`.

Note (not a regression): the hook is Bash-only (settings matcher `Bash`);
Write/Edit JSON fed to it exits 0. Edit/Write protection of
`.claude/settings.json`, `scripts/guard-push.sh` etc. comes from the
`permissions.deny` rules in settings.json, which this PR does not change.

## Mutation

Replaced the PR-merge `block(...)` call in the hook with `pass`.
`scripts/test-guards.sh` then failed (e.g. `gh -R o/r pr merge 2`,
`bash -c "gh pr merge 2"`, `gh pr -R o/r merge 2`: expected exit 2, got 0).
Restored from a copy; `git status` clean, suite back to 179 passed, 0 failed.

## Not run

Full gate (push hook and CI run it). Criterion 6 is the owner's.
