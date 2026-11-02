Reviewer: CHANGES REQUESTED

PR: #141 at `51e8d87678a3926d005127c52e37684fc843c28f`.
Scope: the merge of main after round-1 approval, especially the combined
`save_with_login` transaction and replacement ACL test. No merge performed.

## Findings

| # | Severity | File:line | Finding | Suggested fix |
| --- | --- | --- | --- | --- |
| R1 | Major | `src-tauri/src/commands.rs:130` (callee `crates/app/src/hotkey_setting.rs:415`) | Failed hotkey rollback is hidden from the user. Start with RightAlt saved/listening; switch successfully to F6; fail the settings-file replacement; then fail starting RightAlt during `change.undo`. `switch` keeps F6 listening on this failure, but `undo` only logs it and returns `()`. The command returns only the file error, and `get_settings` reads RightAlt from the unchanged file. Reopening Settings therefore shows RightAlt although F6 is the working hotkey, with no notice explaining that discrepancy or recovery. The merged transaction inherits this T2 limitation; this is the explicitly requested undo-failure path, not a claim that the conflict introduced the helper's defect. | Return an observable rollback result and retain the original file error as the primary error while appending an accurate warning naming the hotkey still active and recovery instructions. Make the settings readback/display account for the actual active hotkey when it differs from the file, or retain the previous source so rollback does not require a fallible new registration. Add a deterministic test that lets the forward switch succeed, fails the file write, then fails only the reverse registration, and checks listener, persisted value, and user-visible result. |

This is a static control-flow finding, not a newly injected failing test:
the reviewer did not edit product or test files. Existing rollback tests
exercise successful hotkey restoration and refused login restoration, but
do not inject failed reverse hotkey registration.

## User-visible failure cases

- Invalid Settings: validation precedes login reads/changes and hotkey
  registration. Neither side effect occurs; the validation error is accurate.
- Login succeeds, new hotkey fails, file succeeds: the original hotkey keeps
  listening and is written back to the file; the login choice is saved.
  The message names the refused hotkey, says the old one still works and
  says the other settings were saved. This is accurate in this case.
- Hotkey succeeds, file fails, both undo operations succeed: the previous
  listener and login state return; the original file error is accurate.
- Login undo fails: restoration is attempted even after hotkey undo, the
  file error remains primary, and the OS refusal is logged. With readable
  settings, the existing UI error path rereads the OS-backed login state,
  so the checkbox reflects the actual state rather than claiming success.
  If readback also fails, the form retains the unapplied draft; it does
  not report a successful save. This preserves T3's reviewed behavior.
- Hotkey undo fails: R1. The new listener survives; the file error alone
  does not explain which hotkey works, and later file-backed display is
  misleading. A simultaneous login-undo failure does not repair this gap.

## Merge integration and ACL coverage

The all-five runtime ACL test subsumes the removed two-command test:
`get_settings` and `save_settings` still execute successfully with valid
arguments on main. The remaining three commands reach argument validation
through the real handler, proving ACL admission without touching a real
keychain or network. Overlay receives an ACL `not allowed` error for every
command; the separate overlay test also retains valid-argument denials for
all five. T2's resolver check remains, including permission-widening,
remote-origin and missing-save-permission mutations. No coverage loss found.

The shell retains `ActiveHotkey`, `CursorReader`, updater/menu-bar builder
and bounded `UiSender` overlay wiring. Bounded key saves still use the shared
`LiveEngine::write_key` path; this merge does not bypass its single write slot.
Unchanged default-branch features and previously approved T3 code were not
re-reviewed.

## Independent validation

- `cargo test -p dictation-app --lib commands::tests:: -- --test-threads=2`:
  **28 passed**, including the combined validation/rollback cases, partial
  hotkey-save messages and bounded key-write ordering.
- `cargo test -p dictation-app --lib capability_tests:: -- --test-threads=1`:
  **7 passed**, including all-five runtime admission/denial and the resolver
  mutation checks.
- `git diff --check`: passed before the report was written.
- `cargo test -p dictation-pipeline --test hotkey_settings`: **17 passed**,
  including failed forward registration and successful file-failure rollback.
- `npx prettier --check reviews/T3-round2.md`: passed.

Full gate and self-hosted CI success are Developer/dispatch evidence, not
independently repeated full-gate results in this limited re-review. No native
VM or owner-screen checks were performed. Prior round-1 minor issues #142
and #143 remain follow-ups; no new minor finding is raised.

## Checklist, limited to merge integration

| Section | Result |
| --- | --- |
| 1. Scope | OK: focused merge reconciliation and preserved test coverage. |
| 2. Correctness | R1: failed reverse hotkey registration leaves undisclosed state divergence. |
| 3. Security/privacy | OK: ACL coverage preserved; no new secret exposure in the reconciliation. |
| 4. Architecture | OK: existing adapter/control boundaries and dependency direction retained. |
| 5. Rust/TypeScript practice | R1: rollback failure is logged but hidden from command/UI consumers. |
| 6. Licences/reuse | OK: reconciliation adds no dependency or external code reuse. |
| 7. Tests | R1: reverse-registration failure lacks a behavioral regression test; focused existing tests pass. |
| 8. Git/PR | OK: ordinary merge commit; reviewer changes only this report; no merge authorized or performed. |
