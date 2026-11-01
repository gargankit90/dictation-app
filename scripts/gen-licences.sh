#!/usr/bin/env bash
# Writes the third-party licence notice (#24) to stdout, or to the file given
# as the first argument:
#
#   scripts/gen-licences.sh THIRD-PARTY-LICENSES.txt
#
# The committed THIRD-PARTY-LICENSES.txt is bundled into the app
# (Contents/Resources) and shipped next to the DMG. scripts/check-licences.sh,
# part of the gate, fails when it is out of date, so a dependency change
# needs a regenerated file in the same commit.
#
# Needs cargo-about (`cargo install cargo-about --version 0.9.3 --locked
# --features cli`), jq and `npm ci`.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

for tool in cargo-about jq node; do
  command -v "$tool" >/dev/null || {
    echo "gen-licences: $tool is not installed" >&2
    exit 1
  }
done
[ -d node_modules ] || {
  echo "gen-licences: node_modules is missing; run npm ci" >&2
  exit 1
}

# The crates deny.toml allows under MPL-2.0, from its exceptions list.
mpl_crates() {
  sed -n 's/.*allow *= *\["MPL-2.0"\], *crate *= *"\([^"]*\)".*/\1/p' deny.toml | sort -u
}

mpl_section() {
  local metadata crate lines
  metadata=$(cargo metadata --format-version 1 --locked)
  for crate in $(mpl_crates); do
    lines=$(jq -r --arg name "$crate" '
      .packages[]
      | select(.name == $name and (.source // "" | startswith("registry+")))
      | "  - \(.name) \(.version): https://crates.io/crates/\(.name)/\(.version)"
    ' <<<"$metadata" | sort -u)
    if [ -z "$lines" ]; then
      echo "gen-licences: $crate is in deny.toml's MPL-2.0 exceptions but not in the lockfile" >&2
      exit 1
    fi
    printf '%s\n' "$lines"
  done
}

generate() {
  cat <<'HEADER'
THIRD-PARTY LICENCES — dictation-app
====================================

dictation-app is built from its own code and the open-source software listed
below. This file gives each component's licence text. The credits that the
licences ask for come first (NOTICE), then the MPL-2.0 components, then every
Rust crate and npm package that ships in the app.

--------------------------------------------------------------------------------
NOTICE
--------------------------------------------------------------------------------

HEADER
  cat NOTICE
  cat <<'MPL'

--------------------------------------------------------------------------------
MPL-2.0 components
--------------------------------------------------------------------------------

These Rust crates are licensed under the Mozilla Public License 2.0 (text
below). They come in through Tauri (tauri-utils -> dom_query -> cssparser,
cssparser-macros, dtoa-short and selectors; dirs -> option-ext). dictation-app
uses them exactly as published on crates.io, unmodified, and links them as
libraries, so no part of dictation-app's own code falls under the MPL. Their
source code is available at the crates.io addresses below, and from the
repositories each licence entry lists. If we ever change one, the modified
MPL-licensed files will be published.

MPL
  mpl_section
  cat <<'RUST'

--------------------------------------------------------------------------------
Rust crates
--------------------------------------------------------------------------------

RUST
  cargo about generate --locked about.hbs 2>/dev/null
  cat <<'NPM'

--------------------------------------------------------------------------------
npm packages (the settings window and overlay)
--------------------------------------------------------------------------------

NPM
  node scripts/npm-licences.js
}

if [ $# -gt 0 ]; then
  tmp=$(mktemp)
  trap 'rm -f "$tmp"' EXIT
  generate >"$tmp"
  mv "$tmp" "$1"
  trap - EXIT
else
  generate
fi
