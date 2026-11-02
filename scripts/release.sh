#!/usr/bin/env bash
# Builds a tester release of dictation-app on the owner's Mac (M5.4):
#
#   scripts/release.sh <version> [--notes <text>]             # dry run
#   scripts/release.sh <version> [--notes <text>] --publish   # and upload
#
# Both build the signed release (`dictation-app Dev`, not notarised) and put
# into target/release-out/v<version>/:
#
#   dictation-app_<version>_<arch>.dmg          the installer testers get
#   dictation-app_<version>_<arch>.app.tar.gz   the updater bundle
#   dictation-app_<version>_<arch>.app.tar.gz.sig
#   latest.json                                 what installed apps fetch
#   README.md (docs/TESTERS.md), THIRD-PARTY-LICENSES.txt, SHA256SUMS
#
# The updater bundle is signed with the owner's minisign key. Its private
# half and password live only in the login Keychain, as generic passwords
# with services dictation-app-updater-key and dictation-app-updater-password.
# They are read into shell variables and handed to the build as
# TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD for that
# one command only: never exported, echoed, logged or written to a file. The
# build's output is filtered so neither can reach the terminal even if a tool
# printed one (scripts/test-release.sh proves it, also under `bash -x`).
#
# Before anything is written, the script checks that <version> is the one in
# src-tauri/tauri.conf.json, package.json and Cargo.toml. After the build it
# verifies the .sig against the public key in tauri.conf.json (minisign
# Ed25519 with OpenSSL 3+) and that it is bound to <version>.
#
# --publish additionally needs a clean tree with tag v<version> on HEAD, and
# then uploads everything with `gh release create v<version>` to the public
# repository gargankit90/dictation-app-releases (the updater's endpoint is
# its releases/latest/download/latest.json). Without --publish nothing leaves
# this Mac. No CI publishing, no tokens: gh uses the owner's own login.
set -euo pipefail
# Never trace: a traced line would print the key.
set +x

RELEASES_REPO=gargankit90/dictation-app-releases
KEY_SERVICE=dictation-app-updater-key
PASSWORD_SERVICE=dictation-app-updater-password
IDENTITY="dictation-app Dev"
APP=dictation-app

say() { printf 'release: %s\n' "$*"; }
die() {
  printf 'release: ERROR: %s\n' "$*" >&2
  exit 1
}

usage() {
  sed -n '2,5p' "$0" >&2
  exit 2
}

version=""
notes=""
publish=0
while [ $# -gt 0 ]; do
  case $1 in
    --publish) publish=1 ;;
    --notes)
      [ $# -ge 2 ] || die "--notes needs a text"
      notes=$2
      shift
      ;;
    -h | --help) usage ;;
    -*) die "unknown option $1" ;;
    *)
      [ -z "$version" ] || die "one version only"
      version=$1
      ;;
  esac
  shift
done
[ -n "$version" ] || usage
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] ||
  die "version must look like 1.2.3 or 1.2.3-alpha.1, not '$version'"
[ -n "$notes" ] || notes="$APP $version"

cd "$(git rev-parse --show-toplevel)"

# --- Checks that need no secret -------------------------------------------

conf=src-tauri/tauri.conf.json
for tool in jq git npm security codesign shasum; do
  command -v "$tool" >/dev/null || die "$tool is not installed"
done
[ "$(uname -s)" = Darwin ] || die "releases are built on macOS"

conf_version=$(jq -r .version "$conf")
npm_version=$(jq -r .version package.json)
cargo_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
[ "$conf_version" = "$version" ] ||
  die "$conf has version $conf_version, not $version (bump it in a PR first)"
[ "$npm_version" = "$version" ] || die "package.json has version $npm_version, not $version"
[ "$cargo_version" = "$version" ] || die "Cargo.toml has version $cargo_version, not $version"

pubkey=$(jq -r '.plugins.updater.pubkey // ""' "$conf")
[ -n "$pubkey" ] || die "$conf has no plugins.updater.pubkey"

if [ "$publish" = 1 ]; then
  [ -z "$(git status --porcelain)" ] || die "--publish needs a clean tree (commit or remove the changes)"
  grep -qxF "v$version" <<<"$(git tag --points-at HEAD)" ||
    die "--publish needs tag v$version on HEAD (git tag v$version, after the version PR is merged)"
  command -v gh >/dev/null || die "gh is not installed"
  gh auth status >/dev/null 2>&1 || die "gh is not logged in (gh auth login)"
fi

case $(uname -m) in
  arm64 | aarch64) arch=aarch64 ;;
  x86_64) arch=x86_64 ;;
  *) die "unknown architecture $(uname -m)" ;;
esac

