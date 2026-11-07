#!/usr/bin/env bash
# Builds a tester release of Vocet on the owner's Mac (M5.4):
#
#   scripts/release.sh <version> [--notes <text>]             # dry run
#   scripts/release.sh <version> [--notes <text>] --publish   # and upload
#   (also --notarize, or --no-notarize for a dry test signing run)
#
# The release notes (the GitHub release's body and the "Update available"
# text in latest.json) are the "## [<version>]" section of CHANGELOG.md, minus
# its heading, unless --notes gives other text. A version with no section (or
# an empty one) is refused before anything is built.
#
# Both build the signed release and put into target/release-out/v<version>/:
#
#   Vocet_<version>_<arch>.dmg          the installer testers get
#   Vocet_<version>_<arch>.app.tar.gz   the updater bundle
#   Vocet_<version>_<arch>.app.tar.gz.sig
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
# Which certificate signs (N1): RELEASE_IDENTITY in
# scripts/release-signing.conf, or `dictation-app Dev` while that is empty. A
# configured identity that is missing or expired stops the release: it never
# falls back, because a release signed by another certificate costs every
# tester their permissions. A "Developer ID Application:" identity is
# notarised and stapled, with the Keychain profile RELEASE_NOTARY_PROFILE
# names (`xcrun notarytool store-credentials`); this script never sees what is
# in that profile, and notarytool's output never reaches the terminal, only
# the status it reports and Apple's list of problems. The environment
# variables RELEASE_IDENTITY and RELEASE_NOTARY_PROFILE override the file for
# a dry run; --publish refuses them, so what is published is what was merged.
#
# Notarising changes the app (the ticket is stapled into it), so the updater
# bundle and the DMG are made again from the stapled app. Both the updater
# bundle and the DMG are then checked to hold an app with the same signature,
# and the bundle's minisign signature is checked as before.
#
# Before anything is written, the script checks that <version> is the one in
# src-tauri/tauri.conf.json, package.json and Cargo.toml. After the build it
# verifies the .sig against the public key in tauri.conf.json (minisign
# Ed25519 with OpenSSL 3+) and that it is bound to <version>.
#
# --publish additionally needs a clean tree whose HEAD is on origin's default
# branch (merged through a PR), and a tag v<version> that exists neither here
# nor on origin: tags are immutable in this repository (a ruleset blocks
# moving or deleting them), so a failed publish needs a new version. It then
# uploads everything with `gh release create v<version> --target <the built
# commit>` to gargankit90/dictation-app, which creates the tag on that commit
# (the updater's endpoint is releases/latest/download/latest.json there).
# Without --publish nothing leaves this Mac. No CI publishing, no tokens: gh
# uses the owner's own login.
set -euo pipefail
# Never trace: a traced line would print the key.
set +x

RELEASES_REPO=gargankit90/dictation-app
KEY_SERVICE=dictation-app-updater-key
PASSWORD_SERVICE=dictation-app-updater-password
# Optional telemetry (A1, A2): the PostHog project key and the Sentry DSN, also
# generic passwords in the login Keychain. A missing one only means the build
# sends nothing of that kind.
POSTHOG_SERVICE=dictation-app-posthog-key
SENTRY_SERVICE=dictation-app-sentry-dsn
# The certificate a release is signed with until release-signing.conf names
# another.
FALLBACK_IDENTITY="dictation-app Dev"
SIGNING_CONF=scripts/release-signing.conf
DEVELOPER_ID_PREFIX="Developer ID Application:"
ENTITLEMENTS=src-tauri/Entitlements.plist
APP=Vocet

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
notarize_flag=""
while [ $# -gt 0 ]; do
  case $1 in
    --publish) publish=1 ;;
    --notarize) notarize_flag=yes ;;
    --no-notarize) notarize_flag=no ;;
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

