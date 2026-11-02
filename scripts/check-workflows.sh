#!/usr/bin/env bash
# Checks the workflow files are safe for a public repository (PUB1): nothing
# in them can reach the owner's Mac or a secret. Part of the quality gate.
#
#   scripts/check-workflows.sh [workflow-dir]     (default .github/workflows)
#
# Per file, comments ignored:
#   - every `runs-on:` is one GitHub-hosted image (ubuntu-*, macos-*,
#     windows-*): no list, no `self-hosted`, no `owner-mac`, no `macOS` label;
#   - a top-level `permissions:` block that is exactly `contents: read`;
#   - every action is pinned to a full 40-character commit SHA;
#   - every actions/checkout sets `persist-credentials: false`;
#   - no `pull_request_target`, no `workflow_run`, no `secrets.` reads.
set -euo pipefail

dir=${1:-.github/workflows}
fail=0
err() {
  echo "check-workflows: $1: $2" >&2
  fail=1
}

shopt -s nullglob
files=("$dir"/*.yml "$dir"/*.yaml)
if [ ${#files[@]} -eq 0 ]; then
  echo "check-workflows: no workflow files in $dir" >&2
  exit 1
fi

for f in "${files[@]}"; do
  # Comments removed (a `#` after whitespace or at the start of a line).
  body=$(sed -E 's/(^|[[:space:]])#.*$//' "$f")

  if grep -qiE 'self-hosted|owner-mac' <<<"$body"; then
    err "$f" "mentions a self-hosted runner (self-hosted / owner-mac)"
  fi

  runs_on=$(grep -E '^[[:space:]]*runs-on:' <<<"$body" || true)
  if [ -z "$runs_on" ]; then
    err "$f" "has no runs-on"
  else
    while IFS= read -r line; do
      value=$(sed -E 's/^[[:space:]]*runs-on:[[:space:]]*//; s/[[:space:]]+$//' <<<"$line")
      if ! grep -qE '^(ubuntu|macos|windows)-[0-9a-z.]+$' <<<"$value"; then
        err "$f" "runs-on must be one hosted image such as ubuntu-24.04, got: $value"
      fi
    done <<<"$runs_on"
  fi

  perms=$(awk '/^permissions:/ {p=1; print; next} p && /^[^[:space:]]/ {p=0} p && NF {print}' <<<"$body")
  if [ "$perms" != $'permissions:\n  contents: read' ]; then
    err "$f" "top-level permissions must be exactly 'contents: read'"
  fi

  uses=$(grep -E '^[[:space:]-]*uses:' <<<"$body" || true)
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    ref=$(sed -E 's/^[[:space:]-]*uses:[[:space:]]*//; s/[[:space:]]+$//' <<<"$line")
    case $ref in ./*) continue ;; esac
    if ! grep -qE '@[0-9a-f]{40}$' <<<"$ref"; then
      err "$f" "action not pinned to a full commit SHA: $ref"
    fi
  done <<<"$uses"

  # Each checkout step has persist-credentials: false in its `with:` block.
  if ! awk '
    function close_step() { if (in_co && !ok) bad = 1; in_co = 0; ok = 0 }
    /^[[:space:]]*-[[:space:]]/ { close_step() }
    /uses:[[:space:]]*actions\/checkout@/ { in_co = 1 }
    in_co && /persist-credentials:[[:space:]]*false[[:space:]]*$/ { ok = 1 }
    END { close_step(); exit bad }
  ' <<<"$body"; then
    err "$f" "actions/checkout without 'persist-credentials: false'"
  fi

  if grep -qE 'pull_request_target|workflow_run' <<<"$body"; then
    err "$f" "uses pull_request_target or workflow_run"
  fi
  if grep -qE 'secrets\.' <<<"$body"; then
    err "$f" "reads secrets"
  fi
done

if [ "$fail" = 0 ]; then
  echo "check-workflows: ${#files[@]} workflow file(s) ok"
fi
exit "$fail"
