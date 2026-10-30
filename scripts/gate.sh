#!/usr/bin/env bash
# The full quality gate (docs/ENGINEERING.md). One list of checks, run by the
# lefthook `pre-push-gate` hook (from scripts/pre-push) and by CI, so both
# gate the same things. Stops at the first failing check.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

step() {
  printf '\n==> %s\n' "$*"
  "$@"
}

# Format
step cargo fmt --all --check
step npx prettier --check .

# Rust lint and dependencies
step cargo clippy --workspace --all-targets -- -D warnings
step cargo deny check
step cargo machete

# Rust tests and coverage. `--no-tests=warn` until the first milestone with
# Rust tests; nextest otherwise fails a workspace with none.
step cargo nextest run --workspace --no-tests=warn ${NEXTEST_PROFILE:+--profile "$NEXTEST_PROFILE"}
step cargo test --workspace --doc
step scripts/coverage.sh

# TypeScript
step npx tsc -b --noEmit
step npx eslint . --max-warnings 0
step npx vitest run

# Spelling and guards
step typos
step scripts/test-guards.sh

printf '\ngate: all checks passed\n'
