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
#   - --publish is refused without a clean tree or without tag v<version>
#     on HEAD, before the Keychain is read; with both, it calls
#     `gh release create v<version> --repo gargankit90/dictation-app-releases`;
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
exit 0
STUB
# The build: an app, a DMG and the updater bundle, signed with whatever key
# the script handed over (by the real tauri signer, bound to a version).
cat >"$stubs/npm" <<'STUB'
#!/usr/bin/env bash
echo "npm $*" >>"$STUB_CALLS/npm"
[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && echo key >>"$STUB_CALLS/npm-got-key"
[ -n "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ] && echo password >>"$STUB_CALLS/npm-got-password"
echo "    Building dictation-app"
if [ -n "${FAKE_LEAK:-}" ]; then
  echo "debug: key=$TAURI_SIGNING_PRIVATE_KEY"
  echo "debug: password=$TAURI_SIGNING_PRIVATE_KEY_PASSWORD" >&2
fi
b=target/release/bundle
mkdir -p "$b/macos/dictation-app.app/Contents" "$b/dmg"
echo app >"$b/macos/dictation-app.app/Contents/Info.plist"
echo dmg >"$b/dmg/dictation-app_0.3.0_aarch64.dmg"
tar -C "$b/macos" -czf "$b/macos/dictation-app.app.tar.gz" dictation-app.app
version=$(jq -r .version src-tauri/tauri.conf.json)
[ -z "${FAKE_SIGN_VERSION:-}" ] || version=$FAKE_SIGN_VERSION
if [ -n "${FAKE_OTHER_KEY:-}" ]; then
  TAURI_SIGNING_PRIVATE_KEY=$FAKE_OTHER_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    "$TAURI_CLI" signer sign --app-version "$version" "$b/macos/dictation-app.app.tar.gz" >/dev/null 2>&1
else
  "$TAURI_CLI" signer sign --app-version "$version" "$b/macos/dictation-app.app.tar.gz" >/dev/null 2>&1
fi
[ -z "${FAKE_TAMPER:-}" ] || echo tampered >>"$b/macos/dictation-app.app.tar.gz"
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
  '{version: $v, plugins: {updater: {pubkey: $k, endpoints: ["https://github.com/gargankit90/dictation-app-releases/releases/latest/download/latest.json"]}}}' \
  >"$repo/src-tauri/tauri.conf.json"
jq -n --arg v "$VERSION" '{name: "dictation-app", version: $v}' >"$repo/package.json"
printf '[workspace.package]\nversion = "%s"\n' "$VERSION" >"$repo/Cargo.toml"
echo "# Testers" >"$repo/docs/TESTERS.md"
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
      ${RELEASE_SHELL:-bash} scripts/release.sh "$@"
  ) >"$work/out.txt" 2>&1
}

no_secret_in_output() {
  ! grep -qF -- "$KEY" "$work/out.txt" || fail "$1: the key was printed"
  ! grep -qF -- "$KEY_PASSWORD" "$work/out.txt" || fail "$1: the password was printed"
}

[ "$(env PATH="$stubs:$PATH" bash -c 'command -v gh')" = "$stubs/gh" ] || fail "the gh stub isn't first on PATH"

out=$repo/target/release-out/v$VERSION
base=dictation-app_${VERSION}_aarch64

# 1. A dry run.
run_release "$VERSION" --notes "Test notes." || fail "the dry run failed"
for file in "$base.dmg" "$base.app.tar.gz" "$base.app.tar.gz.sig" latest.json README.md THIRD-PARTY-LICENSES.txt SHA256SUMS; do
  [ -s "$out/$file" ] || fail "the dry run made no $file"
done
grep -q 'signature verifies against the public key' "$work/out.txt" || fail "no verification line"
[ "$(jq -r .version "$out/latest.json")" = "$VERSION" ] || fail "latest.json has the wrong version"
[ "$(jq -r .notes "$out/latest.json")" = "Test notes." ] || fail "latest.json has the wrong notes"
[ "$(jq -r '.platforms["darwin-aarch64"].url' "$out/latest.json")" = \
  "https://github.com/gargankit90/dictation-app-releases/releases/download/v$VERSION/$base.app.tar.gz" ] ||
  fail "latest.json's url isn't the releases repository's"
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

# 2. A build that prints them is filtered, also under bash -x.
FAKE_LEAK=1 run_release "$VERSION" || fail "the leaking dry run failed"
grep -q 'debug: key=\[redacted\]' "$work/out.txt" || fail "the key line wasn't redacted"
grep -q 'debug: password=\[redacted\]' "$work/out.txt" || fail "the password line wasn't redacted"
no_secret_in_output "leaking build"
FAKE_LEAK=1 RELEASE_SHELL="bash -x" run_release "$VERSION" || fail "the traced dry run failed"
no_secret_in_output "bash -x"
pass "secrets printed by the build or a trace are never shown"

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

# 4. --publish needs a clean tree and the tag, checked before the Keychain.
echo change >>"$repo/docs/TESTERS.md"
run_release "$VERSION" --publish && fail "--publish passed on a dirty tree"
grep -q 'needs a clean tree' "$work/out.txt" || fail "no 'clean tree' message"
[ ! -e "$calls/security" ] || ! grep -q find-generic-password "$calls/security" || fail "the Keychain was read for a refused publish"
[ ! -e "$calls/npm" ] && [ ! -e "$calls/gh" ] || fail "a refused publish built or called gh"
git -C "$repo" checkout -q -- docs/TESTERS.md
run_release "$VERSION" --publish && fail "--publish passed without tag v$VERSION"
grep -q "needs tag v$VERSION on HEAD" "$work/out.txt" || fail "no tag message"
[ ! -e "$calls/npm" ] && [ ! -e "$calls/gh" ] || fail "a refused publish built or called gh"
git -C "$repo" tag "v0.2.9"
run_release "$VERSION" --publish && fail "--publish passed with another version's tag"
# A tag that only matches v$VERSION as a regex (the dots) is not the tag.
lookalike="v${VERSION//./x}"
git -C "$repo" tag "$lookalike"
run_release "$VERSION" --publish && fail "--publish passed with the lookalike tag $lookalike"
grep -q "needs tag v$VERSION on HEAD" "$work/out.txt" || fail "no tag message for the lookalike tag"
[ ! -e "$calls/npm" ] && [ ! -e "$calls/gh" ] || fail "a refused publish built or called gh"
git -C "$repo" tag -d "$lookalike" >/dev/null
pass "--publish is refused without a clean tree or tag v$VERSION"

git -C "$repo" tag "v$VERSION"
run_release "$VERSION" --publish || fail "--publish failed with a clean tree and the tag"
grep -q "^gh release create v$VERSION --repo gargankit90/dictation-app-releases " "$calls/gh" ||
  fail "gh wasn't asked to create v$VERSION in the releases repository: $(cat "$calls/gh" 2>/dev/null)"
grep -q "$base.app.tar.gz.sig" "$calls/gh" && grep -q latest.json "$calls/gh" || fail "gh wasn't given the bundle files"
no_secret_in_output "publish"
pass "--publish with a clean tree and the tag hands the release to gh (a stub here)"

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
