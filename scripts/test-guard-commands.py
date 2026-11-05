"""Exercise the real hook with inert commands, including former false positives."""
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import subprocess
import time
import tempfile

root = Path(__file__).resolve().parent.parent
hook = root / ".claude/hooks/guard-git.sh"
blocked = [
    "gh pr merge 12", "gh -R o/r pr merge 12", "gh repo delete o/r",
    "gh repo edit o/r --visibility private", "gh release delete v1",
    "gh api -X PUT repos/o/r/pulls/12/merge",
    "gh api repos/o/r/merges -f base=main -f head=work",
    "gh api --method=PATCH repos/o/r -f visibility=private",
    "git status; gh pr merge 12", "git status\ngh pr merge 12",
]
for endpoint in ("rulesets/1", "branches/main", "tags/v1", "git/refs/heads/work", "settings"):
    blocked.append(f"gh api -X POST repos/o/r/{endpoint} -f value=x")
for mutation in ("mergePullRequest", "enablePullRequestAutoMerge", "enqueuePullRequest", "mergeBranch", "createCommitOnBranch", "createRef", "updateRefs", "deleteRef", "updateRepository", "deleteRepositoryRuleset", "createBranchProtectionRule"):
    blocked.append(f"gh api graphql -f 'query=mutation {{ {mutation}(input:{{}}) {{ clientMutationId }} }}'")
for path in (".claude/hooks/guard-git.sh", ".claude/settings.json", ".claude/settings.local.json", "scripts/guard-push.sh", "scripts/pre-push", "scripts/install-hooks.sh", "lefthook.yml", "lefthook-local.yml", ".git/hooks/pre-push"):
    blocked.append(f"echo x > {path}")
blocked += ["tee .claude/settings.json", "sed -i '' s/x/y/ scripts/pre-push", "cp /tmp/x scripts/pre-push", "rm .claude/hooks/guard-git.sh"]
allowed = [
    "git commit --no-verify -m test", "git commit -n -m test",
    "git config core.hooksPath /tmp/hooks", "LEFTHOOK=0 git commit -m test",
    "git -c core.hooksPath=/tmp/hooks status", "export LEFTHOOK_EXCLUDE=all",
    "echo x > /tmp/Vocet/settings.json", "cp /tmp/x src/settings.json",
    "grep '>' .claude/hooks/guard-git.sh", "git commit -m '>' .claude/settings.json",
    "cat .claude/settings.json", "grep lefthook lefthook.yml",
    "sed -n '1,20p' scripts/pre-push",
    "python3 -c 'from pathlib import Path; print(Path(\"lefthook.yml\").read_text())'",
    "git commit -m 'mention gh pr merge and mutation { deleteRef(input:{}) }'",
    "echo 'gh repo delete o/r'", "echo 'gh api -X DELETE repos/o/r/rulesets/1'",
    "gh api repos/o/r/rulesets/1", "gh api -X GET repos/o/r/rulesets/1",
    "gh api -X POST repos/o/r/issues -f title=test",
    "gh api repos/$GITHUB_REPOSITORY/issues -f title=test",
    "gh api repos/${GITHUB_REPOSITORY}/issues/12/comments -f body=test",
    "gh api graphql -f 'query=query { viewer { login } }'",
    "git status\npython3 - <<'PYCODE'\nprint(\"it's fine: lefthook\")\nPYCODE",
    "python3 -c 'unterminated", "cat <<EOF\ngh pr merge 1\nEOF",
    "gh api graphql --input /does/not/exist", "git apply /does/not/exist",
    "cp scripts/pre-push /tmp/copy", "git push --force origin main",
]
# Round 2: ordinary spellings and argument/data boundaries from B1, M1-M7.
for prefix in ("if true; then ", "for n in 1 2; do ", "(", "{ ", "x=$(", "! ", "VAR=x ", "env VAR=x ", "command ", "time ", "timeout 60 ", "sudo ", "xargs "):
    suffix = "; fi" if prefix.startswith("if ") else "; done" if prefix.startswith("for ") else ")" if prefix in ("(", "x=$(") else "; }" if prefix == "{ " else ""
    blocked += [prefix + "gh pr merge 12" + suffix, prefix + "rm scripts/pre-push" + suffix]
blocked += [
    "gh pr \\\n merge 12", "gh api \\\n -X PUT \\\n repos/o/r/pulls/12/merge",
    "gh api repos/o/r/pulls/12/merge \\\n -X PUT",
    "gh api graphql \\\n -f query='mutation { mergePullRequest(input:{}) { clientMutationId } }'",
    "git status # check first\ngh pr merge 12", "git status # first\nrm scripts/pre-push",
    'echo "x">.claude/settings.json', "printf 'x'>scripts/pre-push",
    "cp /tmp/x scripts/pre-push 2>/dev/null", "cp /tmp/x scripts/pre-push > /dev/null",
    "echo x > $PWD/scripts/pre-push", "echo x >| scripts/pre-push", "echo x 2>&1 > scripts/pre-push",
    "sed -Ei s/x/y/ scripts/pre-push", "perl -pi -e s/x/y/ scripts/pre-push",
    "rm -rf .claude", "rm -rf scripts", "gh pr --repo=o/r merge 12",
    "gh api -f 'body=-X GET' -X PUT repos/o/r/pulls/12/merge",
]
for verb in ("checkout --", "checkout origin/main --", "restore", "rm", "mv"):
    blocked.append(f"git {verb} scripts/pre-push" + (" /tmp/x" if verb == "mv" else ""))
