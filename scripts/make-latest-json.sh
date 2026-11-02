#!/usr/bin/env bash
# Writes the updater's latest.json (the manifest tauri-plugin-updater reads)
# for a macOS build, from the .app.tar.gz and .sig that `tauri build` makes
# when bundle.createUpdaterArtifacts is on. Holds no key: the .sig is the
# signature the build made with the private key.
#
#   scripts/make-latest-json.sh <version> <notes> <app.tar.gz> <app.tar.gz.sig> <download-url> > latest.json
#
# The platform key is darwin-<arch> of the machine that built the bundle.
# Used by docs/releasing.md and scripts/updater-test.sh.
set -euo pipefail

version=${1:?version}
notes=${2:?notes}
archive=${3:?path to the .app.tar.gz}
sigfile=${4:?path to the .sig}
url=${5:?download url of the .app.tar.gz}

[ -f "$archive" ] || {
  echo "make-latest-json: no such file: $archive" >&2
  exit 1
}
[ -s "$sigfile" ] || {
  echo "make-latest-json: no signature at $sigfile" >&2
  exit 1
}

case $(uname -m) in
  arm64 | aarch64) arch=aarch64 ;;
  x86_64) arch=x86_64 ;;
  *)
    echo "make-latest-json: unknown architecture $(uname -m)" >&2
    exit 1
    ;;
esac

jq -n \
  --arg version "$version" \
  --arg notes "$notes" \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg sig "$(cat "$sigfile")" \
  --arg url "$url" \
  --arg platform "darwin-$arch" \
  '{version: $version, notes: $notes, pub_date: $date,
    platforms: {($platform): {signature: $sig, url: $url}}}'
