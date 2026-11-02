#!/usr/bin/env bash
# The VM e2e suite on a commit of origin/main, run by the Lead on the owner's
# Mac after each merge (PUB1, closes #168). It replaces .github/workflows/
# e2e.yml: the repository is public, so no workflow may reach this Mac.
#
#   scripts/e2e-main.sh [<sha>]
#
# 1. Fetches origin/main. <sha> defaults to its tip.
# 2. Refuses a SHA that is not a commit reachable from origin/main (a branch,
#    a PR head, a made-up object): only merged, reviewed code gets the VMs.
# 3. Checks that commit out into a fresh temporary worktree (never the
#    caller's working tree), installs its npm packages and runs
#    tests/e2e/ci-run.sh there. That builds the app, borrows
#    dictation-vm-2 or dictation-vm through run.sh's lock and ends as
#    "skipped: no VM free" (exit 0) if both stay taken.
# 4. Keeps the evidence under $E2E_MAIN_EVIDENCE_ROOT/<sha12>-<time>/
#    (default target/e2e-main-evidence in this checkout; untracked). Size
#    (#174): a passing run keeps only about.txt and results.txt (a few KB);
#    a failed or skipped run keeps everything ci-run.sh collected (logs,
#    screenshots), with key-shaped strings already hidden.
# 5. Removes the worktree, whatever happened. Exit 0 = passed or skipped,
#    1 = failed build or scenario, 2 = refused / bad usage.
#
# For tests: E2E_MAIN_RUN replaces tests/e2e/ci-run.sh (run inside the
# worktree), E2E_MAIN_SKIP_INSTALL=1 skips npm ci, E2E_MAIN_NO_FETCH=1 skips
# the fetch.
set -uo pipefail

repo=$(git rev-parse --show-toplevel) || exit 2
cd "$repo" || exit 2

log() { printf '[e2e-main] %s\n' "$*" >&2; }

if [ $# -gt 1 ]; then
  echo "usage: scripts/e2e-main.sh [<sha>]" >&2
  exit 2
fi

if [ "${E2E_MAIN_NO_FETCH:-0}" != 1 ] && ! git fetch --quiet origin main; then
  log "cannot fetch origin main"
  exit 2
fi
tip=$(git rev-parse --verify --quiet 'refs/remotes/origin/main^{commit}') || {
  log "no origin/main"
  exit 2
}

want=${1:-$tip}
if ! sha=$(git rev-parse --verify --quiet "$want^{commit}"); then
  log "refusing '$want': not a commit in this repository"
  exit 2
fi
if ! git merge-base --is-ancestor "$sha" "$tip"; then
  log "refusing $sha: not reachable from origin/main"
  exit 2
fi

root=${E2E_MAIN_EVIDENCE_ROOT:-$repo/target/e2e-main-evidence}
dest=$root/${sha:0:12}-$(date +%Y%m%d-%H%M%S)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/e2e-main.XXXXXX") || exit 2
wt=$tmp/wt

cleanup() {
  git -C "$repo" worktree remove --force "$wt" >/dev/null 2>&1 || true
  git -C "$repo" worktree prune >/dev/null 2>&1 || true
  rm -rf "$tmp"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

log "testing $sha (origin/main tip: ${tip:0:12})"
git worktree add --quiet --detach "$wt" "$sha" || exit 2
cd "$wt" || exit 2

if [ "${E2E_MAIN_SKIP_INSTALL:-0}" != 1 ]; then
  log "npm ci"
  if ! npm ci >&2; then
    log "npm ci failed"
    exit 1
  fi
fi

run=${E2E_MAIN_RUN:-tests/e2e/ci-run.sh}
# ci-run.sh writes its evidence to target/e2e-evidence in the worktree.
unset CI_EVIDENCE_DIR
GITHUB_SHA=$sha "$run"
rc=$?

evidence=$wt/target/e2e-evidence
mkdir -p "$dest"
if [ -d "$evidence" ]; then
  if [ "$rc" = 0 ] && grep -q '^status: PASSED' "$evidence/about.txt" 2>/dev/null; then
    for f in about.txt results.txt; do
      [ -f "$evidence/$f" ] && cp "$evidence/$f" "$dest/"
    done
  else
    cp -R "$evidence"/. "$dest"/
  fi
fi
log "evidence: $dest ($(du -sk "$dest" | cut -f1) KB)"
exit "$rc"
