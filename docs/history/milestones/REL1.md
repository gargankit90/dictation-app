# REL1: release prep for 0.1.0

Milestone "0.1 Alpha (friends)". Nothing has been released yet, so the
changelog, the tester guide and the README must read right to a first-time
tester. Docs only: no product code, no change to `scripts/release.sh`, no
publishing. The owner picked the release date, 2026-11-12, so the heading is
`## [0.1.0] — 2026-11-12`. (Criterion 5 was added later: the owner also
approved the `release.sh` change below.)

`scripts/release.sh` uses the `[0.1.0]` section's text (not its heading) as
the GitHub release notes and the "Update available" text, and refuses a
section still dated `[date set at release]`; this one has the real date.

## Acceptance criteria

1. **[auto]** `CHANGELOG.md`: everything in `[Unreleased]` is folded into one
   `[0.1.0]` section written for a first-time tester: what Vocet does, what
   is in this release (on-device or own-key speech, setup, history, menu bar,
   Settings, Troubleshoot and Report, updates, anonymous usage sharing with
   its off switch) and known limits. Plain text, short lines, no HTML, no
   "Changed/Fixed" relative to an earlier version, no rename or "after this
   update" lines. An empty `[Unreleased]` heading sits above it. The heading
   is `## [0.1.0] — 2026-11-12`.
2. **[auto]** `docs/TESTERS.md` matches the app at this commit: install
   (notarized DMG), setup, the dictation key, Settings, Help, how updates
   arrive, privacy and the sharing switch. Known limits include: other keys
   while holding the dictation key drop the recording without a message
   (#315, fixed after 0.1.0); a long dictation on an on-device model can take
   a while and has no time limit yet (#316). The "Always Allow" Keychain note
   stays until the Lead updates it from #259's result.
3. **[auto]** `README.md`: nothing says the app is unreleased or not
   notarized; it points testers to the GitHub release and `docs/TESTERS.md`.
4. **[auto]** Every claim checks out against the code and `docs/privacy.md`.
   `scripts/test-release.sh` and the gate pass.

5. **[auto]** `scripts/release.sh` stops, before it builds, when either
   Keychain item (`dictation-app-posthog-key` or `dictation-app-sentry-dsn`)
   is missing, with a clear message naming the missing item and pointing to
   `docs/releasing.md` "The telemetry keys". It checks presence only and
   never prints, logs or writes a value. A new flag `--no-telemetry` builds
   without them on purpose (with a warning); `--no-telemetry` together with
   `--publish` is refused. `scripts/test-release.sh` covers both present, each
   missing, `--no-telemetry`, and `--no-telemetry --publish` refused, using
   its fake `security` stub (never the real Keychain). `docs/releasing.md`
   "The telemetry keys" says the script now stops instead of warning.

## Out of scope

Other `release.sh` changes, product code, publishing.
