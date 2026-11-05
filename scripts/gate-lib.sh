#!/usr/bin/env bash
# Helpers for scripts/gate.sh (G2): the successful-run receipt and the
# plain-prose shortcut. Sourced, never run. No check runs from here, so
# scripts/test-gate.sh can test every rule without the expensive steps.
#
# Receipt: after a full gate run passes on a clean tree, gate.sh stores a
# digest of everything the result depends on. A later run with the same
# digest (inside GATE_RECEIPT_TTL) is skipped. CI never reads or writes one.
#
# Prose shortcut: when every file changed against the merge base of
# origin/main is ordinary Markdown on the short allowlist below, only the
# format, spelling and link checks run. Anything unclear means the full gate.

GATE_RECEIPT_TTL=${GATE_RECEIPT_TTL:-14400} # seconds (4 h): advisories and dependency data go stale
GATE_BASE_REF=origin/main

gate_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum | cut -d' ' -f1; else shasum -a 256 | cut -d' ' -f1; fi
}

# CI always runs everything.
gate_is_ci() {
  [ -n "${CI:-}" ] || [ -n "${GITHUB_ACTIONS:-}" ]
}

# Clean = nothing modified, staged or untracked (ignored files don't count).
# A git failure counts as dirty.
gate_tree_clean() {
  local status
  status=$(git status --porcelain=v1 -z --untracked-files=all 2>/dev/null) || return 1
  [ -z "$status" ]
}

gate_platform() { uname -sm; }

# One line per tool the checks depend on; a missing tool is part of the key.
gate_tool_versions() {
  local cmd
  for cmd in "rustc --version" "cargo --version" "node --version" "npm --version" \
    "cargo deny --version" "cargo machete --version" "cargo nextest --version" \
    "cargo llvm-cov --version" "cargo about --version" "typos --version"; do
    printf '%s: %s\n' "$cmd" "$($cmd 2>&1 | head -n 1 || true)"
  done
}

# Variables that change what the checks do (flags, profiles, toolchain).
gate_env_flags() {
  env | grep -E '^(NEXTEST_|CARGO_|RUST|DICTATION_|LLVM_|CI=|GITHUB_ACTIONS=)' | LC_ALL=C sort || true
}

# What npm ci installed (the lockfile is in the tree; this is the installed copy).
gate_install_state() {
  if [ -f node_modules/.package-lock.json ]; then gate_sha256 <node_modules/.package-lock.json; else echo none; fi
}

# The files whose content is part of the key (the gate's own scripts).
gate_script_files() { echo "scripts/gate.sh scripts/gate-lib.sh"; }

# Prints the receipt key for the current checkout; fails on a dirty tree.
gate_key() {
  gate_tree_clean || return 1
  {
    echo "tree: $(git rev-parse 'HEAD^{tree}')"
    echo "gate: $(cat $(gate_script_files) | gate_sha256)"
    echo "platform: $(gate_platform)"
    gate_tool_versions
    gate_env_flags
    echo "install: $(gate_install_state)"
  } | gate_sha256
}

gate_receipt_file() { echo "$(git rev-parse --absolute-git-dir)/gate-receipt"; }

# Succeeds when a receipt for key $1 exists and is younger than the TTL.
# Sets GATE_RECEIPT_WHY to the reason it was not usable.
gate_receipt_valid() {
  local key=$1 file stored_key stored_at now
  file=$(gate_receipt_file)
  GATE_RECEIPT_WHY="no receipt"
  [ -f "$file" ] || return 1
  read -r stored_key stored_at <"$file" || true
  GATE_RECEIPT_WHY="receipt is for different input"
  [ "$stored_key" = "$key" ] || return 1
  case "$stored_at" in '' | *[!0-9]*) GATE_RECEIPT_WHY="receipt is unreadable"; return 1 ;; esac
  now=$(date +%s)
  GATE_RECEIPT_WHY="receipt expired"
  [ $((now - stored_at)) -ge 0 ] && [ $((now - stored_at)) -lt "$GATE_RECEIPT_TTL" ] || return 1
  GATE_RECEIPT_AT=$stored_at
  return 0
}

# Atomic: a temp file in the same directory, then rename.
gate_receipt_write() {
  local key=$1 file tmp
  file=$(gate_receipt_file)
  tmp=$(mktemp "$file.XXXXXX")
  printf '%s %s\n' "$key" "$(date +%s)" >"$tmp"
  mv -f "$tmp" "$file"
}

gate_receipt_clear() { rm -f "$(gate_receipt_file)"; }

# ---- the plain-prose shortcut ----

# Ordinary prose only. Not here, on purpose: AGENTS.md, CLAUDE.md, SECURITY.md,
# CHANGELOG.md (release notes source), NOTICE, licences, docs/TESTERS.md and
# docs/releasing.md (read by scripts and e2e), docs/privacy.md and docs/agents
# (policy), site/, .github/, scripts/, .claude/.
gate_prose_path() {
  case "$1" in
    *[[:cntrl:]]* | */../* | ../* | /*) return 1 ;;
    README.md | CONTRIBUTING.md | docs/ENGINEERING.md | docs/review-checklist.md) return 0 ;;
    docs/history/*.md) return 0 ;;
  esac
  return 1
}

# Sets GATE_MODE (full|prose), GATE_REASON, GATE_BASE, GATE_CAND.
gate_select_mode() {
  GATE_MODE=full GATE_BASE="-" GATE_CAND="-"
  local cand bases n list path count=0
  if ! gate_tree_clean; then GATE_REASON="the working tree is dirty (or git status failed)"; return 0; fi
  if ! git symbolic-ref -q HEAD >/dev/null 2>&1; then GATE_REASON="detached HEAD"; return 0; fi
  if ! cand=$(git rev-parse --verify -q 'HEAD^{commit}'); then GATE_REASON="cannot resolve HEAD"; return 0; fi
  GATE_CAND=$cand
  if [ "$(git rev-parse --is-shallow-repository 2>/dev/null)" != false ]; then GATE_REASON="shallow history"; return 0; fi
  if ! git rev-parse --verify -q "refs/remotes/$GATE_BASE_REF^{commit}" >/dev/null; then
    GATE_REASON="$GATE_BASE_REF is missing"
    return 0
  fi
  if ! bases=$(git merge-base --all "refs/remotes/$GATE_BASE_REF" "$cand" 2>/dev/null) || [ -z "$bases" ]; then
    GATE_REASON="no merge base with $GATE_BASE_REF"
    return 0
  fi
  n=$(printf '%s\n' "$bases" | wc -l | tr -d ' ')
  if [ "$n" != 1 ]; then GATE_REASON="$n merge bases with $GATE_BASE_REF"; return 0; fi
  GATE_BASE=$bases
  list=$(mktemp)
  # No rename detection: a rename is a deletion of the old path plus an addition.
  if ! git diff --name-only -z --no-renames "$GATE_BASE" "$cand" >"$list"; then
    rm -f "$list"
    GATE_REASON="git diff failed"
    return 0
  fi
  while IFS= read -r -d '' path; do
    count=$((count + 1))
    if ! gate_prose_path "$path"; then
      rm -f "$list"
      GATE_REASON="not plain prose: $path"
      return 0
    fi
  done <"$list"
  rm -f "$list"
  if [ "$count" -eq 0 ]; then GATE_REASON="empty diff against $GATE_BASE_REF (nothing proves the code was tested)"; return 0; fi
  GATE_MODE=prose
  GATE_REASON="all $count changed file(s) are plain Markdown on the allowlist"
}
