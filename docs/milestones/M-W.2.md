# M-W.2: Windows paste and permissions

## Goal

Paste correctly on Windows.

## Scope

- `platform::insert::windows`: Ctrl+V; Ctrl+Shift+V for terminals (window-class rule).
- Permissions adapter is a no-op on Windows.

## Out of scope

- Installer.

## Acceptance criteria

1. `[auto]` Unit tests with fakes: chord choice per window class, clipboard restore rules.
2. `[auto]` CI green on Windows and macOS.

Every milestone also needs: the gate green, coverage at or above thresholds, `Tester: PASS` and `Reviewer: APPROVED` on the PR.
