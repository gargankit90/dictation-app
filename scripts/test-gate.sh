#!/usr/bin/env bash
# Self-test of the gate's successful-run receipt and plain-prose shortcut
# (G2; scripts/gate.sh, scripts/gate-lib.sh). The expensive checks are stubbed:
# gate_run_full and gate_run_prose only append to a log, so no cargo, npm or
# network. Part of the quality gate.
set -euo pipefail

# A fresh environment for every gate_main: no CI variables from the caller.
unset CI GITHUB_ACTIONS GATE_RECEIPT_TTL
# Git's pre-push hook runs the gate with GIT_DIR and friends set. Left in, they
# point every fixture command at the real repository (this once moved the
# checkout's branch). Drop all of them; GIT_EXEC_PATH (git's own libexec) stays.
# GIT_TEMPLATE_DIR goes too: a caller's template would copy its hooks into every
# fixture repo. Fixtures get an empty template and, as a second lock, hooks
# switched off through the environment (which beats any repo config).
while IFS= read -r v; do unset "$v"; done < <(env | sed -n 's/^\(GIT_[A-Z_]*\)=.*/\1/p' | grep -v -e '^GIT_EXEC_PATH$' || true)

root=$(git rev-parse --show-toplevel)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/empty-template"
export GIT_TEMPLATE_DIR=$tmp/empty-template
export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null
fails=0
ok() { printf 'ok   %s\n' "$1"; }
bad() { printf 'FAIL %s\n' "$1" >&2; fails=$((fails + 1)); }
expect() { # description, command...
  local d=$1
  shift
  if "$@"; then ok "$d"; else bad "$d"; fi
}

export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.com GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.com

# A bare "origin" and a clone with main pushed, then a feature branch.
make_repo() { # name
  local r=$tmp/$1
  git init -q --bare -b main "$r.git"
  git init -q -b main "$r"
  # Refuse to touch anything but the fixture, whatever the environment says.
  [ "$(git -C "$r" rev-parse --show-toplevel)" = "$(cd "$r" && pwd -P)" ] || {
    echo "test-gate: fixture escaped to $(git -C "$r" rev-parse --show-toplevel)" >&2
    exit 1
  }
  mkdir -p "$r/scripts" "$r/docs/history" "$r/node_modules" "$r/.github"
  cp "$root/scripts/gate.sh" "$root/scripts/gate-lib.sh" "$r/scripts/"
  printf 'node_modules/\ngatefile.txt\n' >"$r/.gitignore"
  echo '{}' >"$r/node_modules/.package-lock.json"
  mkdir -p "$r/node_modules/.bin" "$r/node_modules/prettier"
  for t in prettier eslint tsc vitest; do printf '#!/bin/sh\n' >"$r/node_modules/.bin/$t"; chmod +x "$r/node_modules/.bin/$t"; done
  echo '{}' >"$r/node_modules/prettier/package.json"
  echo one >"$r/gatefile.txt"
  echo '# r' >"$r/README.md"
  echo code >"$r/code.rs"
  git -C "$r" add -A
  git -C "$r" commit -q -m init
  git -C "$r" remote add origin "$r.git"
  git -C "$r" push -q origin main
  git -C "$r" fetch -q origin
  git -C "$r" switch -q -c feature
  echo "$r"
}

