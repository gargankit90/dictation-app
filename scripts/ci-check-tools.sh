#!/usr/bin/env bash
# Checks the self-hosted CI runner has the tools a workflow needs, at the
# versions the repo pins, and says what is missing. The runner is the
# owner's Mac, where these are installed by hand; no workflow downloads them
# (docs/ENGINEERING.md, "CI runner").
#
# Usage: scripts/ci-check-tools.sh [cargo-subcommand-or-tool ...]
# Always checks rustc, cargo and node (against rust-toolchain.toml and
# .nvmrc); each argument is a further tool, e.g. cargo-nextest or typos.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

missing=()
for tool in rustc cargo node npm "$@"; do
  if command -v "$tool" >/dev/null 2>&1; then
    printf '%-18s %s\n' "$tool" "$(command -v "$tool")"
  else
    missing+=("$tool")
  fi
done

# The Node major version in .nvmrc (setup-node would have installed it).
want_node=$(tr -d 'v \n' <.nvmrc)
if command -v node >/dev/null 2>&1; then
  have_node=$(node --version | tr -d 'v')
  if [ "${have_node%%.*}" != "${want_node%%.*}" ]; then
    echo "ci-check-tools: node $have_node, but .nvmrc wants $want_node" >&2
    exit 1
  fi
fi

if [ ${#missing[@]} -gt 0 ]; then
  echo "ci-check-tools: not installed on this runner: ${missing[*]}" >&2
  exit 1
fi
