#!/usr/bin/env bash
# Proves scripts/release.sh (M5.4 criterion 2) without the owner's key, a Mac
# or the network. Part of the quality gate.
#
# It runs the script in a throwaway git repository with stub `security`
# (the Keychain: hands out a THROWAWAY key pair made here), `npm` (the build:
# makes a fake bundle and signs it with the real `tauri signer sign`),
# `codesign`, `uname` and `gh` (records what it was asked; uploads nothing),
# and checks that:
#
#   - a dry run makes the bundle, its .sig and a latest.json whose signature
#     verifies against the public key, and never calls gh;
#   - neither the key nor its password is ever printed, even when the build
#     prints them or the script runs under `bash -x`, and neither is written
#     to a file in the repository;
#   - a bundle signed with another key, for another version, or changed after
#     it was signed, is refused;
#   - --publish is refused without a clean tree, with HEAD not on origin's
#     default branch, or with tag v<version> already here or on origin
#     (tags are immutable), before the Keychain is read; otherwise it calls
#     `gh release create v<version> --repo gargankit90/dictation-app
#     --target <HEAD>`;
#   - a version that isn't the one in tauri.conf.json, or a missing Keychain
#     item, is refused;
#   - the certificate comes from scripts/release-signing.conf: `dictation-app
#     Dev` while it is empty, never a fallback once it names one; a Developer ID
#     release is notarised and stapled (a fake notarytool, stapler and spctl:
#     what real Apple tools do is not tested here, see docs/releasing.md), and
#     stops with a clear message without its credential; the updater bundle and
#     the DMG are made again around the stapled app and checked; neither the
#     build nor the terminal ever sees a credential.
set -euo pipefail

root=$(git rev-parse --show-toplevel)
# Under git's pre-push run of the gate, git exports GIT_DIR and friends; left
# set, the throwaway repository's commits and tags below would land in this
# one. Every git command from here on finds its repository by its folder.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY \
  GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_COMMON_DIR GIT_PREFIX GIT_NAMESPACE
tauri_cli=$root/node_modules/.bin/tauri
[ -x "$tauri_cli" ] || {
  echo "test-release: no $tauri_cli (npm ci first)" >&2
  exit 1
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stubs=$work/stubs
calls=$work/calls
mkdir -p "$stubs" "$calls"

fail() {
  echo "test-release: FAIL: $*" >&2
  [ ! -f "$work/out.txt" ] || sed 's/^/  | /' "$work/out.txt" >&2
  exit 1
}
pass() { echo "test-release: ok: $*"; }

# --- Throwaway keys (never the owner's) ----------------------------------------

VERSION=0.3.0
KEY_PASSWORD=pw-$RANDOM-$RANDOM-throwaway
"$tauri_cli" signer generate --ci -w "$work/test.key" -p "$KEY_PASSWORD" >/dev/null 2>&1
"$tauri_cli" signer generate --ci -w "$work/other.key" -p "" >/dev/null 2>&1
POSTHOG=phc_throwaway$RANDOM$RANDOM
SENTRY=https://throwaway$RANDOM@o1.ingest.us.sentry.io/$RANDOM
KEY=$(cat "$work/test.key")
PUBKEY=$(cat "$work/test.key.pub")
[ -n "$KEY" ] && [ -n "$PUBKEY" ] || fail "could not make a throwaway key pair"

# --- Stubs --------------------------------------------------------------------

cat >"$stubs/security" <<'STUB'
#!/usr/bin/env bash
echo "security $*" >>"$STUB_CALLS/security"
case $1 in
  find-identity)
    # -v: only the ones that can sign now.
    if [ "$2" = -v ] && [ -n "${FAKE_VALID_IDENTITIES+x}" ]; then
      printf '%s\n' "$FAKE_VALID_IDENTITIES"
    else
      printf '%s\n' "${FAKE_IDENTITIES:-  1) 0123456789ABCDEF \"dictation-app Dev\"}"
    fi
    ;;
  find-generic-password)
    service=""
    while [ $# -gt 0 ]; do [ "$1" = -s ] && service=$2; shift; done
    case $service in
      dictation-app-updater-key) [ -n "${FAKE_KEY_MISSING:-}" ] && exit 44; printf '%s\n' "$FAKE_KEY" ;;
      dictation-app-updater-password) printf '%s\n' "$FAKE_PASSWORD" ;;
      dictation-app-posthog-key) [ -n "${FAKE_NO_TELEMETRY:-}${FAKE_POSTHOG_MISSING:-}" ] && exit 44; printf '%s\n' "$FAKE_POSTHOG" ;;
      dictation-app-sentry-dsn) [ -n "${FAKE_NO_TELEMETRY:-}${FAKE_SENTRY_MISSING:-}" ] && exit 44; printf '%s\n' "$FAKE_SENTRY" ;;
      *) exit 44 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB
