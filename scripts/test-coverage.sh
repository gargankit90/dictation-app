#!/usr/bin/env bash
# Checks scripts/coverage.sh fails closed (#33, #62): on a fixture workspace, a
# crate with code and no tests must fail, and one with a passing test must
# pass. Part of the quality gate.
set -euo pipefail

root=$(git rev-parse --show-toplevel)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT

mkdir -p "$fixture/untested/src" "$fixture/tested/src"
# The repo's toolchain (with llvm-tools), not whatever `stable` is today.
cp "$root/rust-toolchain.toml" "$fixture/rust-toolchain.toml"
cat >"$fixture/Cargo.toml" <<'TOML'
[workspace]
resolver = "2"
members = ["untested", "tested"]
TOML
for name in untested tested; do
  cat >"$fixture/$name/Cargo.toml" <<TOML
[package]
name = "fixture-$name"
version = "0.0.0"
edition = "2021"
TOML
  echo 'pub fn double(x: u32) -> u32 { x * 2 }' >"$fixture/$name/src/lib.rs"
done
cat >>"$fixture/tested/src/lib.rs" <<'RS'
#[cfg(test)]
mod tests {
  #[test]
  fn doubles() {
    assert_eq!(super::double(2), 4);
  }
}
RS

run() {
  # Its own target dir: the fixture must not touch the repo's coverage data.
  # No NEXTEST_PROFILE: CI sets `ci`, which the fixture has no config for.
  env -u NEXTEST_PROFILE CARGO_TARGET_DIR="$fixture/target" COVERAGE_ROOT="$fixture" \
    COVERAGE_CRATES="$1" "$root/scripts/coverage.sh"
}

echo "==> a crate with a passing test passes"
run "tested:fixture-tested:50"

echo "==> a crate with code and no tests fails"
if run "untested:fixture-untested:0" >"$fixture/out" 2>&1; then
  cat "$fixture/out"
  echo "test-coverage: coverage.sh passed a crate with no tests" >&2
  exit 1
fi
tail -3 "$fixture/out"
echo "failed as expected"

printf '\ntest-coverage: ok\n'
