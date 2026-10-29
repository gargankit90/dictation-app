# M-W.3: Windows installer and e2e

## Goal

Something Windows testers can install, tested end to end.

## Scope

- NSIS installer from CI.
- Windows e2e in a Windows 11 ARM VM: dictate into Notepad, read back with PowerShell UI Automation.

## Out of scope

- Code-signing certificate for Windows.

## Acceptance criteria

1. `[auto]` CI produces the NSIS installer.
2. `[auto]` Windows e2e: mock dictation appears in Notepad; clipboard is restored.
3. `[manual]` One check on real x64 hardware or a cloud Windows VM before inviting Windows testers.

Every milestone also needs: the gate green, coverage at or above thresholds, `Tester: PASS` and `Reviewer: APPROVED` on the PR.