# codesign: describes the app as the test says it is signed. FAKE_WRONG_WHERE
# (archive or dmg) makes that copy of the app look different.
cat >"$stubs/codesign" <<'STUB'
#!/usr/bin/env bash
echo "codesign $*" >>"$STUB_CALLS/codesign"
path=${*: -1}
authority=${FAKE_AUTHORITY:-dictation-app Dev}
cdhash=abc123
where=build
case $path in
  */archive-check/*) where=archive ;;
  */dmg-check/*) where=dmg ;;
esac
if [ "$where" = "${FAKE_WRONG_WHERE:-}" ]; then
  authority="Someone Else"
fi
if [ "$where" = "${FAKE_OTHER_CODE_WHERE:-}" ]; then
  cdhash=def456
fi
case " $* " in
  *" -dvvv "*)
    if [ -n "${FAKE_ADHOC:-}" ]; then echo "Signature=adhoc" >&2; else
      flags='0x10000(runtime)'
      [ -z "${FAKE_NO_RUNTIME:-}" ] || flags='0x0(none)'
      printf 'Executable=x\nCodeDirectory v=20500 size=1 flags=%s hashes=1+7 location=embedded\nCDHash=%s\nAuthority=%s\n' "$flags" "$cdhash" "$authority" >&2
      [ -n "${FAKE_NO_TIMESTAMP:-}" ] || printf 'Timestamp=Nov 6, 2026\n' >&2
      printf 'TeamIdentifier=not set\n' >&2
    fi
    ;;
  *" --entitlements "*)
    printf '<plist><dict><key>com.apple.security.device.audio-input</key><true/>' >&2
    if [ -n "${FAKE_EXTRA_ENTITLEMENT:-}" ]; then
      printf '<key>com.apple.security.cs.disable-library-validation</key><true/>'
    fi
    printf '<key>com.apple.security.device.audio-input</key><true/></dict></plist>\n'
    ;;
esac
exit 0
STUB
# The notarisation tools, none of them Apple's. A ticket is a file in the app
# (a stapled app shows it wherever it is copied), or a <dmg>.stapled marker.
# FAKE_NOTARY_LEAK makes notarytool print what a tool must never get on screen.
cat >"$stubs/xcrun" <<'STUB'
#!/usr/bin/env bash
echo "xcrun $*" >>"$STUB_CALLS/xcrun"
tool=$1
shift
case $tool in
  notarytool)
    if [ -n "${FAKE_NOTARY_LEAK:-}" ]; then
      echo "notarytool: password=$FAKE_NOTARY_LEAK" >&2
      [ "$1" = submit ] || echo "notarytool: password=$FAKE_NOTARY_LEAK"
    fi
    case $1 in
      history) [ -z "${FAKE_NO_NOTARY_CREDENTIAL:-}" ] || exit 1 ;;
      submit)
        [ -e "$2" ] || exit 1
        printf '%s\n' "$2" >>"$STUB_CALLS/notary-submitted"
        [ -z "${FAKE_NOTARY_NO_ANSWER:-}" ] || exit 1
        status=Accepted
        case $2 in
          *.zip) status=${FAKE_APP_STATUS:-Accepted} ;;
          *) status=${FAKE_DMG_STATUS:-Accepted} ;;
        esac
        printf '{"id":"sub-1","status":"%s","message":"password=%s"}\n' "$status" "${FAKE_NOTARY_LEAK:-}"
        ;;
      log)
        printf '{"issues":[{"path":"Vocet.app/x","message":"The binary is not signed."}]}\n' >"${*: -1}"
        ;;
    esac
    ;;
  stapler)
    action=$1
    target=$2
    case $action in
      staple)
        [ -z "${FAKE_STAPLE_FAIL:-}" ] || exit 65
        if [ -d "$target" ]; then echo ticket >"$target/Contents/CodeResources"; else echo ticket >"$target.stapled"; fi
        ;;
      validate)
        [ -z "${FAKE_NO_TICKET_WHERE:-}" ] || case $target in */"$FAKE_NO_TICKET_WHERE"/*) exit 65 ;; esac
        if [ -d "$target" ]; then [ -e "$target/Contents/CodeResources" ]; else [ -e "$target.stapled" ]; fi
        ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB
cat >"$stubs/spctl" <<'STUB'
#!/usr/bin/env bash
echo "spctl $*" >>"$STUB_CALLS/spctl"
if [ -n "${FAKE_SPCTL_REJECT:-}" ]; then
  echo "rejected source=Unnotarized Developer ID"
  exit 3
fi
if [ -n "${FAKE_SPCTL_UNNOTARIZED:-}" ]; then
  echo "accepted source=Developer ID"
  exit 0
fi
echo "accepted source=Notarized Developer ID"
STUB
cat >"$stubs/ditto" <<'STUB'
#!/usr/bin/env bash
echo "ditto $*" >>"$STUB_CALLS/ditto"
if [ "$1" = -c ]; then
  src=${*: -2:1}
  dst=${*: -1}
  tar -C "$(dirname "$src")" -cf "$dst" "$(basename "$src")"
else
  cp -R "$1" "$2"
fi
STUB
# A DMG here is a tar of its contents. A writable mount is packed up again when
# it is detached.
cat >"$stubs/hdiutil" <<'STUB'
#!/usr/bin/env bash
echo "hdiutil $*" >>"$STUB_CALLS/hdiutil"
action=$1
shift
case $action in
  convert)
    src=$1
    dst=""
    while [ $# -gt 0 ]; do [ "$1" = -o ] && dst=$2; shift; done
    cp "$src" "$dst"
    ;;
  attach)
    image=$1
    mount=""
    readonly=0
    while [ $# -gt 0 ]; do
      [ "$1" = -mountpoint ] && mount=$2
      [ "$1" = -readonly ] && readonly=1
      shift
    done
    mkdir -p "$mount"
    tar -xf "$image" -C "$mount"
    printf '%s\t%s\t%s\n' "$mount" "$readonly" "$image" >>"$STUB_CALLS/mounts"
    ;;
  detach)
    mount=$1
    line=$(grep -F "$mount"$'\t' "$STUB_CALLS/mounts" | tail -n 1)
    readonly=$(cut -f2 <<<"$line")
    image=$(cut -f3 <<<"$line")
    if [ "$readonly" = 0 ]; then
      tar -C "$mount" -cf "$image" .
    fi
    rm -rf "${mount:?}"/* "${mount:?}"/.[!.]* 2>/dev/null || true
    ;;
esac
exit 0
STUB
cat >"$stubs/uname" <<'STUB'
#!/usr/bin/env bash
case ${1:-} in
  -m) echo arm64 ;;
  *) echo Darwin ;;
esac
STUB
cat >"$stubs/gh" <<'STUB'
#!/usr/bin/env bash
echo "gh $*" >>"$STUB_CALLS/gh"
# Keep the --notes text apart: it spans lines.
while [ $# -gt 0 ]; do
  [ "$1" = --notes ] && printf '%s' "$2" >"$STUB_CALLS/gh-notes"
  shift
done
exit 0
STUB
# The build: an app, a DMG and the updater bundle, signed with whatever key
# the script handed over (by the real tauri signer, bound to a version).
cat >"$stubs/npm" <<'STUB'
#!/usr/bin/env bash
echo "npm $*" >>"$STUB_CALLS/npm"
# Signing the bundle again around a stapled app (N1).
if [[ " $* " == *" signer sign "* ]]; then
  [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && echo key >>"$STUB_CALLS/npm-resign-got-key"
  if [ -n "${FAKE_LEAK:-}" ]; then
    echo "debug: key=$TAURI_SIGNING_PRIVATE_KEY"
  fi
  version=$(jq -r .version src-tauri/tauri.conf.json)
  [ -z "${FAKE_RESIGN_OTHER_KEY:-}" ] || {
    TAURI_SIGNING_PRIVATE_KEY=$FAKE_RESIGN_OTHER_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
  }
  "$TAURI_CLI" signer sign --app-version "$version" "${*: -1}" >/dev/null 2>&1
  exit $?
fi
[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && echo key >>"$STUB_CALLS/npm-got-key"
printf '%s' "${APPLE_ID-unset}${APPLE_PASSWORD-unset}${APPLE_API_KEY-unset}${APPLE_SIGNING_IDENTITY-unset}" >"$STUB_CALLS/npm-apple"
[ -n "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ] && echo password >>"$STUB_CALLS/npm-got-password"
printf '%s' "${DICTATION_POSTHOG_KEY-unset}" >"$STUB_CALLS/npm-posthog"
printf '%s' "${DICTATION_SENTRY_DSN-unset}" >"$STUB_CALLS/npm-sentry"
echo "    Building Vocet"
if [ -n "${FAKE_LEAK:-}" ]; then
  echo "debug: posthog=${DICTATION_POSTHOG_KEY-}"
  echo "debug: sentry=${DICTATION_SENTRY_DSN-}"
  echo "debug: key=$TAURI_SIGNING_PRIVATE_KEY"
  echo "debug: password=$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" >&2
fi
b=target/release/bundle
mkdir -p "$b/macos/Vocet.app/Contents" "$b/dmg"
echo app >"$b/macos/Vocet.app/Contents/Info.plist"
tar -C "$b/macos" -cf "$b/dmg/Vocet_0.3.0_aarch64.dmg" Vocet.app
tar -C "$b/macos" -czf "$b/macos/Vocet.app.tar.gz" Vocet.app
version=$(jq -r .version src-tauri/tauri.conf.json)
[ -z "${FAKE_SIGN_VERSION:-}" ] || version=$FAKE_SIGN_VERSION
if [ -n "${FAKE_OTHER_KEY:-}" ]; then
  TAURI_SIGNING_PRIVATE_KEY=$FAKE_OTHER_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    "$TAURI_CLI" signer sign --app-version "$version" "$b/macos/Vocet.app.tar.gz" >/dev/null 2>&1
else
  "$TAURI_CLI" signer sign --app-version "$version" "$b/macos/Vocet.app.tar.gz" >/dev/null 2>&1
fi
[ -z "${FAKE_TAMPER:-}" ] || echo tampered >>"$b/macos/Vocet.app.tar.gz"
echo "    Finished"
STUB
chmod +x "$stubs"/*

# --- A throwaway repository ---------------------------------------------------

repo=$work/repo
mkdir -p "$repo/scripts" "$repo/src-tauri" "$repo/docs"
cp "$root/scripts/release.sh" "$root/scripts/make-latest-json.sh" "$root/scripts/release-signing.conf" "$repo/scripts/"
# The repository's own configuration names the Developer ID (checked in
# section 6). The throwaway repository starts with the identity empty, the
# state of a checkout without the certificate, and sections 1 to 6a rely on it.
cp "$root/scripts/release-signing.conf" "$work/conf.as-merged"
sed 's/^RELEASE_IDENTITY=.*/RELEASE_IDENTITY=/' "$work/conf.as-merged" >"$repo/scripts/release-signing.conf"
cp "$root/src-tauri/Entitlements.plist" "$repo/src-tauri/"
printf '#!/usr/bin/env bash\nexit 0\n' >"$repo/scripts/check-licences.sh"
printf '#!/usr/bin/env bash\nexit 0\n' >"$repo/scripts/check-dmg.sh"
chmod +x "$repo/scripts"/*.sh
jq -n --arg v "$VERSION" --arg k "$PUBKEY" \
  '{version: $v, plugins: {updater: {pubkey: $k, endpoints: ["https://github.com/gargankit90/dictation-app/releases/latest/download/latest.json"]}}}' \
  >"$repo/src-tauri/tauri.conf.json"
jq -n --arg v "$VERSION" '{name: "dictation-app", version: $v}' >"$repo/package.json"
printf '[workspace.package]\nversion = "%s"\n' "$VERSION" >"$repo/Cargo.toml"
echo "# Testers" >"$repo/docs/TESTERS.md"
cat >"$repo/CHANGELOG.md" <<EOF
# Changelog

## [Unreleased]

## [$VERSION] — [date set at release]

First line of the notes.

### Known limitations

- A limit.

## [0.0.9] — 2020-01-01

Older notes, never shown.

[Unreleased]: https://example.invalid/compare
[$VERSION]: https://example.invalid/tag
EOF
CHANGELOG_NOTES=$'First line of the notes.\n\n### Known limitations\n\n- A limit.'
echo "licences" >"$repo/THIRD-PARTY-LICENSES.txt"
printf 'target/\n' >"$repo/.gitignore"
(
  cd "$repo"
  git init -q
  # Refuse to go on unless git now means the throwaway repository.
  [ "$(cd "$(git rev-parse --git-dir)" && pwd -P)" = "$(cd "$repo/.git" && pwd -P)" ] || {
    echo "test-release: git doesn't point at the throwaway repository; stopping" >&2
    exit 1
  }
  git -c user.name=t -c user.email=t@example.invalid add -A
  git -c user.name=t -c user.email=t@example.invalid commit -qm init
  # A local bare "origin" with the commit on its default branch (never GitHub).
  git branch -M main
  git init -q --bare "$work/origin.git"
  git remote add origin "$work/origin.git"
  git push -q origin main
  git remote set-head origin main >/dev/null
)

# run_release <args...>: the script in the throwaway repository, with the
# stubs first on PATH and gh pointed at an empty config (so even a gh that
# wasn't the stub would not be logged in). Output in $work/out.txt.
run_release() {
  rm -rf "$calls" && mkdir -p "$calls"
  (
    cd "$repo"
    env PATH="$stubs:$PATH" STUB_CALLS="$calls" TAURI_CLI="$tauri_cli" \
      GH_CONFIG_DIR="$work/no-gh" GH_TOKEN="" GITHUB_TOKEN="" \
      FAKE_KEY="$KEY" FAKE_PASSWORD="$KEY_PASSWORD" \
      FAKE_POSTHOG="$POSTHOG" FAKE_SENTRY="$SENTRY" \
      ${RELEASE_SHELL:-bash} scripts/release.sh "$@"
  ) >"$work/out.txt" 2>&1
}

no_secret_in_output() {
  ! grep -qF -- "$KEY" "$work/out.txt" || fail "$1: the key was printed"
  ! grep -qF -- "$KEY_PASSWORD" "$work/out.txt" || fail "$1: the password was printed"
  ! grep -qF -- "$POSTHOG" "$work/out.txt" || fail "$1: the PostHog key was printed"
  ! grep -qF -- "$SENTRY" "$work/out.txt" || fail "$1: the Sentry DSN was printed"
}

[ "$(env PATH="$stubs:$PATH" bash -c 'command -v gh')" = "$stubs/gh" ] || fail "the gh stub isn't first on PATH"

out=$repo/target/release-out/v$VERSION
base=Vocet_${VERSION}_aarch64

# 1. A dry run.
run_release "$VERSION" --notes "Test notes." || fail "the dry run failed"
for file in "$base.dmg" "$base.app.tar.gz" "$base.app.tar.gz.sig" latest.json README.md THIRD-PARTY-LICENSES.txt SHA256SUMS; do
  [ -s "$out/$file" ] || fail "the dry run made no $file"
done
grep -q 'signature verifies against the public key' "$work/out.txt" || fail "no verification line"
[ "$(jq -r .version "$out/latest.json")" = "$VERSION" ] || fail "latest.json has the wrong version"
[ "$(jq -r .notes "$out/latest.json")" = "Test notes." ] || fail "latest.json has the wrong notes"
[ "$(jq -r '.platforms["darwin-aarch64"].url' "$out/latest.json")" = \
  "https://github.com/gargankit90/dictation-app/releases/download/v$VERSION/$base.app.tar.gz" ] ||
  fail "latest.json's url isn't this repository's"
[ "$(jq -r '.platforms["darwin-aarch64"].signature' "$out/latest.json")" = "$(cat "$out/$base.app.tar.gz.sig")" ] ||
  fail "latest.json's signature isn't the .sig"
[ -s "$calls/npm-got-key" ] && [ -s "$calls/npm-got-password" ] || fail "the build didn't get the key and password"
[ ! -e "$calls/gh" ] || fail "a dry run called gh"
grep -q 'dry run: nothing uploaded' "$work/out.txt" || fail "the dry run didn't say it uploaded nothing"
no_secret_in_output "dry run"
if grep -rqF -- "$KEY" "$repo" || grep -rqF -- "$KEY_PASSWORD" "$repo"; then
  fail "a secret was written to a file in the repository"
fi
pass "a dry run builds, verifies, writes latest.json, uploads nothing and never prints the secrets"

# 1b. Without --notes, the CHANGELOG section is the notes (R2): its text only,
# not the heading, the next version's section or the link definitions.
run_release "$VERSION" || fail "the dry run from the CHANGELOG failed"
[ "$(jq -r .notes "$out/latest.json")" = "$CHANGELOG_NOTES" ] ||
  fail "latest.json's notes aren't the CHANGELOG section: $(jq -r .notes "$out/latest.json")"
[ ! -e "$calls/gh" ] || fail "a dry run called gh"
# A version with no section, or an empty one, is refused before anything is built.
sed "s/^## \[$VERSION\].*/## [0.0.1] — x/" "$repo/CHANGELOG.md" >"$work/CHANGELOG.missing"
cp "$repo/CHANGELOG.md" "$work/CHANGELOG.good"
cp "$work/CHANGELOG.missing" "$repo/CHANGELOG.md"
run_release "$VERSION" && fail "a version with no CHANGELOG section passed"
grep -q "CHANGELOG.md has no \"## \[$VERSION\]\" section" "$work/out.txt" || fail "no missing-section message"
[ ! -e "$calls/npm" ] && [ ! -e "$calls/security" ] || fail "a missing section built or read the Keychain"
printf '# Changelog\n\n## [%s] — x\n\n## [0.0.9] — y\n\nOlder.\n' "$VERSION" >"$repo/CHANGELOG.md"
run_release "$VERSION" && fail "an empty CHANGELOG section passed"
grep -q 'has no "## \[' "$work/out.txt" || fail "no empty-section message"
rm "$repo/CHANGELOG.md"
run_release "$VERSION" && fail "a missing CHANGELOG.md passed"
grep -q 'no CHANGELOG.md' "$work/out.txt" || fail "no missing-file message"
# --notes still wins.
run_release "$VERSION" --notes "Other." || fail "--notes without a CHANGELOG failed"
[ "$(jq -r .notes "$out/latest.json")" = "Other." ] || fail "--notes didn't win"
cp "$work/CHANGELOG.good" "$repo/CHANGELOG.md"
pass "the notes are the CHANGELOG section; a missing file or section is refused before the build; --notes overrides"

