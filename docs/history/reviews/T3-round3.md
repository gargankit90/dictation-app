Reviewer: APPROVED

PR: #141 at `aec34ce3ab57f759ab530b92b55088965ae08e77`.
Scope: only the round-2 R1 fix, `f302032..aec34ce`, and its affected callers.
No merge performed or authorized.

## Findings

R1 is resolved. No new blocker, major, or minor findings in this change.

`HotkeyChange::undo` now returns `UndoFailed` rather than hiding a failed
reverse registration. `save_with_login` preserves the original file error,
attempts both restorations, and adds each failed restoration to the error
shown to the user. `read_settings`, used by `get_settings`, replaces the
file's stale hotkey with the current control's spec.

## Reproduction and user-visible outcome

The deterministic command regression reproduces the exact R1 sequence:
RightAlt saved and listening, successful switch to F6, failed file write,
then failed RightAlt registration during rollback. It verifies that F6
listens and RightAlt does not, the file still contains RightAlt and the old
language, and Settings readback returns F6 while retaining the saved language.
The pipeline regression additionally dictates through F6 and verifies the
old key cannot dictate after the failed undo.

The message starts with the file error, then "Your settings weren't saved",
then names the failed reverse registration, its cause, and F6 as the hotkey
that works now. It tells the user that saving again keeps F6. This distinguishes
the current session from persisted settings and gives a usable recovery action.
If login restoration fails too, its warning follows the hotkey warning and
reports the OS readback (enabled, disabled, approval required, or unknown
with instructions to check System Settings). Neither warning replaces the
original file error. Successful rollback still returns only the file error.

On the existing UI error path, the F6 draft remains visible in this exact
reproduction, and reopening Settings reads F6 from `get_settings`. The UI
also rereads the login checkbox after an error. Other failed saves can
retain an unapplied hotkey draft by existing design; the readback change
does not turn the editable field into a live listener-status indicator.

## Relaunch check

The active override is intentionally in memory, not persisted by a failed
save. On a subsequent launch, `startup_spec(config.settings())` selects the
file's RightAlt again; the shell builds its new control and listener from
that same spec. If registration succeeds, RightAlt listens and Settings
shows RightAlt; F6 is no longer retained. If the old key remains unavailable,
its startup error propagates through `Pipeline::spawn` and shell setup,
rather than opening a working Settings screen claiming F6 survived restart.
Saving F6 successfully before quitting is the recovery described in the error.

This relaunch conclusion is a control-flow trace plus the passing saved-key
startup tests, not a new native relaunch experiment. The existing release
exception for missing Accessibility permission can still launch with no
listener; `spec()` is then the configured startup spec. That unchanged
permission behavior is outside R1, so this approval does not assert that
`with_active` detects every possible dead or absent listener.

## Independent validation

- `cargo test -p dictation-app --lib commands::tests:: -- --test-threads=2`:
  **31 passed**, including the exact R1 reproduction, failed login restore,
  successful rollback, and generated bindings consistency.
- `cargo test -p dictation-pipeline --test hotkey_settings`: **20 passed**,
  including failed reverse registration with real pipeline event delivery,
  saved-key startup, handoff, and missing-permission behavior.
- `cargo test -p dictation-pipeline --lib launch::tests:: -- --test-threads=2`:
  **13 passed**, including all failed-restore readback descriptions.
- `cargo test -p dictation-app --lib capability_tests:: -- --test-threads=1`:
  **7 passed**; the added managed state does not break main-window command
  admission or overlay denial.
- `git diff --check`: passed before writing this report.

No product, test, spec, or guard edits were made. Native VM checks and the
50-run stress exercise were not repeated for this focused re-review;
self-hosted CI success is dispatch evidence. Existing round-1 follow-ups
#142 and #143 remain unchanged.

## Checklist, limited to R1

| Section | Result |
| --- | --- |
| 1. Scope | OK: rollback reporting and truthful readback address R1. |
| 2. Correctness | OK: exact failure reproduced; both rollback results retained; relaunch traced above. |
| 3. Security/privacy | OK: no new network, capability grants, secrets, audio storage, or transcript logging. |
| 4. Architecture | OK: hotkey/login logic remains in app; shell composes command results. |
| 5. Rust/TypeScript practice | OK: typed failures, documented helpers, generated binding documentation consistent. |
| 6. Licences/reuse | OK: no dependencies or external code introduced. |
| 7. Tests | OK: command and pipeline regressions catch R1; affected suites pass. |
| 8. Git/PR | OK: focused fix; reviewer changes only this report; no merge. |
