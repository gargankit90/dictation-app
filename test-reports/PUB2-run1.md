# PUB2 test report, run 1

PR: #180 at 16b4363
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| --- | --- | --- | --- | --- |
| 1 | LICENSE standard MIT, "Copyright (c) 2026 Ankit Garg" | read LICENSE | PASS | Standard MIT text, correct holder and year. |
| 2 | Workspace licence inherited; third_party untouched; npm licence | `cargo metadata --no-deps` | PASS | All 6 original crates (core, ports, platform, transcribe, pipeline, dictation-app) report MIT via `license.workspace = true`. `publish = false` kept; package.json has `"license": "MIT"` and `"private": true`. `git diff origin/main...HEAD -- third_party` is empty. |
| 3 | deny.toml / about.toml comments; deny and notice pass | `cargo deny check`, `scripts/check-licences.sh` | PASS | advisories, bans, licenses, sources ok. check-licences regenerates THIRD-PARTY-LICENSES.txt and finds no diff (rc 0, worktree clean). Comments no longer say "no licence". |
| 4 | NOTICE credits kept; Handy MIT text; vendored licence files | diff vs main; ls third_party | PASS | NOTICE diff has additions only (no removed lines). Handy MIT text (Copyright 2025 CJ Pais) matches the Handy checkout's LICENSE. Licence files present in handy-keys (LICENSE) and coreaudio-rs (LICENSE-MIT, LICENSE-APACHE); NOTICE names them. |
| 5 | SECURITY.md | read | PASS | Private GitHub vulnerability reporting, scope, API keys only in Keychain. |
| 6 | CONTRIBUTING.md | read | PASS | gate.sh, Conventional Commits, PRs to main, maintainer runs VM e2e, AGENTS.md/.claude not required, no-GPL rule names FluidVoice and VoiceInk. |
| 7 | README | read, link check | PASS | Licence section, CI/VM paragraph, links to SECURITY/CONTRIBUTING, learning-project line linking AGENTS.md. Relative links resolve (AGENTS.md, CONTRIBUTING.md, SECURITY.md, LICENSE, NOTICE); the one external link is conventionalcommits.org. |
| 8 | AGENTS.md / CLAUDE.md | diff vs main; git grep | PASS | AGENTS.md: one line changed (reference-repo path reworded to "cloned next to this repo, in the same parent folder"); no rule changes. CLAUDE.md unchanged. No `/Users/` or `~/Documents` left in either. |

## Remaining path mentions (git grep '/Users/' and '~/Documents')

- Historical, fine: reviews/M1.7-round1.md, test-reports/M0.2-run3.md, M1.5-run1.md, and three M1.5-vm log files.
- Generic or test data, fine: `/Users/$VM_USER/...` in tests/e2e/* (VM user variable); `/Users/someone/...` fixture in crates/platform/src/single_instance/mod.rs; the PUB2.md spec itself.
- Minor, outside criterion 8: docs/review-checklist.md:66 still says "~/Documents/Dictation App/". It is a folder name, not an account name. Optional follow-up for neutral wording.
- No local account name in tracked docs; `gargankit90` appears only as the GitHub handle (release repo URLs), which the spec allows.

## Not run

Full gate (the push hook runs it). No VM needed.