# 2. A build that prints them is filtered, also under bash -x.
FAKE_LEAK=1 run_release "$VERSION" || fail "the leaking dry run failed"
grep -q 'debug: key=\[redacted\]' "$work/out.txt" || fail "the key line wasn't redacted"
grep -q 'debug: password=\[redacted\]' "$work/out.txt" || fail "the password line wasn't redacted"
no_secret_in_output "leaking build"
FAKE_LEAK=1 RELEASE_SHELL="bash -x" run_release "$VERSION" || fail "the traced dry run failed"
no_secret_in_output "bash -x"
pass "secrets printed by the build or a trace are never shown"

# 2b. The telemetry keys from the Keychain reach the build, and only it.
run_release "$VERSION" || fail "the dry run failed"
[ "$(cat "$calls/npm-posthog")" = "$POSTHOG" ] || fail "the build didn't get the PostHog key"
[ "$(cat "$calls/npm-sentry")" = "$SENTRY" ] || fail "the build didn't get the Sentry DSN"
if grep -rqF -- "$POSTHOG" "$repo" || grep -rqF -- "$SENTRY" "$repo"; then
  fail "a telemetry value was written to a file in the repository"
fi
no_secret_in_output "both telemetry items present"
# A missing item stops the release before it builds, naming the item and
# pointing to docs/releasing.md; the other item is no excuse.
for missing in POSTHOG SENTRY; do
  case $missing in
    POSTHOG) item=dictation-app-posthog-key ;;
    SENTRY) item=dictation-app-sentry-dsn ;;
  esac
  rm -rf "$repo/target/release-out"
  if [ "$missing" = POSTHOG ]; then
    FAKE_POSTHOG_MISSING=1 run_release "$VERSION" && fail "a release without $item passed"
  else
    FAKE_SENTRY_MISSING=1 run_release "$VERSION" && fail "a release without $item passed"
  fi
  grep -q "ERROR: no Keychain item $item" "$work/out.txt" || fail "no message naming $item"
  grep -q 'The telemetry keys' "$work/out.txt" || fail "no pointer to docs/releasing.md for $item"
  [ ! -e "$calls/npm-posthog" ] || fail "the build started without $item"
  [ ! -e "$repo/target/release-out/v$VERSION/latest.json" ] || fail "a release was made without $item"
  no_secret_in_output "missing $item"
