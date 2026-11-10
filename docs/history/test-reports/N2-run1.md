# N2 test report, run 1

PR: #313 at 13e6475
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | conf names Developer ID and `vocet-notary`; no fallback; gate passes without the certificate | `scripts/test-release.sh` (all checks ok, 28 s); CI run | PASS | The throwaway repo uses an empty identity, so CI and forks without the certificate don't need it. "Configured certificate missing, expired or ambiguous stops the release; never falls back" passes. |
| 2 | `docs/TESTERS.md` section 2 notarized, Known limits, How updates arrive | read as a first-time tester; compared with the VM dialog | PASS | Section 2 matches the dialog. No `xattr` or Terminal steps remain; one fallback line ("tell us in a bug report"). |
| 3 | releasing.md, ENGINEERING.md, README, CHANGELOG consistent | read and grep | PASS | First test signing run (2026-11-09) recorded. 0.1.0 has no "Moving testers" step and says why. Minors below. |
| 4 | VM check of a quarantined notarized DMG | `E2E_VM=dictation-vm-2 tests/e2e/run.sh e2e_notarized_dmg_first_launch` | PASS | See below. Skipped with `E2E_NOTARIZED_DMG=` (empty); not in `ALL_SCENARIOS`, `e2e-main.sh` or CI. |
| 5 | PR notes close the loop on #206 | `gh pr view 313` | PASS | States what is verified and that #259 needs two real builds. |

## E2E on dictation-vm-2 (DMG read only: the owner's N1 test build)

- The DMG copied into the VM had `com.apple.quarantine` = `0083;...;Safari;` (read back with `xattr -p`).
- `spctl` on the DMG and on the app: `accepted source=Notarized Developer ID`.
- Signature verifies, TeamIdentifier XF2G42MVHN, hardened runtime. The VM has no `stapler`; the ticket is checked on the host by release.sh.
- The only `xattr` use in the scenario sets the flag. No Terminal or Open Anyway step.
- The person sees one dialog, from CoreServicesUIAgent: "“Vocet” is an app created by the app Safari. Are you sure you want to open it?" with the smaller text "Safari created this file today at 3:44 PM. Apple checked it for malicious software and none was detected." Buttons **Cancel** (default) and **Open**, plus a **?** help button.
- After Open was clicked, the app logged ready, one process ran, and no other prompt was left. The flag stays on the app as `00c3;...` (expected).
- Screenshots and the UI tree are in the local `tests/e2e/out/20261109-154349-dictation-vm-2/` (not committed).

## Gate
CI on 13e6475: Quality gate (Linux) pass, Platform crate (macOS) pass, DMG (macOS) pass. Not rebuilt locally (docs, scripts and one e2e scenario only; no coverage-relevant code changed).

## Untested behaviour found
- Nothing blocking. The scenario's own failure path (a different prompt, or no start) was not forced.

## Minors (no blockers)
- Prose lines in README.md (about line 21) and docs/TESTERS.md section 2 ("If macOS still refuses...") run long where the neighbours wrap at about 80 columns.
- The dialog sentence in TESTERS.md section 2 reads awkwardly: `"Vocet is an app created by the app Safari. Are you sure you want to open it?", or "downloaded from the Internet"`. Other browsers produce the second wording, so the document is still correct.
- Spelling of "notarised" and "notarized" is mixed across releasing.md and the scripts (it was mixed before this PR).

## Failures
None.