# changelog_notes <version> <file>: the body of the "## [<version>]" (or
# "## <version>") section: everything up to the next "## " heading or the
# link definitions at the end, without surrounding blank lines.
changelog_notes() {
  awk -v v="$1" '
    /^## / {
      if (on) exit
      h = $2
      gsub(/[][]/, "", h)
      if (h == v) on = 1
      next
    }
    on && /^\[[^]]+\]: / { exit }
    on { lines[++n] = $0 }
    END {
      s = 1
      while (s <= n && lines[s] ~ /^[ \t]*$/) s++
      while (n >= s && lines[n] ~ /^[ \t]*$/) n--
      for (i = s; i <= n; i++) print lines[i]
    }
  ' "$2"
}

cd "$(git rev-parse --show-toplevel)"

# --- Checks that need no secret -------------------------------------------

conf=src-tauri/tauri.conf.json
for tool in jq git npm security codesign shasum hdiutil; do
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

# The notes: --notes, else this version's CHANGELOG section (before the build
# and the Keychain, so a missing section costs nothing).
if [ -z "$notes" ]; then
  [ -f CHANGELOG.md ] || die "no CHANGELOG.md (write it, or pass --notes <text>)"
  notes=$(changelog_notes "$version" CHANGELOG.md)
  [ -n "$notes" ] || die "CHANGELOG.md has no \"## [$version]\" section with text; write it first (or pass --notes <text>)"
fi

pubkey=$(jq -r '.plugins.updater.pubkey // ""' "$conf")
[ -n "$pubkey" ] || die "$conf has no plugins.updater.pubkey"

built_commit=""
if [ "$publish" = 1 ]; then
  [ -z "$(git status --porcelain)" ] || die "--publish needs a clean tree (commit or remove the changes)"
  git fetch --quiet origin || die "could not fetch origin to check the commit and tag"
  default_ref=$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null || true)
  [ -n "$default_ref" ] || default_ref=origin/main
  built_commit=$(git rev-parse HEAD)
  git merge-base --is-ancestor "$built_commit" "$default_ref" 2>/dev/null ||
    die "--publish needs HEAD ($built_commit) on $default_ref (merge the version PR, then git switch to the merged commit)"
  tag_error="tag v$version already exists; tags are immutable here (a ruleset blocks moving or deleting them), so a failed or repeated publish needs a new version, not a re-tag"
  [ -z "$(git tag --list "v$version")" ] || die "$tag_error"
  [ -z "$(git ls-remote --tags origin "refs/tags/v$version")" ] || die "$tag_error"
  command -v gh >/dev/null || die "gh is not installed"
  gh auth status >/dev/null 2>&1 || die "gh is not logged in (gh auth login)"
fi

case $(uname -m) in
  arm64 | aarch64) arch=aarch64 ;;
  x86_64) arch=x86_64 ;;
  *) die "unknown architecture $(uname -m)" ;;
esac

# --- Which certificate, and whether to notarise (nothing secret) ---------------

# conf_value <name>: the value of NAME=value in the signing configuration. The
# file is read, not run.
conf_value() {
  [ -f "$SIGNING_CONF" ] || return 0
  sed -n "s/^$1=//p" "$SIGNING_CONF" | tail -n 1 | sed -e 's/^"\(.*\)"$/\1/'
}

if [ "$publish" = 1 ] && { [ -n "${RELEASE_IDENTITY:-}" ] || [ -n "${RELEASE_NOTARY_PROFILE:-}" ]; }; then
  die "--publish uses $SIGNING_CONF as merged: unset RELEASE_IDENTITY and RELEASE_NOTARY_PROFILE"
fi
identity=${RELEASE_IDENTITY:-$(conf_value RELEASE_IDENTITY)}
notary_profile=${RELEASE_NOTARY_PROFILE:-$(conf_value RELEASE_NOTARY_PROFILE)}
fallback=0
if [ -z "$identity" ]; then
  identity=$FALLBACK_IDENTITY
  fallback=1
fi
is_developer_id=0
case $identity in "$DEVELOPER_ID_PREFIX"*) is_developer_id=1 ;; esac

