#!/usr/bin/env bash
# Builds the tester DMG (M5.4) and puts it, the licence notice and a checksum
# in target/dmg-out/:
#
#   scripts/build-dmg.sh              # release build
#   scripts/build-dmg.sh --debug      # faster, for trying the packaging
#
# Signing: with the `dictation-app Dev` certificate in the keychain
# (scripts/make-dev-cert.sh) the app is signed with it, which is what keeps
# a tester's Microphone and Accessibility permission across updates. Without
# it (CI, a fresh Mac) the app is signed ad hoc: it runs, but macOS asks for
# the permissions again after every update. Either way it is not notarised.
# There is no Apple Developer account, so testers clear the quarantine flag
# (docs/testing/TESTER-README.md).
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

profile=release
tauri_profile=()
if [ "${1:-}" = "--debug" ]; then
  profile=debug
  tauri_profile=(--debug)
fi

identity="dictation-app Dev"
signing=()
# DMG_ADHOC=1 forces the ad-hoc path, to try what CI builds.
if [ "${DMG_ADHOC:-0}" != 1 ] && security find-identity -p codesigning 2>/dev/null | grep -q "\"$identity\""; then
  echo "build-dmg: signing with \"$identity\""
else
  echo "build-dmg: NOTE: no \"$identity\" certificate in the keychain; signing ad hoc."
  echo "build-dmg: NOTE: permissions will not carry over to the next build."
  signing=(--config '{"bundle":{"macOS":{"signingIdentity":"-"}}}')
fi

# Tauri styles the DMG window (icon positions) by scripting Finder, which
# times out where Finder can't be automated: an ssh session, a CI runner, a
# terminal without Automation permission. CI=true makes it skip that step.
# The DMG then holds the app and an Applications shortcut in Finder's
# default layout, which is all a tester needs.
export CI=true

# The notice bundled into the app must match the dependencies (#24).
scripts/check-licences.sh

npm run tauri build -- ${tauri_profile[@]+"${tauri_profile[@]}"} ${signing[@]+"${signing[@]}"} --bundles dmg

dmg=$(find "target/$profile/bundle/dmg" -name '*.dmg' -print -quit)
[ -n "$dmg" ] || {
  echo "build-dmg: no DMG under target/$profile/bundle/dmg" >&2
  exit 1
}

scripts/check-dmg.sh "$dmg"

out=target/dmg-out
rm -rf "$out"
mkdir -p "$out"
cp "$dmg" "$out/"
cp THIRD-PARTY-LICENSES.txt "$out/"
cp docs/testing/TESTER-README.md "$out/README.md" 2>/dev/null || true
(cd "$out" && shasum -a 256 -- *.dmg THIRD-PARTY-LICENSES.txt >SHA256SUMS)
echo "build-dmg: done:"
ls -l "$out"
