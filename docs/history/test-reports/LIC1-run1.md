# MLIC1 test report, run 1

PR: #333 at ccb6d2e
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | LICENSE is the unmodified FSL-1.1-ALv2 template | diff against https://fsl.software/FSL-1.1-ALv2.template.md | PASS | Only difference is line 9: `Copyright 2026 Ankit Garg (Vocet Labs)`. The template has two placeholders (year, licensor name), not three. |
| 2 | No claim that Vocet is MIT; metadata says FSL | `git grep -i -E "\bMIT\b\|open[- ]source"` excluding third-party files; Cargo/npm inspection | PASS | The only remaining matches are third-party: TESTERS.md, gen-licences.sh, HelpAbout "Credits". README says versions up to 0.1.0 stay MIT. `Cargo.toml` and `package.json` say `FSL-1.1-ALv2`; all crates and src-tauri use `license.workspace`. `src/bindings.ts` carries the FSL comment. |
| 3 | scripts/gate.sh passes | `scripts/gate.sh --full` (foreground) | PASS | "gate: all checks passed" |
| 4 | Help & About shows FSL line | `src/settings/HelpAbout.tsx:133`; `src/Settings.test.tsx:1274` asserts `/FSL-1\.1-ALv2/` | PASS | Covered by the gate's frontend tests. |
| 5 | Lead reads site and README | n/a | Lead's | Already PASS on the PR. |

## Gate
scripts/gate.sh --full: pass. No e2e or VM run: text-only change.

## Untested behaviour found
None.

## Failures
None.
