#!/usr/bin/env bash
# Runs the patch tests of the vendored crates in third_party/ (#88, P1-02).
# They are not workspace members (root Cargo.toml `exclude`), so no other
# gate step runs them. They hold the callback regressions for #77 (CoreAudio
# buffer capacity) and #80 (hotkey decode) and are macOS-only code.
#
# Each crate is built standalone, so it needs its own lock file. It is seeded
# from the app's Cargo.lock, so the crates are tested against the dependency
# versions the app ships, not whatever is newest today. The seed is removed
# afterwards, and a version that differs from the app's lock fails the run.
# Each crate gets its own CARGO_TARGET_DIR under target/, apart from the
# workspace build.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [ "$(uname -s)" != Darwin ]; then
  echo "vendor-tests: macOS only; skipped on $(uname -s)"
  exit 0
fi

# crate dir : cargo test filter
crates=(
  "third_party/handy-keys:dictation_patch"
  "third_party/coreaudio-rs:patch_tests"
)

host=$(rustc -vV | sed -n 's/^host: //p')

# name vX.Y.Z for each package a manifest builds on this host, tests included.
packages() {
  cargo tree "$@" -e normal,build,dev --target "$host" --prefix none --format '{p}' |
    awk '{ print $1, $2 }' | sort -u
}

# Prints "name vendor-version" for each package the crate resolved to a
# version the app's Cargo.lock doesn't have, while the app does use that name.
vendor_drift() {
  local app vendor
  app=$(packages --workspace)
  vendor=$(packages --manifest-path "$1/Cargo.toml")
  comm -13 <(printf '%s\n' "$app") <(printf '%s\n' "$vendor") |
    while read -r name version; do
      if grep -q "^$name " <<<"$app"; then echo "$name $version"; fi
    done
}

seeded=()
cleanup() { [ ${#seeded[@]} -eq 0 ] || rm -f "${seeded[@]}"; }
trap cleanup EXIT

for entry in "${crates[@]}"; do
  dir=${entry%%:*}
  filter=${entry##*:}
  lock="$dir/Cargo.lock"
  if [ -e "$lock" ]; then
    echo "vendor-tests: $lock exists; remove it first" >&2
    exit 1
  fi
  cp Cargo.lock "$lock"
  seeded+=("$lock")
  # cargo drops the packages the crate doesn't use and resolves any it needs
  # that the app doesn't have (dev-dependencies).
  status=0
  printf '\n==> cargo test %s --lib %s\n' "$dir" "$filter"
  CARGO_TARGET_DIR="$PWD/target/vendor-$(basename "$dir")" \
    cargo test --manifest-path "$dir/Cargo.toml" --lib "$filter" || status=$?

  # Every package built on this Mac that the app builds too must have the
  # version the app has (a few are there twice, so compare name+version).
  drift=""
  if [ "$status" -eq 0 ]; then
    drift=$(vendor_drift "$dir")
  fi
  rm -f "$lock"
  [ "$status" -eq 0 ] || exit "$status"
  if [ -n "$drift" ]; then
    echo "vendor-tests: $dir builds versions the app's Cargo.lock does not have:" >&2
    echo "$drift" >&2
    exit 1
  fi
done

printf '\nvendor-tests: ok\n'