done
FAKE_NO_TELEMETRY=1 run_release "$VERSION" && fail "a release with neither telemetry item passed"
# --no-telemetry builds without them on purpose, with a warning, and drops
# values from the caller's environment instead of passing them on.
FAKE_NO_TELEMETRY=1 DICTATION_POSTHOG_KEY=from-caller DICTATION_SENTRY_DSN=from-caller \
  run_release "$VERSION" --no-telemetry || fail "a --no-telemetry release failed"
grep -q 'WARNING: --no-telemetry' "$work/out.txt" || fail "no --no-telemetry warning"
[ "$(cat "$calls/npm-posthog")" = unset ] && [ "$(cat "$calls/npm-sentry")" = unset ] ||
  fail "a --no-telemetry build still got telemetry values"
[ -s "$out/latest.json" ] || fail "the release wasn't built with --no-telemetry"
! grep -q 'security find-generic-password -s dictation-app-\(posthog\|sentry\)' "$calls/security" ||
  fail "--no-telemetry still read a telemetry item"
no_secret_in_output "--no-telemetry"
# --no-telemetry with --publish is refused before the Keychain is read.
run_release "$VERSION" --no-telemetry --publish && fail "--no-telemetry --publish passed"
grep -q -- '--no-telemetry is for a dry run' "$work/out.txt" || fail "no --no-telemetry --publish message"
[ ! -e "$calls/security" ] || fail "--no-telemetry --publish read the Keychain before refusing"
pass "the telemetry keys reach only the build; a missing item stops the release before it builds; --no-telemetry builds without them; with --publish it is refused; none is printed"

