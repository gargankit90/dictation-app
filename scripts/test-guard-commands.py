"""Exercise the real hook with inert commands, including former false positives."""
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import subprocess
import time

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
    "gh api graphql -f 'query=query { viewer { login } }'",
    "git status\npython3 - <<'PYCODE'\nprint(\"it's fine: lefthook\")\nPYCODE",
    "python3 -c 'unterminated", "cat <<EOF\ngh pr merge 1\nEOF",
    "gh api graphql --input /does/not/exist", "git apply /does/not/exist",
    "cp scripts/pre-push /tmp/copy", "git push --force origin main",
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
settings = json.loads((root / ".claude/settings.json").read_text())
old = json.loads(subprocess.check_output(["git", "show", "origin/main:.claude/settings.json"], cwd=root))
assert settings["permissions"]["deny"] == [rule for rule in old["permissions"]["deny"] if not rule.startswith("Bash(git push ")]
print(f"guard commands: {len(blocked) + len(allowed) + 6} checks passed in {time.monotonic() - start:.2f}s")
