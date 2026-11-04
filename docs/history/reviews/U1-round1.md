Reviewer: CHANGES REQUESTED

# U1 review, round 1

PR: #250 at 43dbf3a8aa6f7fe50bd51a1a9fe682d947b9f413
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | blocker | crates/core/src/diagnostics.rs:135 | The new microphone fix removes “then relaunch.” On supported macOS 15, enabling Microphone does not update the running process's authorization (documented in ENGINEERING.md). Following this instruction alone leaves dictation broken. | Restore explicit relaunch advice and a regression assertion; retain the correct pane and omit the code. |
| 2 | major | scripts/updater-test.sh:55 | Both updater fixtures now use Vocet.app. Together with the ordinary e2e installer deleting the old bundle, this leaves the actual old-name transition untested. The settings/key unit test uses MemSecretStore and cannot establish OS grants or login registration survival. | Add an old-product-name/path to Vocet archive transition test, with the same signing identity and bundle identifier, existing settings/key and enabled login item. Assert restart, retained grants/settings/key and a usable login target without duplicate installs. Validate the documented DMG migration separately before promising permissions survive moving paths. |

Inline threads: [1](https://github.com/gargankit90/dictation-app/pull/250#discussion_r4464460301), [2](https://github.com/gargankit90/dictation-app/pull/250#discussion_r4464461415).

Counts: 1 blocker, 1 major, 0 minors. No tech-debt issues needed.

## Review evidence

- The identifier remains `com.dictationapp.desktop`; Info.plist changes only the purpose string. Signing identity, app-data lookup, Keychain service and account selection, and log prefix are unchanged. No observed settings/key identifier regression.
- The updater endpoint, public key, manifest platform lookup and executable name remain unchanged. Release packaging consistently uses Vocet filenames. Inspection of the installed tauri-plugin-updater macOS implementation shows it strips the archive's top-level name and replaces the existing bundle path: an in-app update should keep `dictation-app.app`, rather than create a second Vocet.app. This is source evidence, not a VM transition test. The guide's separate DMG migration changes paths and needs the evidence in finding 2, including login behavior.
- Settings really labels the section “Transcription”; that copy deviation is appropriate. Details/report keep codes; analytics error mapping and privacy allow-lists are unchanged. Overlay payload and tooltip omit codes.
- Progress consumes pipeline events. Normal Inserting-to-Idle follows InsertDone; insertion errors arrive before Idle and prevent completion. Typing does not affect completion. The existing paste adapter cannot prove target-app delivery; changing that adapter is outside U1. A later attempt can replace the displayed stage list after a prior success, but completion still requires a real successful pipeline run.
- The short menu-bar title is an explicitly disclosed deviation preserving the existing macOS width constraint; the tooltip has the full sentence. Design Lead acceptance remains needed.
- Passed locally: core diagnostics (8 tests), Onboarding/Troubleshoot/Overlay (60 tests), release-script stub suite. Full scripts/gate.sh passed, including workspace tests (1,214 passed, 2 skipped), coverage thresholds, frontend tests/build, guards and shell helpers. Standalone runner helpers also passed (102 runner checks and 32 CI-runner checks).
- No VM GUI was driven in this review; previous-build permission and login survival remain unverified here.

## Checklist

1. Scope: finding 2; auto criteria largely map to code/tests; manual screenshots incomplete as disclosed by author.
2. Correctness: finding 1; no new pipeline cancellation/race defect established.
3. Security and privacy: OK; no new secret/transcript exposure, destinations or analytics fields; command permission added only to main window.
4. Architecture: OK; progress logic lives in app, commands remain thin; no new dependencies, unsafe or OS conditionals.
5. Rust and TypeScript practice: OK on reviewed changes; typed IPC and existing lock discipline retained.
6. Licences and reuse: OK; no dependency or adapted-code additions.
7. Tests: finding 2; the fix-verb test passes despite finding 1, so add a semantic relaunch assertion.
8. Git and PR: OK; Conventional Commits, detailed notes and disclosed deviations. No guard changes. Do not merge this round.
