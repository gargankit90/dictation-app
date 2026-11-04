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
#     item, is refused.
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
  find-identity) echo '  1) 0123456789ABCDEF "dictation-app Dev"' ;;
  find-generic-password)
    service=""
    while [ $# -gt 0 ]; do [ "$1" = -s ] && service=$2; shift; done
    case $service in
      dictation-app-updater-key) [ -n "${FAKE_KEY_MISSING:-}" ] && exit 44; printf '%s\n' "$FAKE_KEY" ;;
      dictation-app-updater-password) printf '%s\n' "$FAKE_PASSWORD" ;;
      dictation-app-posthog-key) [ -n "${FAKE_NO_TELEMETRY:-}" ] && exit 44; printf '%s\n' "$FAKE_POSTHOG" ;;
      dictation-app-sentry-dsn) [ -n "${FAKE_NO_TELEMETRY:-}" ] && exit 44; printf '%s\n' "$FAKE_SENTRY" ;;
      *) exit 44 ;;
    esac
    ;;
  *) exit 1 ;;
esac
STUB
cat >"$stubs/codesign" <<'STUB'
#!/usr/bin/env bash
case " $* " in
  *" -dvv "*)
    if [ -n "${FAKE_ADHOC:-}" ]; then echo "Signature=adhoc" >&2; else
      printf 'Executable=x\nAuthority=dictation-app Dev\nTeamIdentifier=not set\n' >&2
    fi
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
[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && echo key >>"$STUB_CALLS/npm-got-key"
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
echo dmg >"$b/dmg/Vocet_0.3.0_aarch64.dmg"
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
cp "$root/scripts/release.sh" "$root/scripts/make-latest-json.sh" "$repo/scripts/"
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
# One from the caller's environment is dropped, not passed on.
FAKE_NO_TELEMETRY=1 DICTATION_POSTHOG_KEY=from-caller DICTATION_SENTRY_DSN=from-caller \
  run_release "$VERSION" || fail "a release without the telemetry items failed"
grep -q 'WARNING: no Keychain item dictation-app-posthog-key' "$work/out.txt" || fail "no PostHog warning"
grep -q 'WARNING: no Keychain item dictation-app-sentry-dsn' "$work/out.txt" || fail "no Sentry warning"
[ "$(cat "$calls/npm-posthog")" = unset ] && [ "$(cat "$calls/npm-sentry")" = unset ] ||
  fail "a build without the Keychain items still got telemetry values"
[ -s "$out/latest.json" ] || fail "the release wasn't built without the telemetry items"
no_secret_in_output "no telemetry items"
pass "the telemetry keys reach only the build, a missing one warns and builds without it, and none is printed"

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
