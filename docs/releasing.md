# Releasing: DMG, updater key and GitHub Releases

How a tester build gets made, and what only the owner can do. Nothing here is
automated into CI on purpose: it needs a private signing key and a GitHub
Release, and the key must never be in the repo, a log, a PR or a CI secret
unless the owner decides so.

## What exists

| Piece                                                  | Where                                                                        |
| ------------------------------------------------------ | ---------------------------------------------------------------------------- |
| DMG build, checks, checksums                           | `scripts/build-dmg.sh`, `scripts/check-dmg.sh`                               |
| DMG on every packaging PR and on `main` (ad-hoc signed) | `.github/workflows/dmg.yml`, artifact `dictation-app-dmg`                   |
| Third-party licence notice, bundled and checked        | `THIRD-PARTY-LICENSES.txt`, `scripts/gen-licences.sh`, `scripts/check-licences.sh` |
| Tester guide                                           | `docs/testing/TESTER-README.md`                                              |
| Updater (tray "Check for updates…")                    | `src-tauri/src/updater.rs`, `plugins.updater` in `src-tauri/tauri.conf.json` |
| Manifest maker, throwaway-key updater test             | `scripts/make-latest-json.sh`, `scripts/updater-test.sh`                     |

The updater trusts one **minisign public key** from `tauri.conf.json`
(`plugins.updater.pubkey`) and refuses any download whose signature doesn't
verify against it, or whose signed version differs from the announced one
(`requireSignedVersion`). It is **empty** until the owner approves a real key;
the tray item then says _"Updates aren't set up"_.

## Signing, in one place

- **The app** is signed `dictation-app Dev` (`scripts/make-dev-cert.sh`) so
  that macOS keeps Microphone and Accessibility across updates. Build the
  tester DMG on a Mac that has the certificate: `scripts/build-dmg.sh`.
  CI has no certificate, so its DMG is ad hoc: good for checking that
  packaging works, not for handing to testers who should keep permissions.
- **The updates** are signed with the minisign **private** key (a different
  thing: it proves an update came from us). Only its public half is in the
  repo.
- No notarisation: there is no Apple Developer account. Testers clear the
  quarantine flag (see the tester guide).

## One-time, owner only

These need the owner; nothing below has been done.

1. **Make the real update key**, off the repo, with a password:

   ```sh
   npx tauri signer generate -w ~/.tauri/dictation-app.key
   ```

   Keep `~/.tauri/dictation-app.key` and its password in the owner's password
   manager. Losing it means no tester can update again (they would have to
   install a new build by hand); leaking it lets someone push code to every
   tester.

2. **Commit only the public key**: paste the contents of
   `~/.tauri/dictation-app.key.pub` into `plugins.updater.pubkey` in
   `src-tauri/tauri.conf.json`, in a PR.
3. **Decide where releases live.** The endpoint in `tauri.conf.json` is
   `https://github.com/gargankit90/dictation-app/releases/latest/download/latest.json`.
   While the repository is **private**, GitHub answers 404 to anyone not
   signed in, and the updater sends no credentials, so no tester can fetch it.
   Either make the repository public, or publish releases from a small
   public repository (then change `endpoints`), or host `latest.json` and the
   archive somewhere public.
4. (Only if CI should ever build releases) add the key as the GitHub secrets
   `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Not
   needed for the manual flow below.

## Making a release

On the owner's Mac, from a clean checkout of `main` at the version to ship
(bump `version` in `src-tauri/tauri.conf.json`, `Cargo.toml` and
`package.json` first, in a PR):

```sh
export TAURI_SIGNING_PRIVATE_KEY=~/.tauri/dictation-app.key
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD='…'
npm run tauri build -- --bundles app,dmg --config '{"bundle":{"createUpdaterArtifacts":true}}'
scripts/check-dmg.sh target/release/bundle/dmg/dictation-app_<version>_aarch64.dmg
scripts/make-latest-json.sh <version> "<what's new>" \
  target/release/bundle/macos/dictation-app.app.tar.gz \
  target/release/bundle/macos/dictation-app.app.tar.gz.sig \
  https://github.com/gargankit90/dictation-app/releases/download/v<version>/dictation-app.app.tar.gz \
  > target/release/latest.json
gh release create v<version> --title "dictation-app <version>" --notes "<what's new>" \
  target/release/bundle/dmg/dictation-app_<version>_aarch64.dmg \
  target/release/bundle/macos/dictation-app.app.tar.gz \
  target/release/latest.json \
  THIRD-PARTY-LICENSES.txt docs/testing/TESTER-README.md
```

(`scripts/build-dmg.sh` does the DMG part and the checks; use it for the DMG
and the command above for the updater archive, or both flags in one build.)
Testers get the DMG and the README; installed apps find `latest.json` and
offer the update.

## Try the updater with a throwaway key (no owner secrets)

`scripts/updater-test.sh` makes a key pair in `target/updater-test/`
(git-ignored), builds version 0.1.0 and 0.1.1 that trust only that key and
fetch from your Mac, and serves the manifest.

```sh
scripts/updater-test.sh build     # about 2 minutes
scripts/updater-test.sh serve     # leave running
```

In the Lume VM (reachable from the VM at `192.168.64.1`, the default `HOST`):
copy `target/updater-test/old/dictation-app.app` to `~/Applications`, open it,
menu-bar icon > **Check for updates…** > **Install and restart**. The app
restarts as 0.1.1 (the log says `starting version=0.1.1`). On your own Mac use
`HOST=127.0.0.1 scripts/updater-test.sh build`.

**Permissions kept (M5.4 #3):** grant Microphone and Accessibility to the 0.1.0
build first (use `scripts/build-dmg.sh` signing, i.e. the `dictation-app Dev`
certificate, so both builds share a signature), dictate once, update, and
dictate again with no new prompt and no entry missing from System Settings >
Privacy & Security. An ad-hoc build is expected to lose them.
