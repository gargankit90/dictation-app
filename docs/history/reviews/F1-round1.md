Reviewer: CHANGES REQUESTED

# F1 review, round 1

PR: #236 at b85feddbee918ac67a371df591f033e2f439533b
Verdict: CHANGES REQUESTED
Counts: 0 blocker, 2 major, 0 minor.

## Findings

| # | Severity | file:line | Finding | Suggested fix |
| - | -------- | --------- | ------- | ------------- |
| 1 | major | .config/nextest.toml:25 | nextest 0.9.149 warns that both profile.default.env and profile.ci.env are unknown and ignored. Standalone nextest therefore does not enable the promised fallback for platform code linked into integration or src-tauri tests, where platform's cfg(test) is false. The gate export masks this defect. | Use a supported test-scoped environment mechanism and verify it from a cross-crate test under standalone nextest, without gate.sh. Preserve ordinary dev/release behavior. |
| 2 | major | src-tauri/src/commands.rs:2571 | Iterating urls() passes with no recorded calls. The command loop also accepts any non-ACL open_issue error, including missing opener state or an early-return regression. This does not enforce AC1's requirement to assert the recorded URL. | Explicitly build a report, require open_issue success, and assert exactly one expected URL recorded by the fake. Check fake failure propagation separately. |

Inline threads: [1](https://github.com/gargankit90/dictation-app/pull/236#discussion_r4461850094), [2](https://github.com/gargankit90/dictation-app/pull/236#discussion_r4461850229).

## Checklist

1. Scope: findings 1 and 2 (AC2/7 safety net and AC1 regression coverage).
2. Correctness: production wiring OK by inspection. setup registers SystemOpener before tray callbacks; open_issue retains spawn_blocking and error mapping. Reveal log uses the same OS launcher through managed state. Privacy settings retains its existing URL and launcher. Keychain get/set/delete retain the original implementation after the guard.
3. Security and privacy: OK. No new destinations, sensitive logging, capability changes or secrets. Guard checks are runtime environment reads, not compile-time captures. cfg(test) is false for shipped dev/release libraries. Repository search finds the variable assigned only in gate.sh and the ineffective nextest entries, not npm tauri dev or release scripts. An inherited variable can disable these features, but requires control of the launching environment and returns ordinary errors; no privilege escalation or key disclosure identified.
4. Architecture: OK. Opener is a ports trait, the adapter and fake are in platform, and managed state is in the shell. No new unsafe code, target gates outside platform, or dependencies.
5. Rust and TypeScript practice: OK for changed production code. No new panic paths or suppressions without rationale.
6. Licences and reuse: OK; no dependencies or adapted external code added.
7. Tests: findings 1 and 2. Audited Config construction, Keychain uses, launch/login adapters, privacy settings, subprocess tests and host e2e helpers. Ordinary fixtures use temporary directories and MemSecretStore or the explicitly disabled typed store; login and permissions use fakes. The real Keychain integration test is ignored and explicitly hand-run. Host e2e unit harnesses stub VM/launcher operations; real scenarios execute through VM helpers. Single-instance tests use temporary runtime directories; default-path policy tests only inspect paths. No additional ordinary test path reaching real Keychain or app-data writes found.
8. Git and PR: OK; focused Conventional Commits and detailed notes. No protected guard file changes. No merge performed. No minor findings requiring issues.

## Validation

- Reproduced nextest's unknown-key warning locally with `cargo nextest show-config test-groups` (0.9.149).
- Hosted checks at reviewed head: Quality gate (Linux), Platform crate (macOS), and DMG (macOS) passed.
- Static review covers the whole diff and production/test call sites. Full gate is also required by the push hook; no GUI or real Keychain probe was run on the host.
