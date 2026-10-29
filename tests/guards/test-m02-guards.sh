#!/usr/bin/env bash
# Self-contained verification suite for M0.2 acceptance criteria 1-4 and the
# PR #2 guard fixes (install-hooks.sh shared guard copy, linked worktrees with
# and without scripts/pre-push, no-op pushes, guard-git.sh fail-closed input
# handling, quoted option values, case-insensitive hook bypass checks, gh global
# flags and GraphQL mutations, redirect-after-flag handling, expanded self-tamper
# checks, .claude/settings.json deny rules, and 20,000-char lexer cutoff timing).
# Read-only against the repository; all mutations happen inside a mktemp dir.
set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT"

TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT

passed=0
failed=0
pass() { passed=$((passed + 1)); printf '  PASS: %s\n' "$1"; }
fail() { failed=$((failed + 1)); printf '  FAIL: %s\n' "$1" >&2; }

echo "=== 1. Criterion 1 & Minor #19: Scope files and .claude/settings.json ==="
scope_paths=(
  "AGENTS.md"
  "CLAUDE.md"
  "docs/ENGINEERING.md"
  ".claude/agents/developer.md"
  "docs/agents/tester.md"
  "docs/agents/reviewer.md"
  "docs/review-checklist.md"
  "docs/milestones"
  "scripts/guard-push.sh"
  "scripts/pre-push"
  "scripts/install-hooks.sh"
  "lefthook.yml"
  ".claude/hooks/guard-git.sh"
  ".claude/settings.json"
  "test-reports"
  "reviews"
)
for p in "${scope_paths[@]}"; do
  if [ -e "$p" ]; then pass "$p exists"; else fail "$p missing"; fi
done
if [ ! -e "docs/tech-debt.md" ]; then
  pass "docs/tech-debt.md absent (issues replace file)"
else
  fail "docs/tech-debt.md unexpectedly present"
fi
for s in scripts/guard-push.sh scripts/pre-push scripts/install-hooks.sh .claude/hooks/guard-git.sh; do
  if [ -x "$s" ]; then pass "$s executable (+x)"; else fail "$s not executable"; fi
done

# Minor #19: .claude/settings.json is valid JSON and includes the 6 local-override deny rules
if jq -e 'type == "object" and (.permissions.deny | type == "array")' .claude/settings.json >/dev/null 2>&1; then
  pass ".claude/settings.json is valid JSON with permissions.deny array"
else
  fail ".claude/settings.json invalid JSON or missing permissions.deny"
fi
deny_rules=(
  "Edit(**/lefthook-local.yml)"
  "Write(**/lefthook-local.yml)"
  "Edit(**/.lefthook-local.yml)"
  "Write(**/.lefthook-local.yml)"
  "Edit(**/.claude/settings.local.json)"
  "Write(**/.claude/settings.local.json)"
)
for rule in "${deny_rules[@]}"; do
  if jq -e --arg r "$rule" '.permissions.deny | index($r) != null' .claude/settings.json >/dev/null 2>&1; then
    pass ".claude/settings.json denies $rule"
  else
    fail ".claude/settings.json missing deny rule $rule"
  fi
done

echo "=== 2. Criterion 2: scripts/guard-push.sh stdin cases ==="
T_C2="$TMP_ROOT/c2-repo"
git init -q -b work "$T_C2"
gc_c2() { git -C "$T_C2" -c user.name=t -c user.email=t@e.invalid commit -q --allow-empty -m "$1"; git -C "$T_C2" rev-parse HEAD; }
A_SHA=$(gc_c2 a)
B_SHA=$(gc_c2 b)
git -C "$T_C2" switch -q --orphan other
C_SHA=$(gc_c2 c)
ZERO="0000000000000000000000000000000000000000"
UNKNOWN="1111111111111111111111111111111111111111"

