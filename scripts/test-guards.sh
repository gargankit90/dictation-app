#!/usr/bin/env bash
# Tests for the guards: the git push guard (scripts/guard-push.sh, through
# scripts/pre-push) and the Claude Code hook (.claude/hooks/guard-git.sh).
# Part of the quality gate. Needs git and jq; touches nothing outside a temp dir.
set -euo pipefail

root=$(git rev-parse --show-toplevel)
guard="$root/scripts/guard-push.sh"
pre_push="$root/scripts/pre-push"
hook="$root/.claude/hooks/guard-git.sh"
zero=0000000000000000000000000000000000000000

passed=0
failed=0

check() { # name expected-exit actual-exit
  if [ "$2" = "$3" ]; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
    echo "FAIL: $1 (expected exit $2, got $3)" >&2
  fi
}

# --- push guard -----------------------------------------------------------

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

g() { git -C "$tmp" -c user.name=test -c user.email=test@example.invalid "$@"; }
g init -q -b work
g commit -q --allow-empty -m a
a=$(g rev-parse HEAD)
g commit -q --allow-empty -m b
b=$(g rev-parse HEAD)
g switch -q --orphan other
g commit -q --allow-empty -m c
c=$(g rev-parse HEAD)
unknown=1111111111111111111111111111111111111111

push() { # name expected-exit refs-line...
  local name=$1 expected=$2 code=0
  shift 2
  (cd "$tmp" && printf '%s\n' "$@" | "$guard" origin git@example.invalid:r.git) 2>/dev/null || code=$?
  check "guard-push: $name" "$expected" "$code"
}

push "new branch is allowed" 0 "refs/heads/work $b refs/heads/m1-x $zero"
push "fast-forward is allowed" 0 "refs/heads/work $b refs/heads/m1-x $a"
push "new tag is allowed" 0 "refs/tags/m1 $b refs/tags/m1 $zero"
push "push to main is refused" 1 "refs/heads/work $b refs/heads/main $a"
push "new main is refused" 1 "refs/heads/work $b refs/heads/main $zero"
push "deleting main is refused" 1 "(delete) $zero refs/heads/main $a"
push "deleting a branch is refused" 1 "(delete) $zero refs/heads/m1-x $a"
push "non-fast-forward is refused" 1 "refs/heads/other $c refs/heads/m1-x $b"
push "unknown remote commit is refused" 1 "refs/heads/work $b refs/heads/m1-x $unknown"
push "moving a tag is refused" 1 "refs/tags/m1 $b refs/tags/m1 $a"
push "one bad ref refuses the push" 1 \
  "refs/heads/work $b refs/heads/m1-x $zero" \
  "refs/heads/work $b refs/heads/main $a"

# scripts/pre-push must run the guard. Only refused pushes are tested: an
# allowed one would go on to run the full gate, which runs this script.
pre_push_refuses() { # name refs-line
  local code=0
  printf '%s\n' "$2" | "$pre_push" origin git@example.invalid:r.git 2>/dev/null || code=$?
  check "pre-push: $1" 1 "$code"
}
pre_push_refuses "push to main is refused" "refs/heads/x $zero refs/heads/main $zero"
pre_push_refuses "deleting a branch is refused" "(delete) $zero refs/heads/m1-x $zero"

# --- Claude Code hook -----------------------------------------------------

hook_case() { # expected-exit command
  local code=0
  jq -n --arg c "$2" '{tool_input: {command: $c}}' | "$hook" 2>/dev/null || code=$?
  check "guard-git: $2" "$1" "$code"
}
blocked() { hook_case 2 "$1"; }
allowed() { hook_case 0 "$1"; }

blocked "git push --force origin m1-x"
blocked "git push --force-with-lease origin m1-x"
blocked "git push -f origin m1-x"
blocked "git push origin +m1-x"
blocked "git push origin :m1-x"
blocked "git push --delete origin m1-x"
blocked "git push -d origin m1-x"
blocked "git push origin main"
blocked "git push origin HEAD:main"
blocked "git push --all origin"
blocked "git -C . push --force origin m1-x"
blocked "cd repo && git push -f"
blocked "git commit --no-verify -m x"
blocked "git commit -n -m x"
blocked "git commit -anm x"
blocked "git -c core.hooksPath=/dev/null commit -m x"
blocked "LEFTHOOK=0 git commit -m x"
blocked "lefthook uninstall"
blocked "rm .git/hooks/pre-push"
blocked "gh pr merge 1 --merge"
blocked "gh api -X PUT repos/o/r/pulls/1/merge"
blocked "gh api -X PATCH repos/o/r/git/refs/heads/main"
blocked "gh repo delete o/r --yes"
blocked "gh repo edit --visibility public"
blocked "echo x > scripts/guard-push.sh"
blocked "sed -i '' s/main/x/ scripts/pre-push"
blocked "rm lefthook.yml"
blocked "cp /tmp/x .claude/settings.json"
blocked "git checkout -- .claude/hooks/guard-git.sh"
blocked "python3 -c 'open(\"scripts/install-hooks.sh\",\"w\")'"

# Added with the hardened hook (M0.2 guard patches): patches, aliases and
# hookspath variants are blocked; message text no longer trips the checks.
blocked "git apply fix.patch"
blocked "git am 0001-fix.patch"
blocked "patch -p1 < fix.patch"
blocked "git config alias.p 'push --force'"
blocked "gh alias set m 'pr merge'"
blocked "git -c core.hookspath=/dev/null commit -m x"
allowed "git apply --check fix.patch"
allowed "git commit -m 'docs: explain -n and --no-verify are banned'"

# Fails closed on input it can't read.
raw_input_blocked() { # name input
  local code=0
  printf '%s' "$2" | "$hook" 2>/dev/null || code=$?
  check "guard-git: $1" 2 "$code"
}
raw_input_blocked "empty input is blocked" ""
raw_input_blocked "non-JSON input is blocked" "git push --force"

allowed "git push -u origin m0.3-quality-gate"
allowed "git push origin m1-maintenance"
allowed "git commit -m 'feat(core): add step'"
allowed "git status"
allowed "cat lefthook.yml"
allowed "gh pr create --base main --head m1-x"
allowed "gh pr comment 3 --body 'Tester: PASS'"
allowed "cargo clippy --workspace --all-targets -- -D warnings"

echo "test-guards: $passed passed, $failed failed"
[ "$failed" -eq 0 ]