notarize=$is_developer_id
case $notarize_flag in
  yes)
    [ "$is_developer_id" = 1 ] ||
      die "--notarize needs a Developer ID identity; Apple does not notarise \"$identity\" (docs/releasing.md, \"Developer ID\")"
    ;;
  no)
    [ "$publish" = 0 ] || die "--no-notarize is for a dry test signing run, not for --publish"
    notarize=0
    ;;
esac

identities=$(security find-identity -p codesigning 2>/dev/null || true)
grep -qF "\"$identity\"" <<<"$identities" || {
  [ "$fallback" = 1 ] &&
    die "no \"$identity\" certificate in the keychain (scripts/make-dev-cert.sh); a release is never ad hoc"
  die "no \"$identity\" certificate in the keychain, and $SIGNING_CONF asks for it: not falling back to \"$FALLBACK_IDENTITY\" (docs/releasing.md, \"Developer ID\")"
}
if [ "$is_developer_id" = 1 ]; then
  # -v lists only what can sign right now: an expired or revoked certificate
  # is in the first list but not in this one.
  valid_identities=$(security find-identity -v -p codesigning 2>/dev/null || true)
  grep -qF "\"$identity\"" <<<"$valid_identities" ||
    die "the certificate \"$identity\" is in the keychain but can't sign (expired or revoked?); see \"When the certificate expires\" in docs/releasing.md"
fi
if [ "$fallback" = 1 ]; then
  say "signing with the fallback \"$identity\": not notarised, testers clear the quarantine flag"
  if grep -qF "\"$DEVELOPER_ID_PREFIX" <<<"$identities"; then
    say "NOTE: this keychain has a Developer ID certificate, but RELEASE_IDENTITY in $SIGNING_CONF is empty (docs/releasing.md, \"Developer ID\")"
  fi
else
  say "signing with \"$identity\""
fi
[ "$is_developer_id" = 0 ] || [ "$notarize" = 1 ] ||
  say "WARNING: --no-notarize: macOS will refuse this build on a tester's Mac until it is notarised"

# Checked before the build, so a missing credential costs a second, not a
# build. notarytool's output is dropped: only its exit status is used here.
if [ "$notarize" = 1 ]; then
  for tool in xcrun spctl ditto; do
    command -v "$tool" >/dev/null || die "$tool is not installed"
  done
  [ -n "$notary_profile" ] ||
    die "notarisation is needed for \"$identity\", but RELEASE_NOTARY_PROFILE in $SIGNING_CONF is empty (docs/releasing.md, \"The notarisation credential\")"
  xcrun notarytool history --keychain-profile "$notary_profile" >/dev/null 2>&1 ||
    die "notarisation is needed, but the Keychain profile \"$notary_profile\" doesn't work (missing, expired, or Apple can't be reached): check with \`xcrun notarytool history --keychain-profile $notary_profile\`, and see docs/releasing.md, \"The notarisation credential\""
  say "the notarisation credential \"$notary_profile\" works"
