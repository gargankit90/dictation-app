#!/usr/bin/env bash
# The full quality gate (docs/ENGINEERING.md). One list of checks, run by the
# lefthook `pre-push-gate` hook (from scripts/pre-push) and by CI, so both
# gate the same things. Stops at the first failing check.
#
# What really happens (G2), in this order:
#   1. CI (CI or GITHUB_ACTIONS set) runs everything, always. It never reads
#      or writes a receipt and never takes the prose shortcut.
#   2. Otherwise, unless --full: if this checkout's tree is clean and an
#      earlier full run passed on identical input (tree, gate scripts, OS and
#      architecture, tool versions, env flags, installed npm packages), within
#      GATE_RECEIPT_TTL (4 h), the run is skipped with a message. The receipt
#      lives in this worktree's git directory, outside the tracked tree.
#   3. Otherwise, unless --full: if every file changed against the merge base
#      of origin/main is plain Markdown on the allowlist in gate-lib.sh, only
#      prettier, typos and the link check run (no receipt is written).
#      Anything unclear (dirty tree, detached HEAD, shallow clone, missing
#      origin/main, several merge bases, empty diff, any other path) runs the
#      full gate, and the reason is printed.
#   4. A full run that passes on a clean tree writes the receipt, atomically,
#      only at the very end; a failure or interruption writes none.
# --full ignores receipts and the shortcut. The rule stays: run the gate
# before pushing. Helpers: scripts/gate-lib.sh; tests: scripts/test-gate.sh.
set -euo pipefail

gate_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/gate-lib.sh
. "$gate_dir/gate-lib.sh"

step() {
  printf '\n==> %s\n' "$*"
  "$@"
}

gate_check_node() {
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
}
npx() {
  if [ ! -x "node_modules/.bin/$1" ]; then
    echo "gate: $1 is not in node_modules; run npm ci" >&2
    return 1
  fi
  command npx --no-install "$@"
}

# Plain-prose shortcut: formatting, spelling and links only.
gate_run_prose() {
  gate_check_node
  step npx prettier --check .
  step typos
  step scripts/check-doc-links.sh
  printf '\ngate: prose-only checks passed (not a full gate run)\n'
}

gate_run_full() {
  gate_check_node

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
  # The receipt and prose-shortcut rules (G2), with the checks stubbed.
  step scripts/test-gate.sh
  # No workflow can reach the owner's Mac or a secret (PUB1): hosted runners only,
  # pinned actions, read-only token. And e2e-main.sh refuses non-main commits.
  step scripts/check-workflows.sh
  # Relative Markdown links resolve, incl. docs/history (D3).
  step scripts/check-doc-links.sh
  step scripts/test-e2e-main.sh
  step tests/e2e/test-key-handling.sh
  step tests/e2e/test-vm-lock.sh
  step tests/e2e/test-runner.sh
  printf '\ngate: all checks passed\n'
}

gate_main() {
  local force_full=0 arg key="" use_receipt=0 mode_reason=""
  for arg in "$@"; do
    case "$arg" in
      --full) force_full=1 ;;
      *) echo "usage: scripts/gate.sh [--full]" >&2; exit 2 ;;
    esac
  done
  cd "$(git rev-parse --show-toplevel)"

  # F1: no test launches a browser or Finder, or reads the real Keychain; the
  # real launcher and keychain store refuse while this is set.
  export DICTATION_NO_LAUNCH=1

  if gate_is_ci; then
    mode_reason="CI always runs everything; no receipt is read or written"
  elif [ "$force_full" = 1 ]; then
    mode_reason="--full: receipts and the prose shortcut are ignored"
  fi

  if ! gate_is_ci && gate_tree_clean; then
    use_receipt=1
    key=$(gate_key)
    if [ "$force_full" = 0 ]; then
      if ! gate_tools_installed; then
        GATE_RECEIPT_WHY="an npm tool is missing from node_modules"
      elif gate_receipt_valid "$key"; then
        printf 'gate: skipped. This exact input (tree, gate scripts, tools, env, packages) passed a full run %s min ago (receipt: %s). Run scripts/gate.sh --full to run it anyway.\n' \
          "$((($(date +%s) - GATE_RECEIPT_AT) / 60))" "$(gate_receipt_file)"
        return 0
      fi
      printf 'gate: no reusable receipt (%s)\n' "$GATE_RECEIPT_WHY"
    else
      # Same key, so a failure here must not leave the old receipt standing.
      # (A receipt for other input is kept: it still describes that input.)
      gate_receipt_clear
    fi
  fi

  if gate_is_ci || [ "$force_full" = 1 ]; then
    printf 'gate: mode=full reason=%s\n' "$mode_reason"
  else
    gate_select_mode
    printf 'gate: base=%s candidate=%s mode=%s reason=%s\n' "$GATE_BASE" "$GATE_CAND" "$GATE_MODE" "$GATE_REASON"
    if [ "$GATE_MODE" = prose ]; then
      gate_run_prose
      return 0
    fi
  fi

  gate_run_full

  # Only a full pass on input that did not change while it ran earns a receipt.
  if [ "$use_receipt" = 1 ]; then
    if gate_tree_clean && [ "$(gate_key)" = "$key" ]; then
      gate_receipt_write "$key"
      echo "gate: receipt written (valid for $((GATE_RECEIPT_TTL / 3600)) h on identical input)"
    else
      echo "gate: input changed during the run; no receipt written"
    fi
  fi
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  gate_main "$@"
fi