check_push_stdin() {
  local want="$1" input="$2" label="$3"
  local rc=0
  (cd "$T_C2" && printf "$input" | "$ROOT/scripts/guard-push.sh" origin https://example.invalid/r.git) >/dev/null 2>&1 || rc=$?
  if [ "$rc" -eq "$want" ]; then
    pass "guard-push stdin [$label] -> exit $rc"
  else
    fail "guard-push stdin [$label] -> want $want, got $rc"
  fi
}

check_push_stdin 0 ''                                                  "empty stdin"
check_push_stdin 0 '\n'                                                "single blank line (no-op via scripts/pre-push)"
check_push_stdin 0 '\n\n'                                              "multiple blank lines"
check_push_stdin 1 "\nrefs/heads/work $B_SHA refs/heads/main $A_SHA\n" "blank line then push to main"
check_push_stdin 1 "refs/heads/work $B_SHA refs/heads/main $A_SHA"     "push to main without trailing newline"
check_push_stdin 1 "refs/heads/work $B_SHA refs/heads/main $A_SHA\n"   "push to main"
check_push_stdin 1 "refs/heads/other $C_SHA refs/heads/m1-x $B_SHA\n"  "non-fast-forward"
check_push_stdin 1 "(delete) $ZERO refs/heads/m1-x $A_SHA\n"           "delete branch"
check_push_stdin 1 "refs/tags/m1 $B_SHA refs/tags/m1 $A_SHA\n"         "move tag"
check_push_stdin 1 "refs/heads/work $B_SHA refs/heads/m1-x $UNKNOWN\n" "unknown remote sha"
check_push_stdin 1 "refs/heads/only-one-token\n"                       "malformed line"
check_push_stdin 0 "refs/heads/work $B_SHA refs/heads/m1-x $ZERO\n"    "new branch"
check_push_stdin 0 "refs/tags/m1 $B_SHA refs/tags/m1 $ZERO\n"          "new tag"
check_push_stdin 0 "refs/heads/work $B_SHA refs/heads/m1-x $A_SHA\n"   "fast-forward"

echo "=== 3. install-hooks.sh + Linked Worktrees (child bash -euo pipefail) ==="
WT_BASE="$TMP_ROOT/Dictation App"
mkdir -p "$WT_BASE"
git init -q --bare "$WT_BASE/remote.git"
git clone -q "$WT_BASE/remote.git" "$WT_BASE/main-wt" 2>/dev/null
# Run in a separate `bash -euo pipefail` process (not a `( ... ) &&` subshell,
# which would disable `set -e` across intermediate checks). Also use explicit
# `die` helpers instead of bare `[[ a && b ]]`, because macOS `/bin/bash` 3.2
# suppresses `errexit` on `&&` inside `[[ ... ]]`.
if ROOT="$ROOT" WT_BASE="$WT_BASE" bash -euo pipefail <<'SEC3'; then
  die() { echo "sec3 fail: $*" >&2; exit 1; }
  assert_rc_contains() {
    local got_rc="$1" want_rc="$2" text="$3" needle="$4" label="$5"
    [ "$got_rc" -eq "$want_rc" ] || die "$label: want rc $want_rc, got $got_rc ($text)"
    case "$text" in
      *"$needle"*) ;;
      *) die "$label: missing '$needle' in: $text" ;;
    esac
  }
  assert_not_contains() {
    local text="$1" needle="$2" label="$3"
    case "$text" in
      *"$needle"*) die "$label: unexpected '$needle' in: $text" ;;
    esac
  }

  cd "$WT_BASE/main-wt"
  git config user.email "tester@example.com"
  git config user.name "Tester"
  git checkout -qb m0.1-sim
  echo "scaffold" > README.md
  git add README.md
  git commit -qm "m0.1"
  git push -q -u origin m0.1-sim
  git push -q origin HEAD:refs/heads/main

  git checkout -qb m0.2-sim
  mkdir -p scripts
  cp "$ROOT"/scripts/{guard-push.sh,pre-push,install-hooks.sh} scripts/
  cp "$ROOT/lefthook.yml" .
  git add .
  git commit -qm "m0.2"
  ./scripts/install-hooks.sh >/dev/null

  thooks="$(git rev-parse --path-format=absolute --git-common-dir)/hooks"
  [ -x "$thooks/guard-push.sh" ]
  [ -x "$thooks/pre-push" ]
  cmp -s "$thooks/guard-push.sh" scripts/guard-push.sh

  # Verify lefthook install / lefthook run leaves pre-push and guard-push.sh intact
  if command -v lefthook >/dev/null 2>&1; then
    cat > lefthook-local.yml <<'YAML'
