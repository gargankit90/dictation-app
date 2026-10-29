#!/usr/bin/env bash
# Self-contained verification suite for M0.2 acceptance criteria 1-4 and the
# PR #2 guard fixes (install-hooks.sh shared guard copy, linked worktrees with
# and without scripts/pre-push, no-op pushes, guard-git.sh path/subshell blocks,
# quoted-message blanking, and 20,000-char lexer cutoff timing).
# Read-only against the repository; all mutations happen inside a mktemp dir.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT

passed=0
failed=0
pass() { passed=$((passed + 1)); printf '  PASS: %s\n' "$1"; }
fail() { failed=$((failed + 1)); printf '  FAIL: %s\n' "$1" >&2; }

echo "=== 1. Criterion 1: Scope files exist (and docs/tech-debt.md is absent) ==="
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
  [ -e "$p" ] && pass "$p exists" || fail "$p missing"
done
[ ! -e "docs/tech-debt.md" ] && pass "docs/tech-debt.md absent (issues replace file)" || fail "docs/tech-debt.md unexpectedly present"
for s in scripts/guard-push.sh scripts/pre-push scripts/install-hooks.sh .claude/hooks/guard-git.sh; do
  [ -x "$s" ] && pass "$s executable (+x)" || fail "$s not executable"
done

echo "=== 2. Criterion 2 & Finding 2: scripts/guard-push.sh stdin cases ==="
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

echo "=== 3. Findings 1 & 2 + install-hooks.sh + Linked Worktrees ==="
WT_BASE="$TMP_ROOT/Dictation App"
mkdir -p "$WT_BASE"
git init -q --bare "$WT_BASE/remote.git"
git clone -q "$WT_BASE/remote.git" "$WT_BASE/main-wt" 2>/dev/null
(
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
  [ -x "$thooks/guard-push.sh" ] && [ -x "$thooks/pre-push" ]
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
  echo "report" > report.md && git add report.md && git commit -qm "test: report" 2>/dev/null

  git push --dry-run origin HEAD:refs/heads/m0.1-sim >/dev/null 2>&1
  git push origin HEAD:refs/heads/m0.1-sim >/dev/null 2>&1
  git push origin HEAD:refs/heads/m0.1-sim >/dev/null 2>&1 # no-op push
  git push --dry-run origin HEAD:refs/heads/m1-new >/dev/null 2>&1

  rc=0; out=$(git push --dry-run origin HEAD:main 2>&1) || rc=$?
  [[ "$rc" -eq 1 && "$out" == *"guard-push: refused: pushing to main"* ]]

  git commit -q --amend -m "amended" 2>/dev/null
  rc=0; out=$(git push --dry-run --force origin HEAD:refs/heads/m0.1-sim 2>&1) || rc=$?
  [[ "$rc" -eq 1 && "$out" == *"guard-push: refused: force push"* ]]

  rc=0; out=$(git push --dry-run origin --delete m0.1-sim 2>&1) || rc=$?
  [[ "$rc" -eq 1 && "$out" == *"guard-push: refused: deleting"* ]]

  git tag t1 HEAD~1
  git push -q origin t1
  git tag -f t1 HEAD >/dev/null
  rc=0; out=$(git push --dry-run --force origin t1 2>&1) || rc=$?
  [[ "$rc" -eq 1 && "$out" == *"guard-push: refused: moving existing tag"* ]]

  # Fail-closed when installed guard copy is missing
  mv "$thooks/guard-push.sh" "$WT_BASE/gp.bak"
  rc=0; out=$(git push --dry-run origin HEAD:refs/heads/m1-new 2>&1) || rc=$?
  mv "$WT_BASE/gp.bak" "$thooks/guard-push.sh"
  [[ "$rc" -eq 1 && "$out" == *"missing; push refused"* ]]

  # Linked worktree WITH scripts/pre-push: verify chaining, diff note, no-op skip, and early refuse
  cd "$WT_BASE/main-wt"
  git worktree add -q -b m0.2-linked "$WT_BASE/m02-linked" HEAD
  cd "$WT_BASE/m02-linked"
  printf '\necho "CHECKOUT_PRE_PUSH_RAN" >&2\n' >> scripts/pre-push
  printf '\n# local drift test\n' >> scripts/guard-push.sh
  git add scripts/pre-push scripts/guard-push.sh
  git commit -qm "instrument" 2>/dev/null

  out=$(git push origin m0.2-linked 2>&1)
  [[ "$out" == *"CHECKOUT_PRE_PUSH_RAN"* && "$out" == *"pre-push: note:"* ]]

  out_noop=$(git push origin m0.2-linked 2>&1)
  [[ "$out_noop" != *"CHECKOUT_PRE_PUSH_RAN"* ]]

  rc=0; out_refuse=$(git push --dry-run origin HEAD:main 2>&1) || rc=$?
  [[ "$rc" -eq 1 && "$out_refuse" == *"guard-push: refused: pushing to main"* && "$out_refuse" != *"CHECKOUT_PRE_PUSH_RAN"* ]]
) && pass "install-hooks.sh + old/new linked worktree push matrix" || fail "install-hooks.sh + linked worktree push matrix"

echo "=== 4. Criterion 3 & Findings 3 & 4: .claude/hooks/guard-git.sh ==="
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

# Finding 3: path-qualified git, subshells, continuations, quoted main
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

# Finding 4: quoted message/body/title blanking + anti-bypass
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

# 20,000-character cutoff behaviour and timing (< 500 ms budget)
python3 - <<'PY' && pass "guard-git 20,000-char cutoff behaviour and latency (<500 ms)" || fail "guard-git 20,000-char cutoff check"
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

echo "=== 5. Criterion 4: Milestone specs M0.3 through M-W.3 ==="
python3 - <<'PY' && pass "all 23 milestone specs (M0.3..M-W.3) valid" || fail "milestone specs validation"
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

echo ""
echo "test-m02-guards: $passed passed, $failed failed"
[ "$failed" -eq 0 ]
