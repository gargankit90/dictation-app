# M-W.1: Windows CI

## Goal

The whole gate green on Windows.

## Scope

- `windows-latest` job in CI running the same gate.
- Fix anything that only built on macOS, without adding `cfg(target_os)` outside `platform`.

## Out of scope

- Windows paste and installer.

## Acceptance criteria

1. `[auto]` CI passes on `windows-latest`: build, clippy, all unit and integration tests.
2. `[auto]` No changes outside `platform/` were needed for the shared crates to pass, or each one is explained in the PR.

Every milestone also needs: the gate green, coverage at or above thresholds, `Tester: PASS` and `Reviewer: APPROVED` on the PR.