pre-commit:
  jobs:
    - name: noop
      run: "true"
pre-push-gate:
  jobs:
    - name: noop
      run: "true"
YAML
    cp "$thooks/pre-push" "$WT_BASE/pre-push.snap"
    cp "$thooks/guard-push.sh" "$WT_BASE/guard-push.snap"
    lefthook run pre-push-gate >/dev/null
    lefthook install >/dev/null
    cmp -s "$thooks/pre-push" "$WT_BASE/pre-push.snap"
    cmp -s "$thooks/guard-push.sh" "$WT_BASE/guard-push.snap"
    rm -f lefthook-local.yml
  fi

  # Linked worktree WITHOUT scripts/ (simulating pre-M0.2 branch)
  git worktree add -q --detach "$WT_BASE/m01-wt" origin/m0.1-sim
  cd "$WT_BASE/m01-wt"
  echo "report" > report.md
  git add report.md
  git commit -qm "test: report" 2>/dev/null

  git push --dry-run origin HEAD:refs/heads/m0.1-sim >/dev/null 2>&1
  git push origin HEAD:refs/heads/m0.1-sim >/dev/null 2>&1
  git push origin HEAD:refs/heads/m0.1-sim >/dev/null 2>&1 # no-op push
  git push --dry-run origin HEAD:refs/heads/m1-new >/dev/null 2>&1

  rc=0; out=$(git push --dry-run origin HEAD:main 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out" "guard-push: refused: pushing to main" "m01-wt push to main"

  git commit -q --amend -m "amended" 2>/dev/null
  rc=0; out=$(git push --dry-run --force origin HEAD:refs/heads/m0.1-sim 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out" "guard-push: refused: force push" "m01-wt force push"

  rc=0; out=$(git push --dry-run origin --delete m0.1-sim 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out" "guard-push: refused: deleting" "m01-wt branch delete"

  git tag t1 HEAD~1
  git push -q origin t1
  git tag -f t1 HEAD >/dev/null
  rc=0; out=$(git push --dry-run --force origin t1 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out" "guard-push: refused: moving existing tag" "m01-wt tag move"

  # Fail-closed when installed guard copy is missing
  mv "$thooks/guard-push.sh" "$WT_BASE/gp.bak"
  rc=0; out=$(git push --dry-run origin HEAD:refs/heads/m1-new 2>&1) || rc=$?
  mv "$WT_BASE/gp.bak" "$thooks/guard-push.sh"
  assert_rc_contains "$rc" 1 "$out" "missing; push refused" "missing guard-push.sh fail-closed"

  # Linked worktree WITH scripts/pre-push: verify chaining, diff note, no-op skip, and early refuse
  cd "$WT_BASE/main-wt"
  git worktree add -q -b m0.2-linked "$WT_BASE/m02-linked" HEAD
  cd "$WT_BASE/m02-linked"
  printf '\necho "CHECKOUT_PRE_PUSH_RAN" >&2\n' >> scripts/pre-push
  printf '\n# local drift test\n' >> scripts/guard-push.sh
  git add scripts/pre-push scripts/guard-push.sh
  git commit -qm "instrument" 2>/dev/null

  out=$(git push origin m0.2-linked 2>&1)
  assert_rc_contains 0 0 "$out" "CHECKOUT_PRE_PUSH_RAN" "linked pre-push ran"
  assert_rc_contains 0 0 "$out" "pre-push: note:" "linked drift note"

  out_noop=$(git push origin m0.2-linked 2>&1)
  assert_not_contains "$out_noop" "CHECKOUT_PRE_PUSH_RAN" "linked no-op skip"

  rc=0; out_refuse=$(git push --dry-run origin HEAD:main 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out_refuse" "guard-push: refused: pushing to main" "linked main refuse"
  assert_not_contains "$out_refuse" "CHECKOUT_PRE_PUSH_RAN" "linked main early stop"
SEC3
  pass "install-hooks.sh + old/new linked worktree push matrix"
else
  fail "install-hooks.sh + linked worktree push matrix"
fi

echo "=== 4. Criterion 3 & Review Round 1 Regressions: .claude/hooks/guard-git.sh ==="
check_git_hook() {
  local want="$1" cmd="$2"
  local rc=0
  jq -n --arg c "$cmd" '{tool_input:{command:$c}}' | "$ROOT/.claude/hooks/guard-git.sh" >/dev/null 2>&1 || rc=$?
  if [ "$rc" -eq "$want" ]; then
    pass "guard-git [exit $rc] $cmd"
  else
    fail "guard-git [want $want, got $rc] $cmd"
  fi
}

check_git_hook_raw() {
  local want="$1" raw_in="$2" label="$3"
  local rc=0
  printf '%s' "$raw_in" | "$ROOT/.claude/hooks/guard-git.sh" >/dev/null 2>&1 || rc=$?
  if [ "$rc" -eq "$want" ]; then
    pass "guard-git raw [$label] -> exit $rc"
  else
    fail "guard-git raw [$label] -> want $want, got $rc"
  fi
}

# Criterion 3 baseline
check_git_hook 2 "git push --force"
check_git_hook 2 "git push -f"
check_git_hook 2 "git push origin main"
check_git_hook 2 "git commit --no-verify"
check_git_hook 2 "git commit -n"
check_git_hook 2 "git config core.hooksPath x"
check_git_hook 2 "LEFTHOOK=0 git push"
check_git_hook 2 "gh pr merge 1"
check_git_hook 0 "git push origin m0.2-agent-docs"
check_git_hook 0 "git commit -m msg"
check_git_hook 0 "gh pr create"

# Run-1 Finding 3: path-qualified git, subshells, continuations, quoted main
check_git_hook 2 "/usr/bin/git push --force"
check_git_hook 2 "/opt/homebrew/bin/git push -f"
check_git_hook 2 "(git push --force)"
check_git_hook 2 '$(git push -f)'
check_git_hook 2 '`git push -f`'
check_git_hook 2 "bash -c 'git push --force'"
check_git_hook 2 'sh -c "git push origin main"'
check_git_hook 2 "(git push origin main)"
check_git_hook 2 "git push origin 'main'"
check_git_hook 2 "git push origin main;"
check_git_hook 2 '\git push --force'
check_git_hook 2 '"git" push --force'
check_git_hook 2 $'git push \\\n  --force origin x'
check_git_hook 2 "/usr/bin/git commit -n -m msg"
check_git_hook 0 "git push origin main:m1-feature"

# Run-1 Finding 4: quoted message/body/title blanking + anti-bypass
check_git_hook 0 'git commit -m "fix -n flag"'
check_git_hook 0 'git commit -m "mention main" && git push origin m0.2-agent-docs'
check_git_hook 0 'git commit -m "mention main branch" && git push origin m0.2-agent-docs'
check_git_hook 0 'gh pr comment 2 --body "scripts/guard-push.sh -> ok"'
check_git_hook 0 'git commit -am "notes on --force and --no-verify and main"'
check_git_hook 0 'git commit --message="scripts/guard-push.sh > ok"'
check_git_hook 0 'gh pr create --title "guard-push.sh > fix" --body "blocks --no-verify"'
check_git_hook 2 'git commit -m "$(git push --force)"'
check_git_hook 2 "git commit -m 'x' --no-verify"
check_git_hook 2 'git commit -nm "msg"'
check_git_hook 2 'git commit -anm x'
check_git_hook 2 'git checkout -m scripts/pre-push'
check_git_hook 2 'gh pr merge 1 -b "x"'
check_git_hook 2 'gh pr comment 2 --body "x" && gh pr merge 2'
check_git_hook 2 'git commit -m "x"; git push origin main'
check_git_hook 2 'bash -c "echo -m '"'"'"; git push --force; echo "'"'"'"'
check_git_hook 2 $'git commit -m $\'\\\'; git push --force #\''

# Review Major 1: fail closed on missing tools, empty/malformed/non-object JSON, non-string command, or grep error
check_git_hook_raw 2 ''                                           "empty stdin"
check_git_hook_raw 2 '   '                                        "whitespace-only stdin"
check_git_hook_raw 2 '{bad'                                       "malformed JSON"
check_git_hook_raw 2 '[1,2]'                                      "JSON array root"
check_git_hook_raw 2 '"git push --force"'                         "JSON string root"
check_git_hook_raw 2 '{} {"tool_input":{"command":"git push -f"}}' "multiple JSON values"
check_git_hook_raw 2 '{"tool_input":{"command":5}}'               "non-string command"
check_git_hook_raw 2 '{"tool_input":"x"}'                         "non-object tool_input"
check_git_hook_raw 0 '{"tool_input":{"command":""}}'              "empty string command"
check_git_hook_raw 0 '{"tool_input":{"command":null}}'            "null command"
check_git_hook_raw 0 '{}'                                         "missing tool_input"

rc_nojq=0
printf '%s' '{"tool_input":{"command":"git status"}}' | PATH=/bin /bin/bash "$ROOT/.claude/hooks/guard-git.sh" >/dev/null 2>&1 || rc_nojq=$?
if [ "$rc_nojq" -eq 2 ]; then pass "guard-git fails closed when jq missing (exit 2)"; else fail "guard-git no-jq want 2, got $rc_nojq"; fi

mkdir -p "$TMP_ROOT/bad-grep"
printf '#!/bin/sh\nexit 2\n' > "$TMP_ROOT/bad-grep/grep"
chmod +x "$TMP_ROOT/bad-grep/grep"
rc_badgrep=0
printf '%s' '{"tool_input":{"command":"git status"}}' | PATH="$TMP_ROOT/bad-grep:$PATH" "$ROOT/.claude/hooks/guard-git.sh" >/dev/null 2>&1 || rc_badgrep=$?
if [ "$rc_badgrep" -eq 2 ]; then pass "guard-git fails closed when grep exits >=2 (exit 2)"; else fail "guard-git bad-grep want 2, got $rc_badgrep"; fi

# Review Major 2: git options with quoted/escaped spaces or embedded newlines before push
check_git_hook 2 'git -C "/tmp/My Repo/dictation-app" push --force'
check_git_hook 2 'git -C "/tmp/My Repo/dictation-app" push origin main'
check_git_hook 2 'git -c user.name="A B" push origin main'
check_git_hook 2 'git --git-dir="/x y/.git" push --force'
check_git_hook 2 "git --git-dir='/x y/.git' push --force"
check_git_hook 2 'git --git-dir "/x y/.git" --work-tree "/x y" push origin main'
check_git_hook 2 'git -C /a\ b/c push --force'
check_git_hook 2 'git -c user.name="A \" B" push --force'
check_git_hook 2 $'git -c user.name="A\nB" push --force'
check_git_hook 0 'git -C "/tmp/My Repo/dictation-app" push origin m0.2-agent-docs'
check_git_hook 0 'git -C "/tmp/My Repo/dictation-app" status'
check_git_hook 0 'git push origin HEAD:m0.2-agent-docs'

# Review Major 3: case-insensitive core.hooksPath, LEFTHOOK=false/LEFTHOOK_*, lefthook-local, .GIT/HOOKS, git-path, git alias
check_git_hook 2 'git config core.hookspath /tmp/empty'
check_git_hook 2 'git config --local Core.HooksPath /dev/null'
check_git_hook 2 'git -c core.hookspath=/tmp/empty commit -m "x"'
check_git_hook 2 'git -c CORE.HOOKSPATH=/tmp/empty push origin m0.2-agent-docs'
check_git_hook 2 "printf '[core]\\n\\thooksPath = /dev/null\\n' >> .git/config"
check_git_hook 2 'LEFTHOOK=false git commit -m x'
check_git_hook 2 "LEFTHOOK='0' git commit -m x"
check_git_hook 2 'export LEFTHOOK=FALSE; git commit -m x'
check_git_hook 2 'LEFTHOOK_EXCLUDE=gate git push origin m1'
check_git_hook 2 'lefthook uninstall'
check_git_hook 2 'echo "pre-push: {skip: true}" > lefthook-local.yml'
check_git_hook 2 'cp x .lefthook-local.yml'
check_git_hook 2 'rm .GIT/HOOKS/pre-push'
check_git_hook 2 'rm "$(git rev-parse --git-path hooks)/pre-push"'
check_git_hook 2 'git -c alias.p=push p --force'
check_git_hook 2 "git config --global alias.p 'push --force'"

# Review Major 4: gh global flags, path-qualified gh, GraphQL merge/ref mutations, query file, gh alias
check_git_hook 2 'gh -R gargankit90/dictation-app pr merge 2 --merge'
check_git_hook 2 'gh --repo gargankit90/dictation-app pr merge 2'
check_git_hook 2 'gh --repo=o/r pr merge 2'
check_git_hook 2 'gh pr -R o/r merge 2'
check_git_hook 2 '/opt/homebrew/bin/gh -R o/r pr merge 2'
check_git_hook 2 "bash -c 'gh -R o/r pr merge 2'"
for gql_mut in mergePullRequest enablePullRequestAutoMerge createRef updateRef deleteRef updateRefs createCommitOnBranch mergeBranch enqueuePullRequest; do
  check_git_hook 2 "gh api graphql -f query='mutation { $gql_mut(input:{}) { clientMutationId } }'"
done
check_git_hook 2 $'gh api graphql -f query=\'\nmutation($id: ID!) {\n  mergePullRequest(input: {pullRequestId: $id}) { clientMutationId }\n}\' -f id=x'
check_git_hook 2 'gh api graphql -F query=@merge.graphql'
check_git_hook 2 'gh api graphql --input q.json'
check_git_hook 2 'gh -R o/r api repos/o/r/pulls/2/merge'
check_git_hook 2 "gh alias set m 'pr merge'"
check_git_hook 2 'gh -R o/r repo edit --default-branch x'
check_git_hook 0 'gh -R gargankit90/dictation-app pr view 2'
check_git_hook 0 'gh --repo gargankit90/dictation-app pr comment 2 --body-file /tmp/c.md'
check_git_hook 0 'gh -R o/r pr comment 2 --body "please do not gh pr merge yet"'
check_git_hook 0 "gh api graphql -f query='query { repository(owner:\"o\", name:\"r\") { pullRequest(number: 2) { mergeable } } }'"
check_git_hook 0 'gh alias list'

# Minor #16: < and > redirections right after message flag clear pending flag
check_git_hook 2 'git commit -m > scripts/guard-push.sh'
check_git_hook 2 'git commit -m>scripts/guard-push.sh'
check_git_hook 2 'gh pr comment 2 --body > .claude/hooks/guard-git.sh'
check_git_hook 0 'git commit -m "x > y"'
check_git_hook 0 'git commit -m "notes on scripts/guard-push.sh > ok"'

# Minor #18: expanded guard self-tamper check (subshells, relative paths, .claude/hooks/*, patches, 2>&1 / >/dev/null)
check_git_hook 2 '(rm scripts/guard-push.sh)'
check_git_hook 2 '(cp /dev/null scripts/pre-push)'
check_git_hook 2 'cp /dev/null .claude/hooks/other.sh'
check_git_hook 2 'rm -rf .claude/hooks'
check_git_hook 2 'rm -rf .claude'
check_git_hook 2 'rm -rf scripts'
check_git_hook 2 'cd scripts && cp /dev/null pre-push'
check_git_hook 2 'cd .claude && cp x settings.local.json'
check_git_hook 2 'patch -p1 < fix.patch'
check_git_hook 2 "bash -c 'patch -p1 < x.diff'"
check_git_hook 2 'git apply guard-fixes-2.patch'
check_git_hook 2 'git apply --check x.patch && git apply x.patch'
check_git_hook 2 'git am 0001-x.patch'
check_git_hook 2 'sed -E -i "" s/x/y/ scripts/guard-push.sh'
check_git_hook 2 'unlink scripts/pre-push'
check_git_hook 2 'echo x 2>&1 > scripts/guard-push.sh'
check_git_hook 2 'echo x >/dev/null > scripts/install-hooks.sh'
check_git_hook 0 'git apply --check guard-fixes-2.patch'
check_git_hook 0 'patch --dry-run -p1 < x.patch'
check_git_hook 0 'git stash apply'
check_git_hook 0 'cat scripts/guard-push.sh 2>&1'
check_git_hook 0 'scripts/install-hooks.sh 2>&1 | tail -3'
check_git_hook 0 'lefthook run pre-push-gate > /tmp/gate.log'

# 20,000-character cutoff behaviour and timing (< 500 ms budget)
if python3 - <<'PY'; then
import json, subprocess, sys, time

hook = ".claude/hooks/guard-git.sh"
prefix = 'gh pr comment 2 --body "scripts/guard-push.sh -> ok '
suffix = '"'

for length, want_rc in [(20000, 0), (20001, 2)]:
    cmd = prefix + ("a" * (length - len(prefix) - len(suffix))) + suffix
    assert len(cmd) == length
    payload = json.dumps({"tool_input": {"command": cmd}}).encode()
    t0 = time.perf_counter()
    res = subprocess.run([hook], input=payload, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    elapsed_ms = (time.perf_counter() - t0) * 1000.0
    if res.returncode != want_rc or elapsed_ms > 500.0:
        print(f"len={length} rc={res.returncode} (want {want_rc}) elapsed={elapsed_ms:.1f}ms", file=sys.stderr)
        sys.exit(1)
PY
  pass "guard-git 20,000-char cutoff behaviour and latency (<500 ms)"
else
  fail "guard-git 20,000-char cutoff check"
fi

echo "=== 5. Criterion 4: Milestone specs M0.3 through M-W.3 ==="
if python3 - <<'PY'; then
import os, re, sys

milestones = [
    "M0.3",
    "M1.1", "M1.2", "M1.3", "M1.4", "M1.5",
    "M2.1", "M2.2",
    "M3.1", "M3.2", "M3.3", "M3.4",
    "M4.1", "M4.2", "M4.3", "M4.4",
    "M5.1", "M5.2", "M5.3", "M5.4",
    "M-W.1", "M-W.2", "M-W.3",
]

for m in milestones:
    path = os.path.join("docs", "milestones", f"{m}.md")
    if not os.path.isfile(path):
        sys.exit(1)
    with open(path, encoding="utf-8") as fh:
        text = fh.read()
    for sec in ("Goal", "Scope", "Out of scope", "Acceptance criteria"):
        if not re.search(rf"^## {sec}\s*$", text, re.M):
            sys.exit(1)
    ac_text = text.split("## Acceptance criteria", 1)[1]
    items = re.findall(r"^(\d+)\.\s+(.*)$", ac_text, re.M)
    nums = [int(n) for n, _ in items]
    if not nums or nums != list(range(1, len(nums) + 1)):
        sys.exit(1)
    if any(not re.match(r"^`\[(auto|manual)\]`\s+\S", b) for _, b in items):
        sys.exit(1)
PY
  pass "all 23 milestone specs (M0.3..M-W.3) valid"
else
  fail "milestone specs validation"
fi

echo ""
echo "test-m02-guards: $passed passed, $failed failed"
[ "$failed" -eq 0 ]
