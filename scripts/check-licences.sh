#!/usr/bin/env bash
# Fails when THIRD-PARTY-LICENSES.txt (#24) is out of date: regenerates it
# with scripts/gen-licences.sh and compares. Also checks that every crate
# deny.toml allows under MPL-2.0 is named in the committed file with its
# crates.io address, since that is what the MPL asks us to provide.
# Part of the quality gate.
#
# LICENCES_FILE and LICENCES_GEN point the script at a fixture;
# scripts/test-check-licences.sh does that.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

file=${LICENCES_FILE:-THIRD-PARTY-LICENSES.txt}
gen=${LICENCES_GEN:-scripts/gen-licences.sh}
deny=${LICENCES_DENY:-deny.toml}

[ -f "$file" ] || {
  echo "check-licences: $file is missing; run scripts/gen-licences.sh $file" >&2
  exit 1
}

fresh=$(mktemp)
trap 'rm -f "$fresh"' EXIT
"$gen" >"$fresh"

if ! diff -u "$file" "$fresh" >/dev/null; then
  echo "check-licences: $file is out of date (dependencies or NOTICE changed)." >&2
  echo "Run: scripts/gen-licences.sh $file   and commit it." >&2
  diff -u "$file" "$fresh" | head -40 >&2 || true
  exit 1
fi

status=0
for crate in $(sed -n 's/.*allow *= *\["MPL-2.0"\], *crate *= *"\([^"]*\)".*/\1/p' "$deny" | sort -u); do
  if ! grep -q "^  - $crate [0-9][^:]*: https://crates.io/crates/$crate/" "$file"; then
    echo "check-licences: $crate (MPL-2.0 in $deny) has no crates.io source line in $file" >&2
    status=1
  fi
done
exit $status