# 3. A wrong signature is refused.
FAKE_OTHER_KEY=$(cat "$work/other.key") run_release "$VERSION" && fail "a bundle signed with another key passed"
grep -q 'does not verify' "$work/out.txt" || fail "no 'does not verify' message"
[ ! -e "$out/latest.json" ] || fail "latest.json was written for a bad signature"
no_secret_in_output "other key"
FAKE_SIGN_VERSION=9.9.9 run_release "$VERSION" && fail "a bundle signed for another version passed"
grep -q 'not bound to version' "$work/out.txt" || fail "no 'not bound to version' message"
FAKE_TAMPER=1 run_release "$VERSION" && fail "a bundle changed after signing passed"
grep -q 'does not verify against the public key' "$work/out.txt" || fail "no message for a changed bundle"
pass "a bundle signed with another key, for another version or changed after signing is refused"

# 4. --publish needs a clean tree, a merged commit and a new tag, checked
# before the Keychain.
refused() { # refused <message regex> <label>
  grep -q -- "$1" "$work/out.txt" || fail "no '$1' message ($2)"
  [ ! -e "$calls/security" ] || ! grep -q find-generic-password "$calls/security" || fail "the Keychain was read for a refused publish ($2)"
  [ ! -e "$calls/npm" ] && [ ! -e "$calls/gh" ] || fail "a refused publish built or called gh ($2)"
}
echo change >>"$repo/docs/TESTERS.md"
run_release "$VERSION" --publish && fail "--publish passed on a dirty tree"
refused 'needs a clean tree' "dirty tree"
git -C "$repo" checkout -q -- docs/TESTERS.md

# A commit that isn't on origin's default branch.
git -C "$repo" -c user.name=t -c user.email=t@example.invalid commit -q --allow-empty -m unmerged
run_release "$VERSION" --publish && fail "--publish passed with an unmerged commit"
refused 'needs HEAD .* on origin/main' "unmerged commit"
git -C "$repo" reset -q --hard HEAD~1

# A tag that exists here, or only on origin, is refused: tags are immutable.
tag_msg="tag v$VERSION already exists; tags are immutable here.*needs a new version"
git -C "$repo" tag "v$VERSION"
run_release "$VERSION" --publish && fail "--publish passed with a local tag v$VERSION"
refused "$tag_msg" "local tag"
git -C "$repo" tag --delete "v$VERSION" >/dev/null
git -C "$repo" tag "v$VERSION"
git -C "$repo" push -q origin "refs/tags/v$VERSION"
git -C "$repo" tag --delete "v$VERSION" >/dev/null
run_release "$VERSION" --publish && fail "--publish passed with a tag v$VERSION on origin"
refused "$tag_msg" "remote tag"
git -C "$work/origin.git" tag --delete "v$VERSION" >/dev/null
git -C "$repo" tag --delete "v$VERSION" >/dev/null 2>&1 || true  # the script's fetch brought it back
# Other versions' tags, and tags that match v$VERSION only as a regex, are no obstacle.
git -C "$repo" tag "v0.2.9"
git -C "$repo" tag "v${VERSION//./x}"
pass "--publish is refused without a clean tree, off the default branch, or when tag v$VERSION exists"

