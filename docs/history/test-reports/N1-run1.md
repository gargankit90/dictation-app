# N1 test report, run 1

PR: #309 at 32e04a2
Result: FAIL (one blocker: the required Linux gate is red; everything else passed)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | Hardened runtime, fewest entitlements | `codesign` on release bundles built here from `main` (3d26c1d) and from the head; `scripts/test-release.sh`; `e2e_release_build_smoke` | PASS | Both bundles: `flags=0x10000(runtime)`, entitlements exactly `com.apple.security.device.audio-input`. So the Developer is right: `main` was already hardened (Tauri's default); the PR makes it explicit and enforced. Debug bundle of the head: same. |
| 2 | Works hardened | `e2e_release_build_smoke`, `e2e_local_model`, `e2e_launch_at_login`, `e2e_update_keeps_permissions`, `e2e_update_privileged_install`, `e2e_onboarding_first_run`, `e2e_microphone_denied_then_recover`, `e2e_accessibility_revoked` | PASS | All pass on `dictation-vm-2`. Not the whole suite (not asked). The privileged-install path works with no Apple Events entitlement. |
| 3 | Identity from configuration | `scripts/test-release.sh` plus extra cases (below) | PASS | Empty config: fallback. Configured identity missing or expired: stops, no fallback. Missing profile: stops before the build and before the Keychain. A quoted identity in the file works. |
| 4 | Notarize, staple, verify | `scripts/test-release.sh` plus extra cases (below) | PASS with a caveat | Every Apple answer other than Accepted stops the release. See "Untested". |
| 5 | Move to another certificate | `e2e_update_to_new_certificate` x3 | PASS (3/3) | See below. |
| 6 | [manual] | - | left to the Lead | |
| 7 | Both first launches in `docs/TESTERS.md` | `scripts/test-release.sh`; read the diff | PASS | The current quarantine steps are untouched; the notarized version is in a comment marked `SWITCH AT THE FIRST NOTARIZED RELEASE`. The "How updates arrive" text has the exception for differently signed updates. |

## Gate

- `scripts/test-release.sh` passes on this Mac (all 19 cases, 22 s).
- `npx vitest run src/Onboarding.test.tsx`: 124 passed.
- **CI at 32e04a2: `Quality gate (Linux)` FAILS**; `DMG (macOS)` and `Platform crate (macOS)` pass. Not run here: coverage and the Rust unit tests (CI has them).

## Failures

### F1 (blocker, a required check): `scripts/test-release.sh` fails on Linux, "no message for a changed bundle"

`Quality gate (Linux)` run 42337864443, job 128298763493. Section 3, `FAKE_TAMPER=1` (bytes appended to the updater bundle after signing). The release is refused, but not with the minisign message the test expects. `release.sh` now unpacks the bundle (`archive_check`, `tar -xzf`) before it verifies the minisign signature, and GNU tar stops at the appended bytes (`gzip: stdin: decompression OK, trailing garbage ignored` then `tar: Child returned status 2`). macOS's tar accepts them, so the test passes on this Mac and in the macOS jobs.

Why it matters beyond CI: the person running `release.sh` gets a bare tar error for a tampered or damaged bundle instead of "does not verify against the public key". Fix options for the Developer: run `verify_update` on the built bundle before `archive_check` (it is run on the final bundle afterwards anyway), or make the test accept the tar failure.

Reproduce: `gh run view 42337864443 --log-failed`.

## Checks beyond the Developer's tests

Run by copying `scripts/test-release.sh` to a scratch file and adding cases (not committed). With a fake notary that prints a credential on every call:

| Case | Result |
| ---- | ------ |
| App status `In Progress` (what a notary timeout looks like), `Invalid` | refused |
| DMG status `Invalid` or `In Progress` after the app was accepted | refused |
| Staple fails on the DMG only (app stapled fine) | refused |
| notarytool gives no answer; spctl rejects | refused |
| After every refusal | `target/release-out/v<version>/` is empty: no DMG, bundle, `latest.json` or `SHA256SUMS` left looking finished |
| Fake credential and fake update key, grepped in the whole output of each run | never printed (also under `bash -x`, by the Developer's test) |
| Identity in the configuration file in quotes | works |
| Explicit `dictation-app Dev` in the configuration | builds, no notarization |
| `RELEASE_IDENTITY=""` in the environment | the file's identity is used |
| Configured `Some Other Cert`, only the dev certificate in the keychain | stops: "not falling back" |

The updater bundle and the DMG are rebuilt around the stapled app and both are checked for the same CDHash, identity, runtime and entitlements. `latest.json`'s signature equals the new `.sig`, and minisign is checked on the rebuilt bundle (Developer's test; I read it and it passes here).

## Criterion 5, `e2e_update_to_new_certificate`, three runs

| Run | Result | Path |
| --- | ------ | ---- |
| 1 (with build) | PASS | direct: the running app saw the Accessibility grant, no relaunch |
| 2 | PASS | direct |
| 3 | PASS | direct |

So the Relaunch path was taken 0 of 3 times; the Developer saw it 1 of one full run. I did not find the cause either and could not reproduce it. I did not force the Relaunch path; the Developer says they did once. Each run: the update installs, setup opens at Permissions, one Keychain prompt, the microphone prompt, Accessibility recovered as the notice says (remove the old row with minus, Allow, turn on the new row), Done closes setup, settings and history byte for byte the same, the key reads, a dictation and a recording work, launch at login stays on.

### Would a person following the window's words get unstuck?

I read the window (screenshots in the run output, `e2e_update_to_new_certificate-notice.png`).
- **Direct path: yes.** It says "macOS is asking for these again", why (an update that changes how Vocet is signed, or a permission turned off), that settings, history and the saved key are still there, and the three steps for the stale Accessibility row. Done unlocks when both are allowed and says so.
- **Relaunch path: probably.** The card says "macOS says it's allowed, but Vocet can't use it until it restarts. Your choices are kept", with a **Relaunch and resume** button. The Developer's unit test and a forced run show the notice and Done survive the relaunch. I did not exercise it.
- **Wrong order (minor, M1):** someone who turns the old, stale row off and on, instead of removing it, sees "Turned it on and nothing changed? A relaunch makes it take." A relaunch won't help there; the line above the cards about removing the old row is the only thing that does. It's still on screen, so a careful reader gets there, but the two hints disagree.
- A first-time setup shows no notice (`e2e_onboarding_first_run` passes; the vitest cases and the Rust test check that `asked_again` is off on a first run). A person who turned a permission off on purpose after finishing setup gets the same notice and a Done button; the text covers that case ("or when a permission was turned off") and nothing in it is false for them. Only the unit tests cover this; no e2e reads the notice for that case.
- Minor (M2): `asked_again` is stored the first time `status()` sees a permission not granted after setup was completed. A momentary non-granted read while the window is open would leave the notice up (and Done instead of Continue) until setup is finished. I did not see it happen.

## Untested behaviour found

- **Anything with Apple** (real `notarytool`, `stapler`, `spctl`, real tickets, Gatekeeper on a quarantined notarized DMG, what the notary may refuse, timing): only fakes. Their output shapes are assumed: `notarytool submit --output-format json` giving `.status` and `.id`; `stapler validate` on a copied DMG and on an unpacked updater bundle; `spctl --assess` printing `source=Notarized Developer ID` for the app and the DMG.
- **`scripts/test-release.sh` fakes `codesign`, `hdiutil` and `ditto`.** The real `rebuild_dmg` (convert to writable, replace the app, convert back, `codesign` the DMG) and the CDHash comparison were run only by the Developer on the host. I did not repeat that run: it needs the real update key's Keychain items, which I must not read.
- **The real Developer ID certificate**: its secure timestamp and team-based permissions. #259 (Keychain prompt on update) cannot be tested before a real certificate exists.
- **A release build with no test flags** covers hotkey, mic, overlay and Settings only (`e2e_release_build_smoke`). Transcribing under the hardened runtime was run with the debug build (`e2e_local_model` passes).
- **The whole VM suite** was not rerun. Criterion 6 (`docs/releasing.md`) is the Lead's.

## Environment note

One `run.sh` start failed with `Connection closed by 192.168.64.7 port 22` while turning off Terminal's window restoration; an immediate rerun was fine. VM flake, not the PR.
