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

- **The app** is signed by the identity in `scripts/release-signing.conf`
  (`RELEASE_IDENTITY`). While that is empty it is `dictation-app Dev`
  (`scripts/make-dev-cert.sh`), not notarised: testers clear the quarantine flag
  (see the tester guide). Once it names a Developer ID certificate, the app and
  DMG are notarised and stapled too, and a release stops rather than falls back
  if that certificate can't sign. See "Developer ID" below.
- **Every build uses the hardened runtime** (`hardenedRuntime` in
  `src-tauri/tauri.conf.json`, which debug builds and the e2e suite use too), and
  `scripts/release.sh` refuses an app without it.
- **CI has no certificate**, so its DMG is ad hoc: good for checking that
  packaging works, not for handing to testers who should keep permissions.
- **The updates** are signed with the minisign **private** key (a different
  thing: it proves an update came from us). Only its public half is in the
  repo. A different Apple certificate doesn't change it.

### Entitlements: why each one is there

The hardened runtime blocks some things unless an entitlement allows them. The
app has one (`src-tauri/Entitlements.plist`); `scripts/test-release.sh` fails if
the file gains another, and `scripts/release.sh` fails if a built app's list
differs from the file.

| Entitlement | Why | |
| --- | --- | --- |
| `com.apple.security.device.audio-input` | Recording. Without it the hardened runtime answers "denied" with no prompt. | Kept |

Each entitlement below was looked at and left out. The evidence is the full VM
suite passing without it, under the hardened runtime (N1's pull request has the
results). `e2e_local_model` runs the on-device engine, Metal included, and
`e2e_update_privileged_install` takes the updater's administrator path. Add an
entitlement only when a test fails without it.

| Left out | Why it isn't needed |
| --- | --- |
| `cs.allow-jit`, `cs.allow-unsigned-executable-memory` | WKWebView's JavaScript JIT runs in Apple's own WebContent process, which has its own entitlements. Nothing in this process generates code: the on-device engine (`transcribe-cpp`) runs compiled C++ and Metal shaders, and macOS's own service compiles those. |
| `cs.disable-library-validation` | The bundle holds exactly one executable, `Contents/MacOS/Vocet`. `transcribe-cpp` and every other native library are linked into it statically, and it links only system frameworks and `/usr/lib` (`otool -L`). No third-party library is loaded. |
| `cs.allow-dyld-environment-variables`, `cs.disable-executable-page-protection` | Nothing uses them. |
| `automation.apple-events` | The app sends no Apple Events to another app. The updater's fallback for a bundle it can't write (`do shell script … with administrator privileges`) runs AppleScript inside this process. |
| App Sandbox, `keychain-access-groups`, network, Accessibility | The app isn't sandboxed, so network and files need nothing. The key is in the login keychain, which needs no group. Accessibility and the event tap are granted by the person in System Settings (TCC), and no entitlement exists for them. |
| `get-task-allow` | Debugger access. Apple refuses it in a notarised app. |

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

   It builds the release signed by the configured identity (see "Signing, in
   one place"), reads the two Keychain items into the build's environment only
   (filtered from its output, never written anywhere), checks the app's
   signature, hardened runtime and entitlements, checks the DMG
   (`scripts/check-dmg.sh`), notarises and staples if the identity is a Developer
   ID, checks that the updater bundle and the DMG hold that same signed app,
   verifies the updater bundle's `.sig` against the public key in
   `tauri.conf.json` and that it is bound to the version, and writes everything into
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

## Developer ID: what the owner does once Apple approves the order

You do all of this on your own Mac. No agent reads your certificate, your
password or your Keychain, and none of it goes in the repo. Your name and team
ID aren't secret (they are in every signed app), so they may go in
`scripts/release-signing.conf`.

Allow about half an hour, plus Apple's notarisation time (usually a few minutes,
sometimes much longer the first time).

### 1. The certificate

You need the Xcode command-line tools (`xcode-select --install`) and your
membership approved: you get an email, and <https://developer.apple.com/account>
then shows your membership and a Team ID, 10 letters and digits. Only the
**Account Holder** can create this kind of certificate.

1. In **Keychain Access**, choose **Keychain Access > Certificate Assistant >
   Request a Certificate From a Certificate Authority…**. Enter your email and
   name, leave CA Email empty, choose **Saved to disk**, and save the
   `.certSigningRequest` file. This creates your private key, in your login
   keychain. It never leaves your Mac unless you export it.