grep -q "\"$IDENTITY\"" <<<"$(security find-identity -p codesigning 2>/dev/null || true)" ||
  die "no \"$IDENTITY\" certificate in the keychain (scripts/make-dev-cert.sh); a release is never ad hoc"

# An OpenSSL that can check a minisign signature (Ed25519 and BLAKE2b-512);
# macOS's own LibreSSL can't.
openssl_bin=""
for candidate in "${OPENSSL:-}" openssl /opt/homebrew/bin/openssl /usr/local/bin/openssl; do
  [ -n "$candidate" ] || continue
  command -v "$candidate" >/dev/null || continue
  # Captured first, so grep -q can't cut a pipe short (pipefail).
  if grep -qi 'blake2b512' <<<"$("$candidate" list -digest-algorithms 2>/dev/null || true)" &&
    grep -qi 'ed25519' <<<"$("$candidate" list -public-key-algorithms 2>/dev/null || true)"; then
    openssl_bin=$(command -v "$candidate")
    break
  fi
done
[ -n "$openssl_bin" ] || die "needs OpenSSL 3 or newer to check the signature (brew install openssl)"

scripts/check-licences.sh

# --- The signature check ----------------------------------------------------

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# b64d: base64 decode stdin (GNU and BSD).
b64d() { base64 -d 2>/dev/null || base64 -D; }

# verify_update <pubkey (tauri.conf form)> <file> <sig (the .sig's text)> <version>
# True when the minisign signature of <file> verifies against the key, its
# trusted comment (also signed) does, and it names <version>.
verify_update() {
  local pub=$1 file=$2 sig=$3 want=$4 dir=$work/verify alg comment
  rm -rf "$dir"
  mkdir -p "$dir"
  printf '%s' "$pub" | b64d >"$dir/pub.txt"
  sed -n 2p "$dir/pub.txt" | b64d >"$dir/pub.bin"
  printf '%s' "$sig" | b64d >"$dir/sig.txt"
  sed -n 2p "$dir/sig.txt" | b64d >"$dir/sig.bin"
  sed -n 4p "$dir/sig.txt" | b64d >"$dir/global.bin"
  comment=$(sed -n 3p "$dir/sig.txt")
  case $comment in
    "trusted comment: "*) comment=${comment#trusted comment: } ;;
    *)
      echo "release: the signature has no trusted comment" >&2
      return 1
      ;;
  esac
  [ "$(wc -c <"$dir/pub.bin" | tr -d ' ')" = 42 ] || {
    echo "release: the public key is not a minisign key" >&2
    return 1
  }
  [ "$(wc -c <"$dir/sig.bin" | tr -d ' ')" = 74 ] || {
    echo "release: the signature is not a minisign signature" >&2
    return 1
  }
  # Same key id (bytes 3-10 of both).
  if [ "$(tail -c +3 "$dir/pub.bin" | head -c 8 | od -An -tx1 | tr -d ' \n')" != \
    "$(tail -c +3 "$dir/sig.bin" | head -c 8 | od -An -tx1 | tr -d ' \n')" ]; then
    echo "release: the signature was made with another key" >&2
    return 1
  fi
  # The raw Ed25519 public key as DER (SubjectPublicKeyInfo).
  {
    printf '\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00'
    tail -c 32 "$dir/pub.bin"
  } >"$dir/pub.der"
  "$openssl_bin" pkey -pubin -inform DER -in "$dir/pub.der" -out "$dir/pub.pem" 2>/dev/null || return 1
  tail -c 64 "$dir/sig.bin" >"$dir/sig.raw"
  alg=$(head -c 2 "$dir/sig.bin")
  case $alg in
    ED) "$openssl_bin" dgst -blake2b512 -binary "$file" >"$dir/msg" ;;
    Ed) cp "$file" "$dir/msg" ;;
    *)
      echo "release: unknown signature algorithm" >&2
      return 1
      ;;
  esac
  "$openssl_bin" pkeyutl -verify -pubin -inkey "$dir/pub.pem" -rawin \
    -in "$dir/msg" -sigfile "$dir/sig.raw" >/dev/null 2>&1 || {
    echo "release: the signature does not verify against the public key" >&2
    return 1
  }
  {
    cat "$dir/sig.raw"
    printf '%s' "$comment"
  } >"$dir/global.msg"
  "$openssl_bin" pkeyutl -verify -pubin -inkey "$dir/pub.pem" -rawin \
    -in "$dir/global.msg" -sigfile "$dir/global.bin" >/dev/null 2>&1 || {
    echo "release: the signature's trusted comment does not verify" >&2
    return 1
  }
  grep -qxF "version:$want" <<<"$(tr '\t' '\n' <<<"$comment")" || {
    echo "release: the signature is not bound to version $want" >&2
    return 1
  }
}

# --- The build, with the key ------------------------------------------------