head=$(git -C "$repo" rev-parse HEAD)
run_release "$VERSION" --publish || fail "--publish failed with a clean, merged tree and a new tag"
grep -q "^gh release create v$VERSION --repo gargankit90/dictation-app --target $head " "$calls/gh" ||
  fail "gh wasn't asked to create v$VERSION in this repository at $head: $(cat "$calls/gh" 2>/dev/null)"
! grep -q 'dictation-app-releases' "$calls/gh" || fail "gh was pointed at the old repository"
grep -q "$base.app.tar.gz.sig" "$calls/gh" && grep -q latest.json "$calls/gh" || fail "gh wasn't given the bundle files"
[ "$(cat "$calls/gh-notes")" = "$CHANGELOG_NOTES" ] || fail "gh wasn't given the CHANGELOG section as the release notes"
no_secret_in_output "publish"
pass "--publish on a merged, clean commit hands the release to gh at that commit (a stub here)"

# 5. Other refusals.
run_release 0.3.1 && fail "a version that isn't tauri.conf.json's passed"
grep -q 'has version 0.3.0, not 0.3.1' "$work/out.txt" || fail "no version mismatch message"
[ ! -e "$calls/npm" ] || fail "a version mismatch built"
run_release "not-a-version" && fail "a bad version passed"
FAKE_ADHOC=1 run_release "$VERSION" && fail "an app not signed dictation-app Dev passed"
grep -q 'is not signed "dictation-app Dev"' "$work/out.txt" || fail "no signing message"
FAKE_KEY_MISSING=1 run_release "$VERSION" && fail "a missing Keychain item passed"
grep -q 'no Keychain item with service dictation-app-updater-key' "$work/out.txt" || fail "no missing-key message"
[ ! -e "$calls/npm" ] || fail "a missing key built"
pass "a wrong version, an app not signed dictation-app Dev or a missing Keychain item is refused"

# --- 6. The certificate, and notarisation (N1) -------------------------------

DEVID="Developer ID Application: Test Person (ABCDE12345)"
CONF=$repo/scripts/release-signing.conf
NOTARY_SECRET=app-specific-$RANDOM$RANDOM-throwaway
devid_listed="  1) 0123456789ABCDEF \"dictation-app Dev\"
  2) FEDCBA9876543210 \"$DEVID\""
dev_only="  1) 0123456789ABCDEF \"dictation-app Dev\""

# conf_for <identity> <profile>: the configuration says so.
conf_for() {
  printf 'RELEASE_IDENTITY=%s\nRELEASE_NOTARY_PROFILE=%s\n' "$1" "$2" >"$CONF"
}
no_build() { [ ! -e "$calls/npm" ] || fail "$1: it built anyway"; }
keychain_untouched() {
  [ ! -e "$calls/security" ] || ! grep -q find-generic-password "$calls/security" || fail "$1: it read the Keychain items"
}
no_credential_shown() {
  no_secret_in_output "$1"
  ! grep -qF -- "$NOTARY_SECRET" "$work/out.txt" || fail "$1: a notarisation credential was printed"
}

# The repository's own state: hardened runtime, one entitlement, and the file
# as merged falls back to the dev certificate.
[ "$(jq -r .bundle.macOS.hardenedRuntime "$root/src-tauri/tauri.conf.json")" = true ] ||
  fail "tauri.conf.json doesn't turn on the hardened runtime"
[ "$(grep -o 'com\.apple\.[a-z.-]*' "$root/src-tauri/Entitlements.plist" | sort -u)" = "com.apple.security.device.audio-input" ] ||
  fail "Entitlements.plist has more than the microphone: write down why in docs/releasing.md, then change this test"
pass "tauri.conf.json turns on the hardened runtime and Entitlements.plist has only the microphone"
# Since N2 the tester guide has only the notarized first launch: no quarantine
# or xattr step, and "Not notarised" is not a known limit.
grep -q '^<!-- SWITCH AT THE FIRST NOTARIZED RELEASE' "$root/docs/TESTERS.md" &&
  grep -q '^## 2. Open it for the first time$' "$root/docs/TESTERS.md" ||
  fail "docs/TESTERS.md has lost the version for notarized builds"
! grep -qi 'xattr\|clear the quarantine' "$root/docs/TESTERS.md" ||
  fail "docs/TESTERS.md still has the quarantine steps for a build that isn't notarized"
! grep -qi 'not notari' "$root/docs/TESTERS.md" ||
  fail "docs/TESTERS.md still lists \"Not notarised\" as a known limit"
pass "the tester guide has the notarized first launch and no quarantine steps"

# The file as merged (N2) names the owner's Developer ID certificate and the
# notarytool profile; it is a name, not a secret.
grep -qx 'RELEASE_IDENTITY=Developer ID Application: Ankit Garg (XF2G42MVHN)' "$work/conf.as-merged" ||
  fail "scripts/release-signing.conf doesn't name the Developer ID certificate"
grep -qx 'RELEASE_NOTARY_PROFILE=vocet-notary' "$work/conf.as-merged" ||
  fail "scripts/release-signing.conf lost the notarytool profile name"
pass "scripts/release-signing.conf names the Developer ID certificate and the vocet-notary profile"

# 6a. Empty configuration (a checkout that emptied it): the fallback, no
# notarisation.
conf_for "" vocet-notary
run_release "$VERSION" || fail "the fallback dry run failed"
grep -q 'signing with the fallback "dictation-app Dev"' "$work/out.txt" || fail "no fallback line"
grep -qF '"signingIdentity":"dictation-app Dev"' "$calls/npm" || fail "the build wasn't told the fallback identity"
grep -q 'NOT notarised' "$work/out.txt" || fail "the fallback didn't say it isn't notarised"
[ ! -e "$calls/xcrun" ] && [ ! -e "$calls/spctl" ] || fail "the fallback called the notarisation tools"
pass "with an empty configuration, the dev certificate signs and nothing is notarised"