# Run gate_main in repo $1 with the checks stubbed. Output in $tmp/out, the
# stub calls in $tmp/calls (one word per call). $2... are gate.sh arguments.
run_gate() {
  local r=$1
  shift
  : >"$tmp/calls"
  (
    cd "$r"
    # shellcheck disable=SC1091
    . "$root/scripts/gate.sh"
    gate_run_full() { echo full >>"$tmp/calls"; [ -z "${STUB_FAIL:-}" ] || exit 1; }
    gate_run_prose() { echo prose >>"$tmp/calls"; }
    gate_script_files() { echo gatefile.txt; }
    gate_tool_versions() { echo stub-tools; } # the real one runs ten tools
    gate_main "$@"
  ) >"$tmp/out" 2>&1
}
calls() { tr '\n' ' ' <"$tmp/calls"; }
ran_full() { [ "$(calls)" = "full " ]; }
ran_prose() { [ "$(calls)" = "prose " ]; }
ran_none() { [ -z "$(calls)" ]; }
out_has() { grep -qF -- "$1" "$tmp/out"; }
receipt() { echo "$1/.git/gate-receipt"; }
commit_file() { # repo path content
  mkdir -p "$(dirname "$1/$2")"
  printf '%s\n' "$3" >"$1/$2"
  git -C "$1" add -A
  git -C "$1" commit -q -m "change $2"
}

# Isolation probe: build one fixture repo and commit in it. Used by the hostile-
# environment test below, which runs this script again in a poisoned caller env.
if [ "${1:-}" = --probe ]; then
  r=$(make_repo probe)
  commit_file "$r" a.txt a
  exit 0
fi

# The caller's environment is hostile: a template dir whose hooks write a
# sentinel, and GIT_DIR pointing at a decoy repo. Neither may have any effect.
hostile=$tmp/hostile
mkdir -p "$hostile/template/hooks" "$hostile/decoy"
for hook in pre-commit post-commit post-checkout pre-push reference-transaction; do
  printf '#!/bin/sh\ntouch "%s/sentinel-%s"\n' "$hostile" "$hook" >"$hostile/template/hooks/$hook"
  chmod +x "$hostile/template/hooks/$hook"
done
(cd "$hostile/decoy" && git init -q -b main . && git -c core.hooksPath=/dev/null commit -q --allow-empty -m decoy)
decoy_before=$(git -C "$hostile/decoy" for-each-ref; git -C "$hostile/decoy" status --short)
GIT_TEMPLATE_DIR=$hostile/template GIT_DIR=$hostile/decoy/.git GIT_WORK_TREE=$hostile/decoy \
  GIT_CONFIG_COUNT=0 bash "$root/scripts/test-gate.sh" --probe >/dev/null 2>&1 || true
expect "a caller's template-dir hooks never run in fixtures" test -z "$(ls "$hostile" | grep sentinel || true)"
expect "a caller's GIT_DIR is never touched by fixtures" test "$decoy_before" = "$(git -C "$hostile/decoy" for-each-ref; git -C "$hostile/decoy" status --short)"
git config --global core.hooksPath >/dev/null 2>&1 || true

# ---- receipts ----
r=$(make_repo rcpt)
commit_file "$r" code.rs "code 2"
run_gate "$r"
expect "first run on a clean tree runs the full gate" ran_full
expect "...and writes a receipt" test -f "$(receipt "$r")"
expect "...reporting it" out_has "receipt written"
run_gate "$r"
expect "second run on identical input is skipped" ran_none
expect "...with a clear message" out_has "gate: skipped"
run_gate "$r" --full
expect "--full ignores the receipt" ran_full
run_gate "$r"
expect "a --full pass leaves a usable receipt" ran_none

# Each invalidation, then a rerun that restores the key.
inval() { # description, mutate-cmd (run in $r), restore-cmd
  run_gate "$r" --full # fresh receipt
  (cd "$r" && eval "$2")
  run_gate "$r"
  if ran_full || { ! git -C "$r" diff --quiet || [ -n "$(git -C "$r" status --porcelain)" ]; }; then ok "$1"; else bad "$1"; fi
  (cd "$r" && eval "$3")
}
inval "a new commit (different tree) invalidates" \
  'echo x >>code.rs; git add -A; git commit -qm c' 'true'
inval "a changed gate script invalidates" 'echo two >gatefile.txt' 'true'
inval "a changed installed-package state invalidates" 'echo "{\"a\":1}" >node_modules/.package-lock.json' 'true'