# Prints stdin with every occurrence of the two secrets replaced, line by
# line. Takes them from this shell's variables (no new process sees them).
redact() {
  local line
  while IFS= read -r line || [ -n "$line" ]; do
    [ -z "$secret_key" ] || line=${line//"$secret_key"/[redacted]}
    [ -z "$secret_password" ] || line=${line//"$secret_password"/[redacted]}
    printf '%s\n' "$line"
  done
}

secret_key=""
secret_password=""
# security prints the item's value with -w; its own errors name only the
# service. A missing password item is an error too: the key has one.
secret_key=$(security find-generic-password -s "$KEY_SERVICE" -w 2>/dev/null) ||
  die "no Keychain item with service $KEY_SERVICE"
secret_password=$(security find-generic-password -s "$PASSWORD_SERVICE" -w 2>/dev/null) ||
  die "no Keychain item with service $PASSWORD_SERVICE"
[ -n "$secret_key" ] || die "the Keychain item $KEY_SERVICE is empty"

bundle=target/release/bundle
out=target/release-out/v$version
# Nothing from an earlier run is left to be mistaken for this one's.
rm -rf "$bundle/macos" "$bundle/dmg" "$out"
# Tauri styles the DMG window by scripting Finder, which times out without
# Automation permission (an ssh session); CI=true skips that styling.
say "building $APP $version (release, signed \"$IDENTITY\"); this takes a few minutes"
build_status=0
CI=true TAURI_SIGNING_PRIVATE_KEY="$secret_key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$secret_password" \
  npm run tauri build -- --bundles app,dmg --config '{"bundle":{"createUpdaterArtifacts":true}}' \
  </dev/null 2>&1 | redact || build_status=$?
secret_key=""
secret_password=""
[ "$build_status" = 0 ] || die "the build failed (exit $build_status)"

# --- Check what was built ----------------------------------------------------

app_dir="$bundle/macos/$APP.app"
archive="$bundle/macos/$APP.app.tar.gz"
dmg=$(find "$bundle/dmg" -name '*.dmg' -print -quit 2>/dev/null || true)
[ -d "$app_dir" ] || die "no $app_dir"
[ -f "$archive" ] || die "no updater bundle $archive"
[ -s "$archive.sig" ] || die "no signature $archive.sig"
[ -n "$dmg" ] || die "no DMG under $bundle/dmg"

codesign --verify --deep --strict "$app_dir" || die "the app's signature does not verify"
# Captured first: `grep -q` would end the pipe early and, with pipefail,
# fail it.
signature=$(codesign -dvv "$app_dir" 2>&1)
grep -qx "Authority=$IDENTITY" <<<"$signature" || die "the app is not signed \"$IDENTITY\""
scripts/check-dmg.sh "$dmg"

verify_update "$pubkey" "$archive" "$(cat "$archive.sig")" "$version" ||
  die "the updater bundle's signature does not verify against $conf's public key"
say "the updater bundle's signature verifies against the public key and names $version"

# --- The release folder ------------------------------------------------------

mkdir -p "$out"
base=${APP}_${version}_${arch}
cp "$dmg" "$out/$base.dmg"
cp "$archive" "$out/$base.app.tar.gz"
cp "$archive.sig" "$out/$base.app.tar.gz.sig"
cp THIRD-PARTY-LICENSES.txt "$out/"
cp docs/TESTERS.md "$out/README.md"
scripts/make-latest-json.sh "$version" "$notes" "$out/$base.app.tar.gz" "$out/$base.app.tar.gz.sig" \
  "https://github.com/$RELEASES_REPO/releases/download/v$version/$base.app.tar.gz" >"$out/latest.json"
# The manifest's signature is the one that was just checked.
verify_update "$pubkey" "$out/$base.app.tar.gz" "$(jq -r '.platforms[].signature' "$out/latest.json")" "$version" ||
  die "latest.json's signature does not verify"
(cd "$out" && shasum -a 256 -- "$base.dmg" "$base.app.tar.gz" latest.json THIRD-PARTY-LICENSES.txt >SHA256SUMS)
say "built into $out:"
ls -l "$out"

if [ "$publish" = 0 ]; then
  say "dry run: nothing uploaded (add --publish to release v$version to $RELEASES_REPO)"
  exit 0
fi

say "publishing v$version to $RELEASES_REPO"
gh release create "v$version" --repo "$RELEASES_REPO" \
  --title "$APP $version" --notes "$notes" \
  "$out/$base.dmg" "$out/$base.app.tar.gz" "$out/$base.app.tar.gz.sig" \
  "$out/latest.json" "$out/README.md" "$out/THIRD-PARTY-LICENSES.txt" "$out/SHA256SUMS"
say "published: https://github.com/$RELEASES_REPO/releases/tag/v$version"
