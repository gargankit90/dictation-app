#!/usr/bin/env bash
# Proves scripts/check-licences.sh does fail: on a stale file, a missing
# file and an MPL-2.0 crate with no source line. Uses a stub generator, so it
# needs no cargo-about. Part of the quality gate.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cat >"$work/gen.sh" <<'GEN'
#!/usr/bin/env bash
printf 'NOTICE\n  - selectors 0.38.0: https://crates.io/crates/selectors/0.38.0\n'
GEN
chmod +x "$work/gen.sh"
cat >"$work/deny.toml" <<'DENY'
exceptions = [
    { allow = ["MPL-2.0"], crate = "selectors" },
]
DENY
"$work/gen.sh" >"$work/current.txt"

check() {
  LICENCES_FILE=$1 LICENCES_GEN=$work/gen.sh LICENCES_DENY=${2:-$work/deny.toml} \
    scripts/check-licences.sh >"$work/out.txt" 2>&1
}

fail() {
  echo "test-check-licences: $*" >&2
  cat "$work/out.txt" >&2
  exit 1
}

check "$work/current.txt" || fail "a current file was rejected"

printf 'old\n' >"$work/stale.txt"
check "$work/stale.txt" && fail "a stale file passed"
grep -q "out of date" "$work/out.txt" || fail "no 'out of date' message"

check "$work/missing.txt" && fail "a missing file passed"
grep -q "is missing" "$work/out.txt" || fail "no 'missing' message"

# deny.toml gains an MPL-2.0 crate the notice has no source line for.
cat >"$work/deny2.toml" <<'DENY'
exceptions = [
    { allow = ["MPL-2.0"], crate = "selectors" },
    { allow = ["MPL-2.0"], crate = "option-ext" },
]
DENY
check "$work/current.txt" "$work/deny2.toml" && fail "an MPL-2.0 crate without a source line passed"
grep -q "option-ext" "$work/out.txt" || fail "the missing crate was not named"

echo "test-check-licences: ok"