# A Developer ID in the keychain is pointed out, not picked by itself.
FAKE_IDENTITIES="$devid_listed" run_release "$VERSION" || fail "the fallback with a Developer ID present failed"
grep -q 'NOTE: this keychain has a Developer ID certificate' "$work/out.txt" || fail "no note about the unused Developer ID"
grep -qF '"signingIdentity":"dictation-app Dev"' "$calls/npm" || fail "a Developer ID was picked without the configuration"
pass "a Developer ID in the keychain is pointed out but not used until it is configured"

# 6b. A configured Developer ID, the whole flow.
conf_for "$DEVID" vocet-notary
(
  export FAKE_IDENTITIES="$devid_listed" FAKE_AUTHORITY="$DEVID"
  export APPLE_ID=x APPLE_PASSWORD="$NOTARY_SECRET" APPLE_API_KEY=y APPLE_SIGNING_IDENTITY=z
  export FAKE_NOTARY_LEAK="$NOTARY_SECRET"
  run_release "$VERSION"
) || fail "the Developer ID dry run failed"
no_credential_shown "Developer ID"
grep -qF "\"signingIdentity\":\"$DEVID\"" "$calls/npm" || fail "the build wasn't told the Developer ID"
grep -qF '"hardenedRuntime":true' "$calls/npm" || fail "the build wasn't told the hardened runtime"
[ "$(cat "$calls/npm-apple")" = unsetunsetunsetunset ] || fail "Apple credentials from the environment reached the build: [$(cat "$calls/npm-apple")]"
[ "$(wc -l <"$calls/notary-submitted" | tr -d ' ')" = 2 ] || fail "not exactly two submissions: $(cat "$calls/notary-submitted")"
sed -n 1p "$calls/notary-submitted" | grep -q 'Vocet.zip$' || fail "the app wasn't submitted first"
sed -n 2p "$calls/notary-submitted" | grep -q 'aarch64.dmg$' || fail "the DMG wasn't submitted second"
grep -c -- '--keychain-profile vocet-notary' "$calls/xcrun" >/dev/null || fail "notarytool wasn't given the profile"
! grep -qF -- "$NOTARY_SECRET" "$calls/xcrun" || fail "a credential was on notarytool's command line"
grep -qF "stapler validate" "$calls/xcrun" || fail "no stapler validate"
grep -q -- '--type execute' "$calls/spctl" || fail "spctl didn't assess the app"
grep -q -- '--type open --context context:primary-signature' "$calls/spctl" || fail "spctl didn't assess the DMG"
grep -q -- '--sign .*--timestamp .*aarch64.dmg$' "$calls/codesign" || fail "the DMG wasn't signed with a timestamp"
grep -q 'the updater bundle and the DMG hold the same notarised app' "$work/out.txt" || fail "no summary of the checks"
grep -q 'notarised and stapled' "$work/out.txt" || fail "no notarised line"
# What the release folder holds is stapled, and its manifest is signed for it.
check=$work/check-release
rm -rf "$check" && mkdir -p "$check/tar" "$check/dmg"
tar -xzf "$out/$base.app.tar.gz" -C "$check/tar"
[ -s "$check/tar/Vocet.app/Contents/CodeResources" ] || fail "the updater bundle's app isn't the stapled one"
tar -xf "$out/$base.dmg" -C "$check/dmg"
[ -s "$check/dmg/Vocet.app/Contents/CodeResources" ] || fail "the DMG's app isn't the stapled one"
grep -q 'signature verifies against the public key' "$work/out.txt" || fail "the rebuilt bundle's minisign check didn't run"
[ "$(jq -r '.platforms[].signature' "$out/latest.json")" = "$(cat "$out/$base.app.tar.gz.sig")" ] || fail "latest.json doesn't carry the new signature"
pass "a Developer ID release: Apple's two submissions, staples, Gatekeeper checks, and bundle and DMG rebuilt around the stapled app"

# The same with a trace on, and with a build that prints its secrets.
(
  export FAKE_IDENTITIES="$devid_listed" FAKE_AUTHORITY="$DEVID" FAKE_NOTARY_LEAK="$NOTARY_SECRET" FAKE_LEAK=1
  RELEASE_SHELL="bash -x" run_release "$VERSION"
) || fail "the traced Developer ID run failed"
no_credential_shown "Developer ID under bash -x"
pass "no key or credential is printed by the notarising flow, even under a trace"

# run_devid_refused <message> <label> <VAR=value...>: the Developer ID flow
# fails, with that message, publishing nothing.
run_devid_refused() {
  local message=$1 label=$2
  shift 2
  rm -rf "$out"
  if (
    export FAKE_IDENTITIES="$devid_listed" FAKE_AUTHORITY="$DEVID"
    for assignment in "$@"; do export "${assignment?}"; done
    run_release "$VERSION"
  ); then
    fail "$label passed"
  fi
  grep -qF -- "$message" "$work/out.txt" || fail "$label: no '$message' message"
  [ ! -e "$calls/gh" ] || fail "$label called gh"
  [ ! -e "$out/latest.json" ] || fail "$label wrote latest.json"
  no_credential_shown "$label"
}

# 6c. The credential: asked for before anything is built or read.
conf_for "$DEVID" ""
run_devid_refused 'RELEASE_NOTARY_PROFILE in scripts/release-signing.conf is empty' "no profile name"
no_build "no profile name"
keychain_untouched "no profile name"
conf_for "$DEVID" vocet-notary
run_devid_refused "the Keychain profile \"vocet-notary\" doesn't work" "a profile that doesn't work" FAKE_NO_NOTARY_CREDENTIAL=1
no_build "a profile that doesn't work"
keychain_untouched "a profile that doesn't work"
pass "notarisation without a working credential stops with a clear message before the build and the Keychain"

