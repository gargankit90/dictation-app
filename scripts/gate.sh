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

# npm tools run only from node_modules. Plain npx would offer to install a
# missing one, and lefthook hides that prompt, so the gate hung (#25).
if [ ! -d node_modules ]; then
  echo "gate: node_modules is missing; run npm ci" >&2
  exit 1
fi
if [ package-lock.json -nt node_modules/.package-lock.json ]; then
  echo "gate: package-lock.json changed since the last install; run npm ci" >&2
  exit 1
fi
npx() {
  if [ ! -x "node_modules/.bin/$1" ]; then
    echo "gate: $1 is not in node_modules; run npm ci" >&2
    return 1
  fi
  command npx --no-install "$@"
}

# Format
step cargo fmt --all --check
step npx prettier --check .

# Rust lint and dependencies
step cargo clippy --workspace --all-targets -- -D warnings
# The platform crate on its own, without feature `testing`: the workspace run
# above has it on (app dev-depends on platform with it), so production code
# that reaches for `crate::fakes`, or warns only without the fakes, would pass
# it (#48). Only the library, as shipped.
step cargo clippy -p dictation-platform -- -D warnings
step cargo deny check
step cargo machete
# The pipeline holds no Tauri types (docs/ENGINEERING.md, M1.3).
# The tree is captured first, so a cargo tree failure fails the step
# instead of being negated by the `!` (#43).
no_tauri_in_pipeline() {
  local tree
  tree=$(cargo tree -p dictation-pipeline -e normal,build --prefix none)
  ! grep -q '^tauri' <<<"$tree"
}
step no_tauri_in_pipeline
# The third-party licence notice that ships in the app is current, and the
# check itself fails when it should (#24).
step scripts/test-check-licences.sh
# The release script (M5.4): never prints the updater key, verifies the
# signature, refuses --publish without a clean tree and the tag. Stubs only;
# no Keychain, network or real key.
step scripts/test-release.sh
step scripts/check-licences.sh

# Rust tests and coverage. nextest fails a run that finds no tests, so a
# filter or `#[cfg]` that hides them all can't pass quietly.
step cargo nextest run --workspace ${NEXTEST_PROFILE:+--profile "$NEXTEST_PROFILE"}
# Platform's black-box tests of the fakes need feature `testing` (#35).
# The workspace run gets it only through app's dev-dependency, so name it.
step cargo nextest run -p dictation-platform --features testing ${NEXTEST_PROFILE:+--profile "$NEXTEST_PROFILE"}
step cargo test --workspace --doc
# Fakes only with feature `testing` (M1.2). `app` dev-depends on platform
# with it, which turns it on for the workspace run above, so check the
# platform crate on its own, where its compile_fail doctest runs.
step cargo test -p dictation-platform --doc
step scripts/coverage.sh
# coverage.sh fails a crate with no tests (#33, #62); prove it does.
step scripts/test-coverage.sh
# The vendored crates' callback tests (#77, #80, #88). macOS only; the script
# skips itself elsewhere.
step scripts/vendor-tests.sh

# TypeScript
step npx tsc -b --noEmit
step npx eslint . --max-warnings 0
step npx vitest run
# The production frontend the app bundle ships (#103 P2-04): a missing asset
# or a broken transform fails here, not when the app opens. About 2 s.
step npm run build

# Spelling and guards
step typos
step scripts/test-guards.sh
# No workflow can reach the owner's Mac or a secret (PUB1): hosted runners only,
# pinned actions, read-only token. And e2e-main.sh refuses non-main commits.
step scripts/check-workflows.sh
step scripts/test-e2e-main.sh
step tests/e2e/test-key-handling.sh
step tests/e2e/test-vm-lock.sh
step tests/e2e/test-runner.sh

printf '\ngate: all checks passed\n'
