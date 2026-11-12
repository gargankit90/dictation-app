# REL1 test report, run 1

PR: #320 at 6d05a80
Result: PASS (one minor finding)

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | CHANGELOG one [0.1.0] section | read in full | PASS | Plain text, no relative "changed" lines, empty [Unreleased]; features checked in code |
| 2 | TESTERS.md matches app, known limits | screen names grepped against src/ | PASS, 1 minor | #315, #316 present; notarized install wording matches N2 |
| 3 | README | read | PASS | No "unreleased"/"not notarized"; links release and TESTERS.md |
| 4 | Claims, test-release.sh | scripts/test-release.sh | PASS | all sections ok |
| 5 | release.sh both keys | test-release.sh + script read | PASS | see below |

## Release script
Covered: both present, each missing (names item, points to docs, no build, no
manifest), --no-telemetry (warning, values dropped, keychain not read),
--no-telemetry --publish refused before the Keychain is read. No path prints
a key value: redact() covers both telemetry values, only "found" is printed,
values are exported only inside the build subshell and cleared afterwards.
A publish needs telemetry=1, which requires both non-empty items. docs/releasing.md matches.

## Failures / findings
Minor: TESTERS.md "Quit and uninstall" says "untick **Launch at login** in
Settings and **Save**". The app's switch is "Open Vocet at login" (Settings >
General) and Settings has no Save button (changes apply at once).
No VM run: docs only, screen names checked against the source.