# 6d. No fallback once configured.
if (
  export FAKE_IDENTITIES="$dev_only"
  run_release "$VERSION"
); then fail "a missing configured certificate passed"; fi
grep -q 'not falling back to "dictation-app Dev"' "$work/out.txt" || fail "no message for a missing configured certificate"
no_build "a missing certificate"
if (
  export FAKE_IDENTITIES="$devid_listed" FAKE_VALID_IDENTITIES="$dev_only"
  run_release "$VERSION"
); then fail "an expired certificate passed"; fi
grep -q "is in the keychain but can't sign (expired or revoked?)" "$work/out.txt" || fail "no message for an expired certificate"
no_build "an expired certificate"
if (
  export FAKE_IDENTITIES="$devid_listed"
  FAKE_VALID_IDENTITIES="$devid_listed
  3) 0000111122223333 \"$DEVID\""
  export FAKE_VALID_IDENTITIES
  run_release "$VERSION"
); then fail "two valid certificates with one name passed"; fi
grep -q 'two valid certificates are named' "$work/out.txt" || fail "no message for two certificates with one name"
no_build "two certificates with one name"
pass "a configured certificate that is missing, expired or ambiguous stops the release; it never falls back"

# 6e. What Apple and macOS say.
run_devid_refused "Apple's answer for the app is \"Invalid\"" "the app refused" FAKE_APP_STATUS=Invalid
grep -q 'The binary is not signed.' "$work/out.txt" || fail "Apple's list of problems wasn't shown"
run_devid_refused "Apple's answer for the DMG is \"Rejected\"" "the DMG refused" FAKE_DMG_STATUS=Rejected
run_devid_refused 'notarytool gave no answer for the app' "no answer" FAKE_NOTARY_NO_ANSWER=1
run_devid_refused 'could not staple the ticket to the app' "a failed staple" FAKE_STAPLE_FAIL=1
run_devid_refused 'Gatekeeper refuses the app' "spctl says no" FAKE_SPCTL_REJECT=1
run_devid_refused "Gatekeeper doesn't see the app as notarised" "spctl sees no ticket" FAKE_SPCTL_UNNOTARIZED=1
run_devid_refused 'the app in the updater bundle has no valid ticket' "no ticket in the bundle" FAKE_NO_TICKET_WHERE=archive-check
run_devid_refused 'the app in the DMG has no valid ticket' "no ticket in the DMG" FAKE_NO_TICKET_WHERE=dmg-check
pass "a refusal by Apple, a failed staple or a Gatekeeper refusal stops the release and writes no manifest"

# 6f. What was built.
run_devid_refused 'is not signed with the hardened runtime' "no hardened runtime" FAKE_NO_RUNTIME=1
run_devid_refused "entitlements are" "a broad entitlement" FAKE_EXTRA_ENTITLEMENT=1
run_devid_refused 'has no secure timestamp' "no timestamp" FAKE_NO_TIMESTAMP=1
run_devid_refused 'is not signed "'"$DEVID"'"' "the updater bundle signed by someone else" FAKE_WRONG_WHERE=archive
run_devid_refused 'is not signed "'"$DEVID"'"' "the DMG signed by someone else" FAKE_WRONG_WHERE=dmg
run_devid_refused 'the updater bundle holds different code' "other code in the bundle" FAKE_OTHER_CODE_WHERE=archive
run_devid_refused 'the DMG holds different code' "other code in the DMG" FAKE_OTHER_CODE_WHERE=dmg
run_devid_refused 'does not verify' "a bundle re-signed with another update key" \
  FAKE_RESIGN_OTHER_KEY="$(cat "$work/other.key")"
pass "another signer, other code, no hardened runtime, an extra entitlement or a bad update signature is refused"

# 6g. The switches.
conf_for "" vocet-notary
run_release "$VERSION" --notarize && fail "--notarize passed with the dev certificate"
grep -q 'Apple does not notarise "dictation-app Dev"' "$work/out.txt" || fail "no message for --notarize without a Developer ID"
no_build "--notarize with the dev certificate"
conf_for "$DEVID" vocet-notary
(
  export FAKE_IDENTITIES="$devid_listed" FAKE_AUTHORITY="$DEVID"
  run_release "$VERSION" --no-notarize
) || fail "the test signing run failed"
[ ! -e "$calls/notary-submitted" ] || fail "--no-notarize submitted"
grep -q 'WARNING: --no-notarize' "$work/out.txt" || fail "no warning for --no-notarize"
grep -q 'NOT notarised' "$work/out.txt" || fail "--no-notarize didn't say it isn't notarised"
git -C "$repo" -c user.name=t -c user.email=t@example.invalid add -A >/dev/null
git -C "$repo" -c user.name=t -c user.email=t@example.invalid commit -qm "the Developer ID configuration"
git -C "$repo" push -q origin main
(
  export FAKE_IDENTITIES="$devid_listed" FAKE_AUTHORITY="$DEVID"
  run_release "$VERSION" --publish --no-notarize
) && fail "--publish --no-notarize passed"
grep -q -- '--no-notarize is for a dry test signing run' "$work/out.txt" || fail "no message for --publish --no-notarize"
no_build "--publish --no-notarize"
(
  export FAKE_IDENTITIES="$devid_listed" FAKE_AUTHORITY="$DEVID" RELEASE_IDENTITY="$DEVID"
  run_release "$VERSION" --publish
) && fail "--publish with an identity from the environment passed"
grep -q -- '--publish uses scripts/release-signing.conf as merged' "$work/out.txt" || fail "no message for a --publish override"
no_build "--publish with an override"
# A notarised publish hands the rebuilt, notarised files to gh.
(
  export FAKE_IDENTITIES="$devid_listed" FAKE_AUTHORITY="$DEVID"
  run_release "$VERSION" --publish
) || fail "a notarised --publish failed"
grep -q "^gh release create v$VERSION " "$calls/gh" || fail "a notarised publish didn't reach gh"
[ "$(wc -l <"$calls/notary-submitted" | tr -d ' ')" = 2 ] || fail "a notarised publish skipped notarising"
no_credential_shown "notarised publish"
pass "--notarize needs a Developer ID, --no-notarize is dry-run only, and --publish reads only the merged configuration"
