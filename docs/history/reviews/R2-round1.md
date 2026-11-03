Reviewer: CHANGES REQUESTED

# R2 review, round 1

PR: #230 at 8815d784c5a0ee01e32d69ac011ff10c02a2edf6
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | CHANGELOG.md:29 | “tested before it is saved” overstates Settings behavior. `src/Settings.tsx:196` saves independently of the Test key result; `crates/app/src/config.rs:136` only validates the string and writes it to SecretStore. A syntactically valid rejected key can be saved without testing. | Say the key is stored in Keychain and can be tested; restrict the mandatory test claim to onboarding. |
| 2 | major | CHANGELOG.md:75 | “Crashes in the rest of the app … are reported” promises broader crash coverage than ships. `crates/telemetry/src/errors.rs:293` records Rust panics through a panic hook, not arbitrary process crashes outside the engine (segfaults, aborts, or forced termination). `docs/privacy.md` correctly describes panics rather than all crashes. | Say Rust panics and failed dictations can be reported when sharing is on and the build has a Sentry DSN; retain the native-crash limitation. |

## Claim audit

- Dictation, configurable Right Option, overlay, Secure Input protection and five-minute cap: supported by settings/coordinator, pipeline and platform adapters. “Where your cursor was” should be read with the engineering rule that insertion uses focus at paste time; the app does not preserve the original target.
- Local models: catalogue contains Parakeet English default (477,274,496 bytes) and Qwen3-ASR (589,560,480 bytes, 30 languages including Hindi); Settings exposes downloads and local selection. Local transcription does not upload speech. Offline verification remains explicitly pending, consistent with L1-run2 and #222.
- Cloud providers, write-only Keychain storage, four onboarding steps and local “No key needed”: supported; mandatory key testing caveat is finding 1.
- Settings controls, Troubleshoot checks, optional provider connection test and preview/save/copy/browser issue report: supported by Settings, Troubleshoot, commands and report types. No automatic upload of the user-generated problem report.
- Privacy: default true, immediate shared off switch, allow-listed usage/error fields and absent audio/transcripts/keys/app names match core settings, usage wiring and telemetry adapters. Error reports can contain crate-relative source locations, as detailed in the linked privacy page. Build-time keys/DSN gate actual sending. Panic coverage caveat is finding 2.
- Updater: quiet launch check, explicit menu check, user-triggered installation, signature verification and restart match updater.rs. Apple Silicon release target, minimum macOS 13.0, self-signing without notarization and first-launch instructions match configuration/release docs; native crash capture remains excluded.
- README Try it and TESTERS requirements, local/cloud alternatives, privacy default and stated limits agree with these sources.

## Validation

- `npm ci` completed.
- `scripts/test-release.sh`: all eight groups passed using throwaway keys, fake Keychain/build/signing identity and stubbed gh; no owner Keychain reads or real publishing.
- `scripts/check-doc-links.sh`: 50 relative links passed.
- Reviewed whole diff: exact version token comparison avoids prefix/regex confusion; extraction stops at the next level-two heading or trailing link definitions, trims blank lines and preserves subheadings. Missing file/section/empty body errors precede builds and Keychain reads; explicit notes override and both manifest/release consumers are covered by stubs. Secret redaction and trace protections remain intact.
- Full gate is delegated to the normal push hook/CI; this review adds documentation only.

## Checklist

1. Scope: findings 1–2 concern the factual-accuracy acceptance criterion. The added milestone spec is acknowledged in PR notes as requested task scope.
2. Correctness: findings 1–2; release notes extraction and error paths OK for documented changelog format.
3. Security/privacy: OK; no new destinations or secret reads introduced by extraction.
4. Architecture: OK; no product architecture changes.
5. Rust/TypeScript practice: OK; no product code changes.
6. Licences/reuse: OK; no dependencies or adapted third-party code.
7. Tests: OK; stubbed release checks passed, including overrides and refusal cases.
8. Git/PR: OK; conventional commits and detailed PR notes; no guard changes. No minor findings, so no tech-debt issues needed. Do not merge this round.
