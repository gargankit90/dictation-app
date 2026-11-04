# R2 test report, run 1

PR: #230 at 8815d78
Result: PASS 

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | CHANGELOG 0.1.0 section | read as a new tester; claims spot-checked in code | PASS | 5-minute cap (`MAX_RECORDING`) and Right Option default (`hotkey_spec.rs`) match. Date placeholder as specified. |
| 2 | release.sh uses the section as notes | `scripts/test-release.sh` (all 8 checks ok); stubbed gh/security on PATH | PASS | The test covers: notes = CHANGELOG section, missing file or section refused before the build, `--notes` overrides. My stubbed dry run stopped at the version/keychain checks (no publish, no real Keychain read). |
| 3 | README and TESTERS link the changelog and match | `scripts/check-doc-links.sh` (50 links ok); manual read | PASS | Requirements (Apple Silicon, macOS 13+) and limits (not notarized, offline unchecked, #226, 5 min, Mac only) agree in all three. The anchor to TESTERS section 2 matches its heading. |
| 4 | Gate | hosted CI | PASS | Quality gate (Linux) and Platform crate (macOS) both pass at 8815d78. |

Note: the "missing section" and `--notes` paths were exercised by the repo's own test, not by hand, because release.sh checks the version and the Keychain certificate before it reaches the notes step.