# Mutations of functions and env need the stubbed run to see them.
run_gate_with() { # shell snippet to run after sourcing, then gate_main
  local r=$1 snippet=$2
  : >"$tmp/calls"
  (
    cd "$r"
    . "$root/scripts/gate.sh"
    gate_run_full() { echo full >>"$tmp/calls"; }
    gate_run_prose() { echo prose >>"$tmp/calls"; }
    gate_script_files() { echo gatefile.txt; }
    gate_tool_versions() { echo stub-tools; }
    eval "$snippet"
    gate_main
  ) >"$tmp/out" 2>&1
}
run_gate "$r" --full
run_gate_with "$r" 'gate_platform() { echo Other x86_64; }'
expect "a different OS or architecture invalidates" ran_full
run_gate "$r" --full
run_gate_with "$r" 'gate_tool_versions() { echo "rustc: 9.9"; }'
expect "a different tool version invalidates" ran_full
run_gate "$r" --full
run_gate_with "$r" 'export CARGO_BUILD_JOBS=3'
expect "a changed env flag or profile invalidates" ran_full
run_gate "$r" --full
run_gate_with "$r" 'export COVERAGE_MIN_CORE=1'
expect "a coverage threshold override invalidates" ran_full
run_gate "$r" --full
run_gate_with "$r" 'export LICENCES_GEN=/nonexistent'
expect "a licence-check override invalidates" ran_full
run_gate "$r" --full
rm "$r/node_modules/.bin/prettier"
run_gate "$r"
expect "a deleted npm tool: no reuse" ran_full
expect "...saying the tool is missing" out_has "npm tool is missing"
printf '#!/bin/sh\n' >"$r/node_modules/.bin/prettier"; chmod +x "$r/node_modules/.bin/prettier"
run_gate "$r" --full
rm -rf "$r/node_modules/prettier"
run_gate_with "$r" 'true'
expect "a deleted installed package changes the key" ran_full
mkdir "$r/node_modules/prettier"; echo '{}' >"$r/node_modules/prettier/package.json"
run_gate "$r" --full
run_gate_with "$r" 'GATE_RECEIPT_TTL=0'
expect "an expired receipt (TTL) is not used" ran_full
expect "...and says why" out_has "receipt expired"
run_gate "$r" --full
run_gate_with "$r" 'true'
expect "an unchanged key still reuses (control)" ran_none
printf '%s %s\n' "$(cut -d' ' -f1 "$(receipt "$r")")" "$(($(date +%s) - 99999))" >"$(receipt "$r")"
run_gate "$r"
expect "a receipt older than the TTL is not used" ran_full
printf 'garbage' >"$(receipt "$r")"
run_gate "$r"
expect "an unreadable receipt is not used" ran_full

# Dirty trees never reuse or write.
run_gate "$r" --full
echo dirty >>"$r/code.rs"
run_gate "$r"
expect "a modified tracked file: no reuse" ran_full
git -C "$r" checkout -q code.rs
rm -f "$(receipt "$r")"
echo dirty >>"$r/code.rs"
run_gate "$r"
expect "a dirty tree writes no receipt" test ! -f "$(receipt "$r")"
git -C "$r" checkout -q code.rs
run_gate "$r" --full
echo new >"$r/untracked.txt"
run_gate "$r"
expect "an untracked non-ignored file: no reuse" ran_full
rm -f "$r/untracked.txt"
run_gate "$r" --full
echo cache >"$r/node_modules/cache.bin"
run_gate "$r"
expect "an ignored file that is not a key input does not invalidate" ran_none

