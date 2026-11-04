#!/usr/bin/env bash
# Tries the updater end to end with a THROWAWAY key pair (M5.4 criteria 3 and
# 4). It never touches the real release key: the pair is made in
# target/updater-test/ (git-ignored, never committed), and the two builds
# trust only its public key and fetch from a local web server.
#
#   scripts/updater-test.sh build            # key, build 0.1.0 and 0.1.1, latest.json
#   scripts/updater-test.sh serve            # serve them (Ctrl-C to stop)
#
# Then (docs/releasing.md, "Try the updater"): install
# target/updater-test/old/Vocet.app on a Mac or in the VM, open it,
# menu-bar icon > Check for updates… > Install and restart, and see 0.1.1 run
# with its permissions kept.
#
# HOST is where the app can reach this Mac: the Lume VM sees it at
# 192.168.64.1 (the default); the Mac itself can use 127.0.0.1.
# Signing uses `dictation-app Dev` when present, so the old and new builds
# share a signature (that is what keeps the permissions); without it both
# are ad hoc and macOS will ask again.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

HOST=${HOST:-192.168.64.1}
PORT=${PORT:-8765}
OLD_VERSION=${OLD_VERSION:-0.1.0}
NEW_VERSION=${NEW_VERSION:-0.1.1}
# UPDATER_TEST_DIR: where the key, builds and manifest go (the e2e scenario
# e2e_update_keeps_permissions uses its own); under target/, git-ignored.
dir=${UPDATER_TEST_DIR:-target/updater-test}
base=http://$HOST:$PORT

sign_config='{}'
if [ "${DMG_ADHOC:-0}" = 1 ] || ! security find-identity -p codesigning 2>/dev/null | grep -q '"dictation-app Dev"'; then
  echo "updater-test: NOTE: no \"dictation-app Dev\" certificate; signing ad hoc."
  sign_config='{"bundle":{"macOS":{"signingIdentity":"-"}}}'
fi

build_one() {
  local version=$1 outdir=$2 config
  config=$(jq -nc \
    --arg pubkey "$(cat "$dir/test.key.pub")" \
    --arg endpoint "$base/latest.json" \
    --arg version "$version" \
    --argjson sign "$sign_config" \
    '{version: $version,
      bundle: {createUpdaterArtifacts: true},
      plugins: {updater: {pubkey: $pubkey, endpoints: [$endpoint],
                          dangerousInsecureTransportProtocol: true}}} * $sign')
  # A debug build: faster, and the same updater code.
  TAURI_SIGNING_PRIVATE_KEY="$PWD/$dir/test.key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    npm run tauri build -- --debug --bundles app --config "$config"
  rm -rf "$outdir"
  mkdir -p "$outdir"
  cp -R target/debug/bundle/macos/Vocet.app "$outdir/"
  cp target/debug/bundle/macos/Vocet.app.tar.gz "$outdir/"
  cp target/debug/bundle/macos/Vocet.app.tar.gz.sig "$outdir/"
}

case ${1:-} in
  build)
    mkdir -p "$dir"
    # The throwaway pair. Never commit it; target/ is git-ignored.
    rm -f "$dir/test.key" "$dir/test.key.pub"
    npx tauri signer generate --ci -w "$dir/test.key" -p "" >/dev/null
    build_one "$OLD_VERSION" "$dir/old"
    build_one "$NEW_VERSION" "$dir/new"
    mkdir -p "$dir/serve"
    cp "$dir/new/Vocet.app.tar.gz" "$dir/serve/Vocet_$NEW_VERSION.app.tar.gz"
    scripts/make-latest-json.sh "$NEW_VERSION" "A test update." \
      "$dir/serve/Vocet_$NEW_VERSION.app.tar.gz" "$dir/new/Vocet.app.tar.gz.sig" \
      "$base/Vocet_$NEW_VERSION.app.tar.gz" >"$dir/serve/latest.json"
    echo "updater-test: built $OLD_VERSION ($dir/old) and $NEW_VERSION ($dir/new)"
    echo "updater-test: manifest: $dir/serve/latest.json"
    ;;
  serve)
    [ -f "$dir/serve/latest.json" ] || {
      echo "updater-test: run '$0 build' first" >&2
      exit 1
    }
    echo "updater-test: serving $dir/serve at http://0.0.0.0:$PORT (Ctrl-C to stop)"
    exec python3 -m http.server "$PORT" --bind 0.0.0.0 --directory "$dir/serve"
    ;;
  *)
    sed -n '2,19p' "$0"
    exit 2
    ;;
esac