fi

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
    [ -z "$posthog_key" ] || line=${line//"$posthog_key"/[redacted]}
    [ -z "$sentry_dsn" ] || line=${line//"$sentry_dsn"/[redacted]}
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

# The telemetry items are optional: a missing one is a loud warning, not a
# failure. The values are never printed, only whether they were found.
posthog_key=$(security find-generic-password -s "$POSTHOG_SERVICE" -w 2>/dev/null) || posthog_key=""
sentry_dsn=$(security find-generic-password -s "$SENTRY_SERVICE" -w 2>/dev/null) || sentry_dsn=""
if [ -n "$posthog_key" ]; then
  say "usage analytics: found $POSTHOG_SERVICE in the Keychain"
else
  say "WARNING: no Keychain item $POSTHOG_SERVICE, so this build sends NO usage analytics (docs/releasing.md says how to add it)"
fi
if [ -n "$sentry_dsn" ]; then
  say "error reports: found $SENTRY_SERVICE in the Keychain"
else
  say "WARNING: no Keychain item $SENTRY_SERVICE, so this build sends NO error reports (docs/releasing.md says how to add it)"
fi

bundle=target/release/bundle
out=target/release-out/v$version
# Nothing from an earlier run is left to be mistaken for this one's.
rm -rf "$bundle/macos" "$bundle/dmg" "$out"
# Tauri styles the DMG window by scripting Finder, which times out without
# Automation permission (an ssh session); CI=true skips that styling.
say "building $APP $version (release, signed \"$identity\"); this takes a few minutes"
build_config=$(jq -nc --arg identity "$identity" \
  '{bundle: {createUpdaterArtifacts: true, macOS: {signingIdentity: $identity, hardenedRuntime: true}}}')
build_status=0
# A subshell, so the telemetry values reach only this build; whatever the
# caller exported under these names is dropped first.
(
  unset DICTATION_POSTHOG_KEY DICTATION_SENTRY_DSN
  # Tauri signs and notarises by itself if it finds these. This script does
  # that, with a Keychain profile; a password in the environment stays out.
  unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH \
    APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_SIGNING_IDENTITY APPLE_PROVIDER_SHORT_NAME
  if [ -n "$posthog_key" ]; then export DICTATION_POSTHOG_KEY="$posthog_key"; fi
  if [ -n "$sentry_dsn" ]; then export DICTATION_SENTRY_DSN="$sentry_dsn"; fi
  CI=true TAURI_SIGNING_PRIVATE_KEY="$secret_key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$secret_password" \
    npm run tauri build -- --bundles app,dmg --config "$build_config"
) </dev/null 2>&1 | redact || build_status=$?
posthog_key=""
sentry_dsn=""
if [ "$build_status" != 0 ]; then
  secret_key=""
  secret_password=""
  die "the build failed (exit $build_status)"
fi

app_dir="$bundle/macos/$APP.app"
archive="$bundle/macos/$APP.app.tar.gz"
dmg=$(find "$bundle/dmg" -name '*.dmg' -print -quit 2>/dev/null || true)
[ -d "$app_dir" ] || die "no $app_dir"
[ -f "$archive" ] || die "no updater bundle $archive"
[ -s "$archive.sig" ] || die "no signature $archive.sig"
[ -n "$dmg" ] || die "no DMG under $bundle/dmg"

# signature_details <path>: codesign's description of a signature, and only
# that. Captured first: `grep -q` would end a pipe early and, with pipefail,
# fail it.
signature_details() { codesign -dvvv "$1" 2>&1 || true; }

# The one signature everything must carry: the identity, hardened runtime,
# the entitlements in the repository and nothing else.
check_app_signature() {
  local app=$1 details want have
  codesign --verify --deep --strict "$app" || die "$app's signature does not verify"
  details=$(signature_details "$app")
  grep -qxF "Authority=$identity" <<<"$details" || die "$app is not signed \"$identity\""
  grep -q '^CodeDirectory .*flags=[^ ]*runtime' <<<"$details" ||
    die "$app is not signed with the hardened runtime"
  want=$(grep -o 'com\.apple\.[a-z.-]*' "$ENTITLEMENTS" | sort -u)
  have=$(codesign -d --entitlements - "$app" 2>/dev/null | grep -o 'com\.apple\.[a-z.-]*' | sort -u || true)
  [ "$have" = "$want" ] ||
    die "$app's entitlements are [$(tr '\n' ' ' <<<"$have")], not the ones in $ENTITLEMENTS: [$(tr '\n' ' ' <<<"$want")]"
  if [ "$is_developer_id" = 1 ]; then
    # Apple refuses a signature with no secure timestamp.
    grep -q '^Timestamp=' <<<"$details" || die "$app's signature has no secure timestamp; notarisation would refuse it"
  fi
}

# same_signature <app> <what>: <app> is the same signed code as the built app.
same_signature() {
  check_app_signature "$1"
  [ "$(sed -n 's/^CDHash=//p' <<<"$(signature_details "$1")")" = "$built_cdhash" ] ||
    die "$2 holds different code from the app that was built and checked"
}

# archive_check <archive>: the app in the updater bundle is the checked one.
archive_check() {
  local dir=$work/archive-check
  rm -rf "$dir"
  mkdir -p "$dir"
  tar -xzf "$1" -C "$dir"
  [ -d "$dir/$APP.app" ] || die "the updater bundle has no $APP.app at its top"
  same_signature "$dir/$APP.app" "the updater bundle"
  if [ "$notarize" = 1 ]; then
    xcrun stapler validate "$dir/$APP.app" >/dev/null 2>&1 || die "the app in the updater bundle has no valid ticket"
  fi
}

# dmg_check <dmg>: the app in the DMG is the checked one.
dmg_check() {
  local mount=$work/dmg-check
  mkdir -p "$mount"
  hdiutil attach "$1" -nobrowse -readonly -noautoopen -mountpoint "$mount" -quiet || die "could not mount $1"
  ( same_signature "$mount/$APP.app" "the DMG" ) || {
    hdiutil detach "$mount" -quiet -force || true
    exit 1
  }
  if [ "$notarize" = 1 ] && ! xcrun stapler validate "$mount/$APP.app" >/dev/null 2>&1; then
    hdiutil detach "$mount" -quiet -force || true
    die "the app in the DMG has no valid ticket"
  fi
  hdiutil detach "$mount" -quiet -force || die "could not unmount $1"
}

# notary_submit <file> <label>: sends <file> to Apple and waits for the answer.
# Only the status and Apple's list of problems are shown, never notarytool's
# raw output.
notary_submit() {
  local file=$1 label=$2 answer status id
  answer=$work/notary-$label.json
  say "notarising the $label with Apple; this usually takes a few minutes"
  xcrun notarytool submit "$file" --keychain-profile "$notary_profile" --wait --timeout 1h \
    --output-format json >"$answer" 2>/dev/null || true
  status=$(jq -r '.status // empty' "$answer" 2>/dev/null || true)
  id=$(jq -r '.id // empty' "$answer" 2>/dev/null || true)
  [ -n "$status" ] ||
    die "notarytool gave no answer for the $label (check \`xcrun notarytool history --keychain-profile $notary_profile\`); nothing was published"
  if [ "$status" != Accepted ]; then
    if [ -n "$id" ] && xcrun notarytool log "$id" --keychain-profile "$notary_profile" "$work/notary-$label-log.json" >/dev/null 2>&1; then
      jq -r '.issues[]? | "release:   \(.path // "?"): \(.message // "?")"' "$work/notary-$label-log.json" >&2 || true
    fi
    die "Apple's answer for the $label is \"$status\", not Accepted (submission ${id:-unknown}); nothing was published"
  fi
  say "Apple accepted the $label (submission $id)"
}

# staple <path> <label>: attaches and checks the ticket, offline from then on.
staple() {
  xcrun stapler staple "$1" >/dev/null 2>&1 || die "could not staple the ticket to the $2"
  xcrun stapler validate "$1" >/dev/null 2>&1 || die "the ticket stapled to the $2 doesn't validate"
}

# assessed_as_notarized <what> <spctl args...>: Gatekeeper accepts it, as
# notarised: the check a tester's Mac makes.
assessed_as_notarized() {
  local what=$1 said
  shift
  said=$(spctl --assess --verbose=2 "$@" 2>&1) || die "Gatekeeper refuses the $what: $said"
  grep -q 'source=Notarized Developer ID' <<<"$said" || die "Gatekeeper doesn't see the $what as notarised: $said"
  say "Gatekeeper accepts the $what (Notarized Developer ID)"
}

# rebuild_archive <app> <archive>: the updater bundle, as Tauri lays it out,
# from the stapled app, signed again with the update key. Its filtered output
# keeps the key off the terminal as the build's does.
rebuild_archive() {
  local status=0
  rm -f "$2" "$2.sig"
  # No AppleDouble files: they would land in the installed bundle and break
  # its signature.
  COPYFILE_DISABLE=1 tar -C "$(dirname "$1")" -czf "$2" "$(basename "$1")"
  (
    TAURI_SIGNING_PRIVATE_KEY="$secret_key" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$secret_password" \
      npm run tauri -- signer sign --app-version "$version" "$2"
  ) </dev/null 2>&1 | redact >/dev/null || status=$?
  [ "$status" = 0 ] || die "could not sign the updater bundle again (exit $status)"
  [ -s "$2.sig" ] || die "signing the updater bundle again left no $2.sig"
}

# rebuild_dmg <app> <dmg>: the same DMG with the stapled app inside, then
# signed. Through a writable copy, so its layout and icon stay as Tauri made them.
rebuild_dmg() {
  local rw=$work/rw.dmg mount=$work/rw-mount done=$work/stapled.dmg
  rm -f "$rw" "$done"
  mkdir -p "$mount"
  hdiutil convert "$2" -format UDRW -quiet -o "$rw" || die "could not open the DMG for the stapled app"
  hdiutil attach "$rw" -nobrowse -noautoopen -mountpoint "$mount" -quiet || die "could not mount the DMG for the stapled app"
  if ! { rm -rf "${mount:?}/$APP.app" && ditto "$1" "$mount/$APP.app"; }; then
    hdiutil detach "$mount" -quiet -force || true
    die "could not put the stapled app into the DMG"
  fi
  hdiutil detach "$mount" -quiet || hdiutil detach "$mount" -quiet -force || die "could not unmount the DMG"
  hdiutil convert "$rw" -format UDZO -imagekey zlib-level=9 -quiet -o "$done" || die "could not compress the DMG"
  mv "$done" "$2"
  codesign --force --sign "$identity" --timestamp "$2" || die "could not sign the DMG"
}

# --- Check what was built, and notarise it -------------------------------------

check_app_signature "$app_dir"
built_cdhash=$(sed -n 's/^CDHash=//p' <<<"$(signature_details "$app_dir")")
[ -n "$built_cdhash" ] || die "codesign reports no CDHash for the app"
scripts/check-dmg.sh "$dmg"

if [ "$notarize" = 1 ]; then
  app_zip=$work/$APP.zip
  ditto -c -k --keepParent "$app_dir" "$app_zip"
  notary_submit "$app_zip" app
  staple "$app_dir" app
  assessed_as_notarized app --type execute "$app_dir"
  rebuild_archive "$app_dir" "$archive"
  rebuild_dmg "$app_dir" "$dmg"
  notary_submit "$dmg" DMG
  staple "$dmg" DMG
  assessed_as_notarized DMG --type open --context context:primary-signature "$dmg"
  scripts/check-dmg.sh "$dmg"
fi
secret_key=""
secret_password=""
archive_check "$archive"
dmg_check "$dmg"
if [ "$notarize" = 1 ]; then
  say "the updater bundle and the DMG hold the same notarised app, signed \"$identity\""
else
  say "the updater bundle and the DMG hold the same app, signed \"$identity\""
fi

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
if [ "$notarize" = 1 ]; then
  say "signed \"$identity\", notarised and stapled"
else
  say "signed \"$identity\", NOT notarised"
fi
say "built into $out:"
ls -l "$out"

if [ "$publish" = 0 ]; then
  say "dry run: nothing uploaded (add --publish to release v$version to $RELEASES_REPO)"
  exit 0
fi

# The build must still be of the checked commit.
[ "$(git rev-parse HEAD)" = "$built_commit" ] && [ -z "$(git status --porcelain)" ] ||
  die "the tree or HEAD changed during the build; nothing was published"
say "publishing v$version ($built_commit) to $RELEASES_REPO"
gh release create "v$version" --repo "$RELEASES_REPO" --target "$built_commit" \
  --title "$APP $version" --notes "$notes" \
  "$out/$base.dmg" "$out/$base.app.tar.gz" "$out/$base.app.tar.gz.sig" \
  "$out/latest.json" "$out/README.md" "$out/THIRD-PARTY-LICENSES.txt" "$out/SHA256SUMS"
say "published: https://github.com/$RELEASES_REPO/releases/tag/v$version"