# Failure and interruption write nothing.
rm -f "$(receipt "$r")"
if STUB_FAIL=1 run_gate "$r"; then bad "a failing gate should fail"; else ok "a failing gate fails"; fi
expect "a failed run writes no receipt" test ! -f "$(receipt "$r")"
run_gate "$r" --full
STUB_FAIL=1 run_gate "$r" --full || true
expect "a failed run removes an older receipt" test ! -f "$(receipt "$r")"
# Interrupted: killed mid-run.
{
  (
    cd "$r"
    . "$root/scripts/gate.sh"
    gate_run_full() { kill -TERM "$BASHPID"; sleep 5; }
    gate_script_files() { echo gatefile.txt; }
    gate_main
  ) >/dev/null 2>&1 || true
} 2>/dev/null
expect "an interrupted run writes no receipt" test ! -f "$(receipt "$r")"

# Input changed during the run: no receipt.
(
  cd "$r"
  . "$root/scripts/gate.sh"
  gate_run_full() { echo during >>code.rs; }
  gate_script_files() { echo gatefile.txt; }
  gate_main
) >"$tmp/out" 2>&1
expect "a tree that changed during the run gets no receipt" test ! -f "$(receipt "$r")"
git -C "$r" checkout -q code.rs

# Receipts are per worktree.
git -C "$r" worktree add -q "$tmp/wt" -b other
cp "$r/gatefile.txt" "$tmp/wt/gatefile.txt"
mkdir -p "$tmp/wt/node_modules" && cp "$r/node_modules/.package-lock.json" "$tmp/wt/node_modules/"
run_gate "$r" --full
run_gate "$tmp/wt"
expect "a receipt in one worktree is not used in another" ran_full
expect "the receipt is outside the tracked tree" test -z "$(git -C "$r" ls-files | grep -i receipt || true)"

# CI never reads or writes.
run_gate "$r" --full
CI=true run_gate "$r"
expect "CI ignores a matching receipt" ran_full
rm -f "$(receipt "$r")"
CI=true run_gate "$r"
expect "CI writes no receipt" test ! -f "$(receipt "$r")"
rm -f "$(receipt "$r")"
GITHUB_ACTIONS=true run_gate "$r"
expect "GITHUB_ACTIONS also means CI" test ! -f "$(receipt "$r")"
unset CI GITHUB_ACTIONS || true

# Tag pushes: no special case, a receipt either matches or not.
run_gate "$r" --full
git -C "$r" tag v-test
run_gate "$r"
expect "a tag on the same commit reuses only through the receipt" ran_none

# ---- the prose shortcut ----
p=$(make_repo prose)
mode_of() { run_gate "$1" "${@:2}"; }
commit_file "$p" docs/history/notes.md "notes"
mode_of "$p"
expect "only a history note changed: prose checks only" ran_prose
expect "...printing base, candidate, mode and reason" out_has "base=$(git -C "$p" merge-base origin/main HEAD) candidate=$(git -C "$p" rev-parse HEAD) mode=prose"
rm -f "$(receipt "$p")"
expect "a prose run writes no receipt" test ! -f "$(receipt "$p")"
mode_of "$p" --full
expect "--full beats the shortcut" ran_full

check_full() { # description, repo ; expects a full run and prints reason
  rm -f "$(receipt "$2")"
  run_gate "$2" || true
  expect "$1" ran_full
}
commit_file "$p" README.md "# readme 2"
mode_of "$p"
expect "README plus history notes is still prose" ran_prose
commit_file "$p" code.rs "code 3"
check_full "prose plus a code file runs everything" "$p"
expect "...naming the offending path" out_has "not plain prose: code.rs"

for path in NOTICE AGENTS.md scripts/x.md .github/x.md docs/TESTERS.md docs/privacy.md site/README.md CHANGELOG.md SECURITY.md docs/agents/tester.md docs/history/fixture.txt LICENSE; do
  q=$(make_repo "bad$(echo "$path" | tr -c 'a-zA-Z0-9\n' _)")
  commit_file "$q" "$path" "x"
  check_full "$path is not plain prose" "$q"
done

