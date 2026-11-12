# REL1 review, round 1

PR: #320 at 6d05a80956d5333184c9d6d6ad3b6adc87df9909
Verdict: APPROVED

## Findings

No blocker, major or minor findings.

## Checklist

1. Scope: OK. REL1 criteria 1–5 map to the documentation and telemetry preflight.
2. Correctness: OK. Either missing or empty telemetry value stops before building
   or submitting anything for notarization. Publish requires telemetry; argument
   parsing refuses the opt-out with publish before Keychain access. Dirty-tree,
   existing-tag and no-notarize publish refusals remain in place.
3. Security and privacy: OK. New diagnostics print service names only. Values are
   captured with stderr suppressed and reach the build through its environment;
   build output is redacted. No new value file or logging path. The opt-out
   removes inherited telemetry variables. Privacy text matches docs/privacy.md.
4. Architecture: OK; no product architecture changes.
5. Rust and TypeScript practice: OK; no Rust or TypeScript changes.
6. Licences and reuse: OK; no new dependencies or adapted product code.
7. Tests: OK. bash scripts/test-release.sh passed, using a throwaway repository,
   freshly generated test keys and fake security, Apple tools, npm and gh.
   Assertions check build markers, manifest creation, refusal messages and
   output redaction; they exercise behavior rather than merely matching source.
   bash -n passed for both scripts; Prettier passed for the changed release docs.
8. Git and PR: OK. One scoped Conventional Commit and acceptance mapping.
   The milestone records owner authorization for the release-script extension.

## Documentation checks and limits

Checked setup and Settings labels against src/Onboarding.tsx, src/onboarding,
src/Settings.tsx and src/settings; model sizes against the model catalogue;
menu actions against crates/app/src/tray.rs; update behavior against
src-tauri/src/updater.rs; privacy against docs/privacy.md. Release notes remain
readable without HTML and describe the first release. Issues #315 and #316
remain documented limits; #259's Always Allow advice is retained as specified.

The actual release script was not run against the owner's environment. No real
Keychain item, updater key or Apple credential was read. Only its isolated copy
inside test-release.sh ran with fake tools. No build, notarization or publishing
was performed. Required Linux and macOS CI checks were pending at review time;
this review does not replace CI or the Lead's manual release checks.
