#!/usr/bin/env bash
# Tests for scripts/e2e-main.sh and scripts/check-workflows.sh, with no VM,
# build or network: a scratch "origin" repo, a stub ci-run.sh. Part of the
# quality gate.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd -P)
# shellcheck disable=SC2046 # word splitting is wanted: one name per word
unset $(git rev-parse --local-env-vars)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

passed=0
failed=0
check() { # name expected actual
  if [ "$2" = "$3" ]; then
    passed=$((passed + 1))
  else
    failed=$((failed + 1))
    echo "FAIL: $1 (expected '$2', got '$3')" >&2
  fi
}
g() { git -c user.name=t -c user.email=t@example.com "$@"; }

# --- e2e-main.sh --------------------------------------------------------
# "origin" is an ordinary repo on branch main; the clone's origin/main
# follows it, so the script's fetch needs no network.
g init -q -b main "$tmp/origin"
echo one >"$tmp/origin/file"
g -C "$tmp/origin" add -A
g -C "$tmp/origin" commit -q -m one
main1=$(g -C "$tmp/origin" rev-parse HEAD)
echo two >"$tmp/origin/file"
g -C "$tmp/origin" commit -q -am two
main2=$(g -C "$tmp/origin" rev-parse HEAD)

g clone -q "$tmp/origin" "$tmp/clone" 2>/dev/null
mkdir -p "$tmp/clone/scripts"
cp "$root/scripts/e2e-main.sh" "$tmp/clone/scripts/"
g -C "$tmp/clone" checkout -q -b side
echo side >"$tmp/clone/file"
g -C "$tmp/clone" commit -q -am side
side=$(g -C "$tmp/clone" rev-parse HEAD)

# The stub records where it ran and the SHA it was told, and writes evidence
# like ci-run.sh: about.txt, results.txt and a big log.
stub=$tmp/stub-ci-run.sh
cat >"$stub" <<'STUB'
#!/usr/bin/env bash
echo "pwd=$(pwd) head=$(git rev-parse HEAD) file=$(cat file) sha=$GITHUB_SHA" >>"$STUB_LOG"
mkdir -p target/e2e-evidence
echo "commit: $GITHUB_SHA" >target/e2e-evidence/about.txt
echo "status: ${STUB_STATUS:-PASSED}" >>target/e2e-evidence/about.txt
echo "PASS e2e_startup" >target/e2e-evidence/results.txt
head -c 200000 /dev/zero | tr '\0' x >target/e2e-evidence/run.log
exit "${STUB_RC:-0}"
STUB
chmod +x "$stub"

run_main() { # [VAR=value ...] -- [args]
  local envs=()
  while [ "$1" != -- ]; do envs+=("$1"); shift; done
  shift
  (cd "$tmp/clone" && env E2E_MAIN_RUN="$stub" E2E_MAIN_SKIP_INSTALL=1 \
    E2E_MAIN_EVIDENCE_ROOT="$tmp/evidence" STUB_LOG="$tmp/stub.log" \
    ${envs[@]+"${envs[@]}"} scripts/e2e-main.sh "$@") 2>"$tmp/err"
}
worktrees() { git -C "$tmp/clone" worktree list | wc -l | tr -d ' '; }
last_run() { sed -E 's/^pwd=[^ ]* //' "$tmp/stub.log" | tail -1; }
evidence_files() { ls "$tmp"/evidence/*/ | tr '\n' ' ' | sed 's/ $//'; }

: >"$tmp/stub.log"
set +e
run_main -- "$side"
check "refuses a SHA not on origin/main" 2 "$?"
check "the stub never ran for a refused SHA" 0 "$(wc -l <"$tmp/stub.log" | tr -d ' ')"
run_main -- deadbeefdeadbeefdeadbeefdeadbeefdeadbeef
check "refuses an unknown object" 2 "$?"
run_main -- a b
check "refuses two arguments" 2 "$?"

run_main --
check "the default (origin/main tip) passes" 0 "$?"
check "the tip is checked out" "head=$main2 file=two sha=$main2" "$(last_run)"
check "not in the caller's checkout" 0 "$(grep -c "pwd=$tmp/clone " "$tmp/stub.log" || true)"
check "the worktree is removed" 1 "$(worktrees)"
check "a pass keeps only the summary" "about.txt results.txt" "$(evidence_files)"

run_main -- "$main1"
check "an older origin/main commit is allowed" 0 "$?"
check "that commit is what got checked out" "head=$main1 file=one sha=$main1" "$(last_run)"

rm -rf "$tmp/evidence"
run_main STUB_RC=1 "STUB_STATUS=FAILED (exit 1)" --
check "a failed run exits 1" 1 "$?"
check "a failed run keeps the full evidence" "about.txt results.txt run.log" "$(evidence_files)"
check "the worktree is removed after a failure" 1 "$(worktrees)"
set -e

# --- check-workflows.sh -------------------------------------------------
cw=$root/scripts/check-workflows.sh
good='name: T
on: [push]
permissions:
  contents: read
jobs:
  j:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7
        with:
          persist-credentials: false
      - run: echo hi'
case_dir() { # name content
  mkdir -p "$tmp/wf/$1"
  printf '%s\n' "$2" >"$tmp/wf/$1/w.yml"
  echo "$tmp/wf/$1"
}
cw_rc() { "$cw" "$1" >/dev/null 2>&1 && echo 0 || echo 1; }
nope='permissions:
  contents: read
'
check "a good workflow passes" 0 "$(cw_rc "$(case_dir good "$good")")"
check "self-hosted fails" 1 "$(cw_rc "$(case_dir sh "${good/ubuntu-24.04/[self-hosted, macOS, owner-mac]}")")"
check "a runner label list fails" 1 "$(cw_rc "$(case_dir lst "${good/ubuntu-24.04/[ubuntu-24.04]}")")"
check "an unpinned action fails" 1 "$(cw_rc "$(case_dir pin "${good/3d3c42e5aac5ba805825da76410c181273ba90b1 # v7/v4}")")"
check "checkout without persist-credentials fails" 1 \
  "$(cw_rc "$(case_dir pc "${good/persist-credentials: false/fetch-depth: 1}")")"
check "wider permissions fail" 1 "$(cw_rc "$(case_dir perm "${good/contents: read/contents: write}")")"
check "missing permissions fail" 1 "$(cw_rc "$(case_dir noperm "${good/"$nope"/}")")"
check "pull_request_target fails" 1 "$(cw_rc "$(case_dir prt "${good/on: [push]/on: [pull_request_target]}")")"
check "workflow_run fails" 1 "$(cw_rc "$(case_dir wr "${good/on: [push]/on: [workflow_run]}")")"
check "a secrets read fails" 1 "$(cw_rc "$(case_dir sec "$good
      - run: echo \${{ secrets.X }}")")"
check "a comment naming self-hosted is fine" 0 "$(cw_rc "$(case_dir cmt "# not self-hosted
$good")")"
check "the repo's workflows pass" 0 "$(cw_rc "$root/.github/workflows")"

echo "test-e2e-main: $passed passed, $failed failed"
[ "$failed" = 0 ]