# Deletions and renames.
q=$(make_repo del)
commit_file "$q" docs/history/a.md a
git -C "$q" push -q origin feature:main
git -C "$q" fetch -q origin
git -C "$q" rm -q docs/history/a.md
git -C "$q" commit -qm "rm a"
mode_of "$q"
expect "deleting a plain-prose file is prose" ran_prose
git -C "$q" reset -q --hard HEAD~1
git -C "$q" mv docs/history/a.md code2.rs
git -C "$q" commit -qm "rename to code"
check_full "a rename out of prose counts the new path" "$q"
git -C "$q" reset -q --hard HEAD~1
git -C "$q" mv docs/history/a.md scripts/a.md
git -C "$q" commit -qm "rename into scripts"
check_full "a rename into scripts counts the new path" "$q"
git -C "$q" reset -q --hard HEAD~1
commit_file "$q" code.rs "to delete"
git -C "$q" push -q origin feature:main
git -C "$q" fetch -q origin
git -C "$q" rm -q code.rs
git -C "$q" commit -qm "rm code"
check_full "deleting a code file is full" "$q"

# Fallbacks.
q=$(make_repo empty)
check_full "an empty diff is NOT a pass: full" "$q"
expect "...saying so" out_has "empty diff"
q=$(make_repo dirty)
commit_file "$q" docs/history/a.md a
echo more >>"$q/docs/history/a.md"
check_full "a dirty tree is full" "$q"
expect "...saying so" out_has "dirty"
q=$(make_repo detached)
commit_file "$q" docs/history/a.md a
git -C "$q" checkout -q --detach
check_full "a detached HEAD is full" "$q"
expect "...saying so" out_has "detached HEAD"
q=$(make_repo noorigin)
commit_file "$q" docs/history/a.md a
git -C "$q" update-ref -d refs/remotes/origin/main
check_full "a missing origin/main is full" "$q"
expect "...saying so" out_has "origin/main is missing"
q=$(make_repo shallow)
commit_file "$q" docs/history/a.md a
git -C "$q" push -q origin feature
git clone -q --depth 1 --branch feature "file://$tmp/shallow.git" "$tmp/shallowclone"
git -C "$tmp/shallowclone" fetch -q --depth 1 origin main
check_full "a shallow clone is full" "$tmp/shallowclone"
expect "...saying so" out_has "shallow"
# Several merge bases (criss-cross merge).
q=$(make_repo multi)
git -C "$q" switch -q main
commit_file "$q" m1.txt 1
git -C "$q" switch -q -c side
commit_file "$q" s1.txt 1
git -C "$q" switch -q main
commit_file "$q" m2.txt 2
git -C "$q" switch -q side
commit_file "$q" s2.txt 2
git -C "$q" merge -q --no-edit -s ours main >/dev/null
git -C "$q" switch -q main
git -C "$q" merge -q --no-edit -s ours side~1 >/dev/null || true
git -C "$q" push -q origin main
git -C "$q" fetch -q origin
git -C "$q" switch -q -c cx side
if [ "$(git -C "$q" merge-base --all origin/main HEAD | wc -l | tr -d ' ')" -gt 1 ]; then
  check_full "several merge bases are full" "$q"
  expect "...saying so" out_has "merge bases"
else
  ok "several merge bases (fixture did not produce them; skipped)"
fi

# The allowlist itself.
. "$root/scripts/gate-lib.sh"
for p in README.md docs/history/milestones/G2.md docs/ENGINEERING.md; do
  expect "allowlist accepts $p" gate_prose_path "$p"
done
for p in "docs/history/a
b.md" ../README.md /etc/x.md docs/history/../../NOTICE.md README.txt; do
  expect "allowlist rejects an odd path" bash -c '! . "$1"; ! gate_prose_path "$2"' _ "$root/scripts/gate-lib.sh" "$p"
done

if [ "$fails" -ne 0 ]; then
  echo "test-gate: $fails failure(s)" >&2
  exit 1
fi
echo "test-gate: all passed"
