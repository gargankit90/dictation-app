# MSIMP1-A review, round 1

PR: #345 at 1faf2c9eaad6c9a1db5562bb341c4f6b293e4ea2
Verdict: CHANGES REQUESTED

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | tests/e2e/scenarios/e2e_onboarding_five_steps.sh:70 | Restart expects Welcome despite having saved local settings at line 61. `Onboarding::load` treats settings existence as Welcome passed and initializes at Ready; opening clamps to the first unmet step, which cannot be Welcome here. The subsequent Get started action also cannot work. This prevents the remaining functional suite from passing. | Expect the actual first unmet step, return explicitly to Speech to restart the partial download, and run the scenario in the VM. Keep the simplified loader and manual download resume. |
| 2 | minor | crates/app/src/history.rs:406 | Deleting the practice commands leaves `History::attempted` with only test readers. The preceding line still clones every transcript into this unused cache. | Delete the attempted field/accessor/writes and cache-specific tests, retaining normal History and Copy last. Follow-up: [#346](https://github.com/gargankit90/dictation-app/issues/346). |

Both findings have inline PR comments. Finding 2 does not block approval.

## Checklist

1. Scope: OK for PR A; typed errors, remaining network cleanup, single-instance and launch-at-login simplification explicitly belong to PR B. Owner-authorized removal of migrations and progress persistence needs no compatibility layer.
2. Correctness: finding 1. The single completion marker, permission-loss reopening and in-memory step guards remain coherent on inspection. Restarting unfinished setup at the first unmet step is intentional.
3. Security and privacy: OK. Removed telemetry variants match the privacy table deletions; Sentry remains separate. No new network destinations, permissions or sensitive logging found. Clipboard restoration and secure-input handling remain. Finding 2 is unused memory retention, not a new disclosure.
4. Architecture: OK; no new crate dependency, platform conditional or unsafe block. Shared integration stand-ins move into test-only common modules.
5. Rust and TypeScript practice: OK for the changes reviewed; finding 2 is a simplification follow-up.
6. Licences and reuse: OK; no added dependency or externally adapted implementation found.
7. Tests: finding 1. Merged clipboard tests retain state-sequence, paste, log-privacy and restore assertions; merged silence tests retain no-insert and clipboard checks. Removed fake self-tests and language/mic/progress tests concern retired behavior. History, Help/report, Welcome animation, retry/jitter, launch at login, paste-key fallback and real-OpenRouter scenario remain represented in production paths and surviving tests. Required CI names stay intact and removed plain macOS builds are covered by the corresponding clippy/nextest passes.
8. Git and PR: OK; detailed per-item notes and explicit deferrals. No guard modification. No merge performed.

## Validation and limits

- Installed locked frontend dependencies with `npm ci`.
- Focused Vitest run: Onboarding, App, Settings, onboarding logic and History: **5 files, 201 tests passed**.
- Restart finding established by tracing the scenario's saved settings through `Onboarding::load`, `with`, `open` and `Readiness::resume_at`; the VM scenario was not run by this reviewer.
- Full quality gate and VM e2e are delegated to existing CI/Tester evidence; CI was still running when inspected. No claim of a full local gate or VM pass.
- Reviewed against the SIMP1 spec and checklist, with priority on kept features, onboarding, telemetry consistency, deleted-test coverage and leftovers. No request to restore a removed product feature.