2. At <https://developer.apple.com/account/resources/certificates/add>, choose
   **Developer ID Application** (not "Developer ID Installer", and not "Apple
   Development"), choose the **G2 Sub-CA** profile if asked, and upload that file.
3. Download the `.cer` file and double-click it. It joins your login keychain.
4. Check it. This lists names only, no secrets:

   ```sh
   security find-identity -v -p codesigning
   ```

   You should see `"Developer ID Application: Your Name (ABCDE12345)"`. If it
   isn't listed, or says `CSSMERR_TP_NOT_TRUSTED`, install Apple's intermediate
   certificate "Developer ID - G2" from
   <https://www.apple.com/certificateauthority/>.
5. **Back up the private key.** In Keychain Access, under **My Certificates**,
   right-click the certificate and choose **Export…**. Save the `.p12` with a
   strong password somewhere you keep backups, beside the update key's backup. A
   new Mac needs it.

### 2. The notarisation credential

The credential goes into your Keychain as a named profile, and from then on
`scripts/release.sh` only uses the name. The script can't read what is in it.

1. At <https://account.apple.com>, under **Sign-In and Security > App-Specific
   Passwords**, create one named `vocet-notary`. Copy it for the next step only.
2. Store it. Leave `--password` off this line, so that it asks and the password
   doesn't land in your shell history:

   ```sh
   xcrun notarytool store-credentials vocet-notary --apple-id you@example.com --team-id ABCDE12345
   ```

3. Check it. It lists your past submissions, which is an empty list today:

   ```sh
   xcrun notarytool history --keychain-profile vocet-notary
   ```

If Apple says an agreement is missing, accept the latest one at
<https://developer.apple.com/account>. `vocet-notary` is the profile name
`scripts/release-signing.conf` already has.

### 3. A test signing run

Nothing here publishes anything. On a clean checkout of `main`, with the version
that is in `tauri.conf.json`, and with your own identity in place of the example:

```sh
export RELEASE_IDENTITY="Developer ID Application: Your Name (ABCDE12345)"
scripts/release.sh 0.1.0 --notes "Test signing run." --no-notarize   # signing only
scripts/release.sh 0.1.0 --notes "Test signing run."                 # and Apple's notarisation
unset RELEASE_IDENTITY
```

The first command stops within minutes if the certificate or a build setting is
wrong, without sending anything to Apple. The second sends the app and the DMG to
Apple, which only records that it checked them. macOS may ask once to let
`codesign` use the new key: choose **Always Allow**.

If Apple refuses something, the script prints Apple's list of problems. Paste
that into an issue; it holds no secrets.

### 4. Verify

The script has done these checks already. Running them yourself once shows what
"good" looks like:

```sh
cd target/release-out/v0.1.0
hdiutil attach Vocet_0.1.0_aarch64.dmg -nobrowse -mountpoint /tmp/vocet-dmg
codesign -dvv /tmp/vocet-dmg/Vocet.app 2>&1 | grep -E 'Authority|TeamIdentifier|flags'  # your name, your team, runtime
xcrun stapler validate /tmp/vocet-dmg/Vocet.app                                         # The validate action worked!
spctl --assess --type execute --verbose=2 /tmp/vocet-dmg/Vocet.app                      # source=Notarized Developer ID
hdiutil detach /tmp/vocet-dmg
xcrun stapler validate Vocet_0.1.0_aarch64.dmg
spctl --assess --type open --context context:primary-signature --verbose=2 Vocet_0.1.0_aarch64.dmg
```

Then the real test. Put the DMG somewhere a browser would download from and
download it, or send it to yourself, on a Mac (or Mac user account) that hasn't
run Vocet. Drag it to Applications and open it. macOS should ask only whether to
open an app from the internet.

### 5. Switch to it

In one pull request (**the first notarised release, not before**):

- Set `RELEASE_IDENTITY=Developer ID Application: Your Name (ABCDE12345)` in
  `scripts/release-signing.conf`.
- In `docs/TESTERS.md`, replace section 2 with the version in the comment marked
  `SWITCH AT THE FIRST NOTARIZED RELEASE`, take "Not notarised" out of "Known
  limits", and update "How updates arrive" (below).
- In `CHANGELOG.md`, tell people about the permissions; see "Moving testers to
  Developer ID".
- Tell `docs/ENGINEERING.md`, "Signing".

From then on, a release that can't sign or can't notarise stops: it never ships a
build under the old certificate.

Also close the last box of #206 with a VM run: the DMG with the quarantine flag
set, opened with no `xattr` step. That needs a real notarised build, so it can't
run before this.

### When the certificate expires

A Developer ID certificate lasts 5 years. Apple emails you before it ends, and
`scripts/release.sh` stops with a clear message once it has.

- **Copies people already have keep working.** Notarised, timestamped apps go on
  opening after the certificate ends. Only signing something new stops.
- **Make a new one**: repeat step 1, set the new name in
  `scripts/release-signing.conf` if it changed, and do a test signing run. Your
  team ID is the same, and Developer ID permissions follow the team, so people
  should not lose their permissions (check it with the steps in "Confirm #259").
- **Don't revoke** the old certificate just because it is old. Revoke only if the
  private key leaks: revoking can stop what you already signed from opening, so
  talk to Apple first.
- **If your membership lapses**, the same is true of existing copies, but you
  can't sign or notarise until you renew.
- **The app-specific password** stops working if you change your Apple ID password
  or revoke it. Repeat step 2.

### Confirm #259 (no Keychain prompt on update)

Today every update makes macOS ask once for the saved key, which looks like a
result of the self-signed certificate. To find out, make two real Developer ID
builds that update one to the other, and try it in the VM. The update key here is
the throwaway one, not yours, and these two builds don't need notarising, since a
file copied over ssh has no quarantine flag:

```sh
APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (ABCDE12345)" \
  HOST=127.0.0.1 PORT=18741 scripts/updater-test.sh build
```

1. Start the VM (`lume run dictation-vm`, with its window), and copy
   `target/updater-test/old/Vocet.app` to `~/Applications` in it, and the
   `target/updater-test/serve` folder too.
2. In the VM, in that `serve` folder, run `python3 -m http.server 18741 --bind 127.0.0.1`.
3. Open the app, allow what it asks, and save a key in Settings (a made-up
   `gsk_…` will do if you click **Save** past the test failure, or a real one).
4. Quit it, open it, and see that it asks nothing. That is your "before".
5. In Settings > General, **Check now**, then **Install and restart**.
6. **Does macOS ask about the key?** With no prompt, and with the key still shown
   as saved, #259 is fixed: close it. If it does ask, say so on #259 with a
   screenshot, and keep the tester guide's "Always Allow" note.

Also check that both permissions stayed, with `codesign -dr - ~/Applications/Vocet.app`
showing a requirement with your team ID rather than one certificate.

## Moving testers to Developer ID

The first Developer ID release is signed by a different certificate from the
builds testers have, and macOS treats it as a different app for permissions. It is
a one-time step for each tester. This was tried in the VM with a stand-in
certificate (`tests/e2e/scenarios/e2e_update_to_new_certificate.sh`):

1. The update installs and the app restarts by itself. The update key doesn't
   care about the certificate, and no quarantine flag comes with an update.
2. **Both permissions are gone**, because macOS ties them to the certificate. Vocet
   notices at launch and opens its setup window at Permissions. It says macOS is
   asking again and that settings, history and the key are all still there.
3. macOS asks once for the Keychain password for the saved key. Choose **Always
   Allow**. Until that is answered the app can't read the key, and it doesn't send
   anyone to add a new one.
4. **Microphone:** **Allow**, and macOS asks as if for the first time.
5. **Accessibility is the awkward one.** System Settings still shows Vocet turned
   on, but that row means the old signature. Turning it off and on doesn't help
   (tried, in the VM). The person selects it, clicks **−**, clicks **Allow** in
   Vocet, and turns on the new row. The window says so. The hotkey then starts by
   itself, with no relaunch and no reinstall, and **Done** closes the window.
6. Settings and history were byte for byte the same in that test, the key read
   correctly, and launch at login stayed on.

Put this in the first Developer ID release's `CHANGELOG.md` section. It appears in
the "Update available" dialog as well as on the release page:

> Vocet is now signed with an Apple Developer ID, so a new install opens without
> the Terminal step. **After this update, macOS asks for your permissions once
> more:** Vocet opens its setup window and shows you what to do. Your settings,
> history and saved key are all kept. macOS may also ask once for your Keychain
> password: choose Always Allow.

### What real Developer ID may do differently

The stand-in is a second self-signed certificate. What can't be seen until the
real one exists:

- **Later updates.** Developer ID permissions follow the team ID, not one
  certificate, so they should survive updates and certificate renewals, and the
  Keychain prompt on each update should stop (#259). A self-signed certificate
  can't show this.
- **Gatekeeper.** Nothing in the VM test is notarised. A notarised DMG with the
  quarantine flag gets the "downloaded from the internet" question; that needs the
  run in step 4 above. Updates arrive with no quarantine flag either way.
- **Login Items.** macOS tracks these partly by team, and a self-signed build has
  none. It may show Vocet as a new item under your developer name, or ask people
  to approve it again. Check it on the first real update.
- **The notary.** A real submission can refuse something a local check can't see.
  `scripts/test-release.sh` fakes `notarytool`, `stapler` and `spctl`, so real
  answers, real tickets and real timing have never been through the script.

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
