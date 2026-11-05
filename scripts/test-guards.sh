#!/usr/bin/env bash
# Tests for the guards: the git push guard (scripts/guard-push.sh, through
# scripts/pre-push) and the Claude Code hook (.claude/hooks/guard-git.sh).
# Part of the quality gate. Needs git and jq; touches nothing outside a temp dir.
set -euo pipefail

# Found from this file, not from git: the repo variables below are cleared.
root=$(cd "$(dirname "$0")/.." && pwd -P)

# git's pre-push hook exports GIT_DIR (and from a linked worktree it is an
# absolute path), and an inherited GIT_DIR beats `git -C <scratch>`: the
# scratch-repo commands below would then rewrite the real repo. Clear every
# repo-local variable git knows about, once, before any git command runs.
# shellcheck disable=SC2046 # word splitting is wanted: one name per word
unset $(git rev-parse --local-env-vars)

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

tmp=$(cd "$(mktemp -d)" && pwd -P)
trap 'rm -rf "$tmp"' EXIT

g() { git -C "$tmp" -c user.name=test -c user.email=test@example.invalid "$@"; }
g init -q -b work
# Refuse to go on unless git really is working in the scratch repo.
if [ "$(g rev-parse --absolute-git-dir)" != "$tmp/.git" ]; then
  python3 "$root/scripts/test-check-rulesets.py"
echo "test-guards: scratch repo resolves to $(g rev-parse --absolute-git-dir), not $tmp/.git; stopping before it touches a real repo" >&2
  exit 1
fi
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
  (cd "$root" && printf '%s\n' "$2" | "$pre_push" origin git@example.invalid:r.git) 2>/dev/null || code=$?
  check "pre-push: $1" 1 "$code"
}
pre_push_refuses "push to main is refused" "refs/heads/x $zero refs/heads/main $zero"
pre_push_refuses "deleting a branch is refused" "(delete) $zero refs/heads/m1-x $zero"

ROOT=$root
TMP_ROOT=$tmp
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

  # Only this synthetic repository uses this runner. The application gate
  # and real hook files remain unchanged; CI need not install global lefthook.
  mkdir -p "$WT_BASE/bin"
  cat > "$WT_BASE/bin/lefthook" <<'RUNNER'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  install)
    hooks_dir="$(git rev-parse --git-common-dir)/hooks"
    mkdir -p "$hooks_dir"
    cat > "$hooks_dir/pre-commit" <<'HOOK'
#!/usr/bin/env bash
exec lefthook run pre-commit
HOOK
    chmod +x "$hooks_dir/pre-commit"
    ;;
  'run pre-commit') ;;
  'run pre-push-gate')
    echo SYNTHETIC_GATE_RAN >&2
    [ ! -e .fixture-gate-fails ]
    ;;
  *) echo "unexpected fixture lefthook arguments: $*" >&2; exit 1 ;;
esac
RUNNER
  chmod +x "$WT_BASE/bin/lefthook"
  export PATH="$WT_BASE/bin:$PATH"

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
  # Synthetic repository: exercise the installed hooks without invoking the
  # application gate (which does not exist in this scratch checkout).
  cat > lefthook.yml <<'YAML'
pre-commit:
  jobs:
    - name: fixture
      run: "true"
pre-push-gate:
  jobs:
    - name: fixture
      run: "true"
YAML
  git add .
  git commit -qm "m0.2"
  ./scripts/install-hooks.sh >/dev/null

  thooks="$(git rev-parse --path-format=absolute --git-common-dir)/hooks"
  [ -x "$thooks/guard-push.sh" ]
  [ -x "$thooks/pre-push" ]
  cmp -s "$thooks/guard-push.sh" scripts/guard-push.sh

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
  assert_rc_contains 0 0 "$out" "SYNTHETIC_GATE_RAN" "linked gate ran"

  # The unchanged pre-push script must propagate a failing gate, too.
  touch .fixture-gate-fails
  rc=0; out_fail=$(git push --dry-run origin HEAD:refs/heads/gate-failure 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out_fail" "SYNTHETIC_GATE_RAN" "linked gate failure refuses push"
  rm .fixture-gate-fails
  assert_rc_contains 0 0 "$out" "pre-push: note:" "linked drift note"

  out_noop=$(git push origin m0.2-linked 2>&1)
  assert_not_contains "$out_noop" "CHECKOUT_PRE_PUSH_RAN" "linked no-op skip"
  assert_not_contains "$out_noop" "SYNTHETIC_GATE_RAN" "linked no-op skips gate"

  rc=0; out_refuse=$(git push --dry-run origin HEAD:main 2>&1) || rc=$?
  assert_rc_contains "$rc" 1 "$out_refuse" "guard-push: refused: pushing to main" "linked main refuse"
  assert_not_contains "$out_refuse" "CHECKOUT_PRE_PUSH_RAN" "linked main early stop"
  assert_not_contains "$out_refuse" "SYNTHETIC_GATE_RAN" "linked main refuses before gate"
SEC3
  check "installed hooks + linked worktree push matrix" 0 0
else
  check "installed hooks + linked worktree push matrix" 0 1
fi


# --- Plain-command hook policy: table-driven, no inspected command executes. ---
python3 "$root/scripts/test-guard-commands.py"
python3 "$root/scripts/test-check-rulesets.py"
echo "test-guards: $passed push checks passed, $failed failed"
[ "$failed" -eq 0 ]
