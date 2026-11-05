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
| Tester guide (attached to each release as README.md)   | `docs/TESTERS.md`                                                            |
| Release build, signature check, upload                 | `scripts/release.sh` (tests: `scripts/test-release.sh`, in the gate)         |
| Updater (Settings → General → Check now)               | `src-tauri/src/updater.rs`, `plugins.updater` in `src-tauri/tauri.conf.json` |
| Manifest maker, throwaway-key updater test             | `scripts/make-latest-json.sh`, `scripts/updater-test.sh`                     |

The updater trusts one **minisign public key** from `tauri.conf.json`
(`plugins.updater.pubkey`, the owner's, key id `F6C80F173A96D78F`) and refuses
any download whose signature doesn't verify against it, or whose signed
version differs from the announced one (`requireSignedVersion`). It fetches
`https://github.com/gargankit90/dictation-app/releases/latest/download/latest.json`
(this repository's own releases; the separate `gargankit90/dictation-app-releases`
is archived and was never used for a release), once at launch
(quietly: a dialog only when there is an update) and from the tray's
Settings → General → **Check now**.

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

## The update key (done by the owner, 2026-11-02)

The private key and its password are generic passwords in the owner's login
Keychain, services `dictation-app-updater-key` and
`dictation-app-updater-password`, with a backup kept by the owner. Only the
public half is in the repo. Losing the key means no tester can update again
(each would install a new DMG by hand); leaking it lets someone push code to
every tester. It is never a CI secret: releases are built on the owner's Mac.

## The telemetry keys (owner, once)

A release carries the PostHog project key and the Sentry DSN, read by
`scripts/release.sh` from the login Keychain (generic passwords) and handed to
that one build as `DICTATION_POSTHOG_KEY` and `DICTATION_SENTRY_DSN`. Add them
once (the `-w` with no value asks for the value, so it isn't in your shell
history):

```sh
security add-generic-password -s dictation-app-posthog-key -a "$USER" -w
security add-generic-password -s dictation-app-sentry-dsn -a "$USER" -w
```

If an item is missing, the script prints a warning saying so and builds
without it: that build sends no analytics (PostHog) or no error reports
(Sentry), and the release still succeeds. The values are never printed, logged
or written to a file. See `docs/privacy.md` for the project settings to turn on.

## Making a release

1. In a PR, bump `version` in `src-tauri/tauri.conf.json`, `package.json` and
   `Cargo.toml` (the script refuses a version they don't all have), and make
   sure `CHANGELOG.md` has a `## [x.y.z] — <date>` section for it, with the
   `[date set at release]` replaced. That section's text (not its heading) is
   the GitHub release's notes and the "Update available" text; the script
   refuses a version with no section before it builds or reads the Keychain.
   `--notes "<text>"` overrides it. Merge the PR.
2. On the owner's Mac, on that merged `main` commit (`git switch --detach
   origin/main`), with a clean tree:

   ```sh
   scripts/release.sh 0.2.0   # dry run
   ```

   It builds the release signed `dictation-app Dev`, reads the two Keychain
   items into the build's environment only (filtered from its output, never
   written anywhere), checks the DMG (`scripts/check-dmg.sh`), verifies the
   updater bundle's `.sig` against the public key in `tauri.conf.json` and
   that it is bound to the version, and writes everything into
   `target/release-out/v0.2.0/`: the DMG, `…app.tar.gz` and its `.sig`,
   `latest.json`, `README.md` (this repo's `docs/TESTERS.md`),
   `THIRD-PARTY-LICENSES.txt` and `SHA256SUMS`. Nothing is uploaded. macOS may
   ask once to let `security` read the two Keychain items.

3. To publish, run it again with `--publish`. Do not create the tag yourself:

   ```sh
   scripts/release.sh 0.2.0 --publish
   ```

   It refuses without a clean tree, with a `HEAD` that is not on origin's
   default branch, or when tag `v0.2.0` already exists here or on origin. Then
   it runs `gh release create v0.2.0 --repo gargankit90/dictation-app
   --target <the commit it built>` with those files (the owner's own `gh`
   login; no new token), which creates the tag on that commit. Installed apps
   find the new `latest.json` at their next launch or Settings → General → **Check now**.

   **Tags are immutable here**: a ruleset blocks moving or deleting them. If
   a publish fails after the tag was created, do not retry the same version:
   bump to a new one (the script's error says so).

`docs/TESTERS.md` is attached to each release as its README; update it there
when the guide changes. Publishing the first real release (0.1.0) is the
owner's call.

## Try the updater with a throwaway key (no owner secrets)

`scripts/updater-test.sh` makes a key pair in `target/updater-test/`
(git-ignored), builds version 0.1.0 and 0.1.1 that trust only that key and
fetch from your Mac, and serves the manifest.

```sh
scripts/updater-test.sh build     # about 2 minutes
scripts/updater-test.sh serve     # leave running
```

In the Lume VM (reachable from the VM at `192.168.64.1`, the default `HOST`):
copy `target/updater-test/old/Vocet.app` to `~/Applications`, open it,
menu-bar icon > **Settings…** > General > **Check now** > **Install and restart**. The app
restarts as 0.1.1 (the log says `starting version="0.1.1"`).
`tests/e2e/scenarios/e2e_update_keeps_permissions.sh` does all of this in the
VM, on its own throwaway key and a fixture server inside the VM. On your own Mac use
`HOST=127.0.0.1 scripts/updater-test.sh build`.

**Permissions kept (M5.4 #4):** grant Microphone and Accessibility to the 0.1.0
build first (use `scripts/build-dmg.sh` signing, i.e. the `dictation-app Dev`
certificate, so both builds share a signature), dictate once, update, and
dictate again with no new prompt and no entry missing from System Settings >
Privacy & Security. An ad-hoc build is expected to lose them.
