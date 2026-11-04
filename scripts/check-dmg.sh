#!/usr/bin/env bash
# Checks a built DMG the way a tester meets it: it verifies, it mounts, it
# holds Vocet.app beside an Applications shortcut, the app's
# signature is intact, the Info.plist says what we promise (bundle id, macOS
# 13 minimum, menu-bar app) and the third-party licence notice is inside.
#
#   scripts/check-dmg.sh path/to/Vocet_0.1.0_aarch64.dmg
#
# macOS only. Used by scripts/build-dmg.sh, CI and tests/e2e.
set -euo pipefail

dmg=${1:?usage: check-dmg.sh <dmg>}
[ -f "$dmg" ] || {
  echo "check-dmg: no such file: $dmg" >&2
  exit 1
}

mount=$(mktemp -d)
cleanup() {
  hdiutil detach "$mount" -quiet -force 2>/dev/null || true
  rmdir "$mount" 2>/dev/null || true
}
trap cleanup EXIT

fail() {
  echo "check-dmg: $*" >&2
  exit 1
}

hdiutil verify "$dmg" -quiet || fail "hdiutil verify failed"
hdiutil attach "$dmg" -nobrowse -readonly -noautoopen -mountpoint "$mount" -quiet ||
  fail "could not mount $dmg"

app=$mount/Vocet.app
[ -d "$app" ] || fail "no Vocet.app in the DMG ($(ls "$mount" | tr '\n' ' '))"
[ -e "$mount/Applications" ] || fail "no Applications shortcut in the DMG"

codesign --verify --deep --strict "$app" || fail "the app's signature does not verify"

plist=$app/Contents/Info.plist
plist_value() { /usr/libexec/PlistBuddy -c "Print :$1" "$plist" 2>/dev/null || true; }
[ "$(plist_value CFBundleIdentifier)" = "com.dictationapp.desktop" ] ||
  fail "unexpected bundle id: $(plist_value CFBundleIdentifier)"
[ "$(plist_value LSMinimumSystemVersion)" = "13.0" ] ||
  fail "LSMinimumSystemVersion is '$(plist_value LSMinimumSystemVersion)', not 13.0"
[ "$(plist_value LSUIElement)" = "true" ] || fail "LSUIElement is not true (the app would show a Dock icon)"
[ -n "$(plist_value NSMicrophoneUsageDescription)" ] || fail "no microphone usage description"

notice=$app/Contents/Resources/THIRD-PARTY-LICENSES.txt
[ -s "$notice" ] || fail "THIRD-PARTY-LICENSES.txt is missing from the app's Resources"
grep -q "Mozilla Public License" "$notice" || fail "the notice has no MPL-2.0 text"
grep -q "https://crates.io/crates/selectors/" "$notice" || fail "the notice has no MPL crate source line"

details=$(codesign -dvv "$app" 2>&1)
signature=$(sed -n 's/^Authority=//p' <<<"$details" | head -1)
[ -n "$signature" ] || signature=$(sed -n 's/^Signature=//p' <<<"$details" | head -1)
echo "check-dmg: ok: $(basename "$dmg"), version $(plist_value CFBundleShortVersionString), signed: ${signature:-unknown}"