for repo in ("o/r", "{owner}/{repo}", ":owner/:repo", "$OWNER/$REPO", "$GITHUB_REPOSITORY"):
    for path in ("pulls/12/merge", "rulesets/1", "branches/main", "tags/v1", "git/refs/heads/work", "settings"):
        for options in ("-X PUT {endpoint}", "{endpoint} --method=delete"):
            blocked.append("gh api " + options.format(endpoint=f"repos/{repo}/{path}"))
for option in ("-H 'Accept: application/json'", "--jq .name", "-t '{{.name}}'", "--input /not/read", "--hostname github.com"):
    blocked.append(f"gh api {option} -X PUT https://api.github.com/repos/o/r/pulls/12/merge?x=1#fragment")
    allowed.append(f"gh api {option} -X GET repos/o/r/rulesets/1")
allowed += [
    "gh api repos/o/r/pulls/12/comments -f 'body=see repos/o/r/rulesets/1 for details'",
    "gh api repos/o/r/pulls/12/comments -f 'body=this is on repos/o/r in general'",
    "gh api -X POST repos/o/r/issues/12/comments -f 'body=the endpoint repos/o/r/git/refs is wrong'",
    "gh api -f 'body=-X DELETE repos/o/r/rulesets/1' -X GET repos/o/r/rulesets/1",
    "gh api repos/o/r/issues -f 'body=mutation { deleteRef(input:{}) }'",
    "gh api graphql -f 'query=query { viewer { login } }' -f 'note=mutation { deleteRef(input:{}) }'",
    "git status # trailing comment\ncat scripts/pre-push",
    "git commit -m 'text # with hash and gh pr merge'",
    "grep '>' scripts/pre-push", "echo 'x > scripts/pre-push'", "cp scripts/pre-push /tmp/copy 2>/dev/null",
]
start = time.monotonic()
def check(case):
    expected, command = case
    result = subprocess.run([str(hook)], input=json.dumps({"tool_input": {"command": command}}), text=True, capture_output=True)
    assert result.returncode == expected, (command, expected, result.returncode, result.stderr)


# Independent inert commands can share no state; parallel startup keeps this
# suite short even on macOS, where launching Python dominates each check.
with ThreadPoolExecutor(max_workers=4) as pool:
    list(pool.map(check, [(expected, command) for expected, commands in ((2, blocked), (0, allowed)) for command in commands]))
for payload in ("invalid JSON", "[]", "{} {}", '{"tool_input":{"command":5}}', '{"tool_input":null}'):
    assert subprocess.run([str(hook)], input=payload, text=True, capture_output=True).returncode == 0
# -I must prevent a working-directory module from replacing the standard library.
with tempfile.TemporaryDirectory() as scratch:
    Path(scratch, "shlex.py").write_text("raise RuntimeError('local module executed')\n")
    result = subprocess.run([str(hook)], cwd=scratch, input=json.dumps({"tool_input": {"command": "gh pr merge 12"}}), text=True, capture_output=True)
    assert result.returncode == 2, result.stderr
    assert "Code Reviewer merges" in result.stderr and "owner changes guards" in result.stderr
settings = json.loads((root / ".claude/settings.json").read_text())
expected_deny = [
    'Bash(gh pr merge:*)',
    'Edit(**/.claude/settings.json)',
    'Edit(**/.claude/hooks/**)',
    'Write(**/.claude/settings.json)',
    'Write(**/.claude/hooks/**)',
    'Edit(**/scripts/guard-push.sh)',
    'Write(**/scripts/guard-push.sh)',
    'Edit(**/lefthook.yml)',
    'Write(**/lefthook.yml)',
    'Edit(**/lefthook-local.yml)',
    'Write(**/lefthook-local.yml)',
    'Edit(**/.lefthook-local.yml)',
    'Write(**/.lefthook-local.yml)',
    'Edit(**/.claude/settings.local.json)',
    'Write(**/.claude/settings.local.json)',
    'Edit(**/scripts/pre-push)',
    'Write(**/scripts/pre-push)',
    'Edit(**/scripts/install-hooks.sh)',
    'Write(**/scripts/install-hooks.sh)',
]
assert settings["permissions"]["deny"] == expected_deny
print(f"guard commands: {len(blocked) + len(allowed) + 8} checks passed in {time.monotonic() - start:.2f}s")
