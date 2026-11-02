Reviewer: APPROVED

# T2 review, round 3

PR: #137 at `16695e9996e581a3f26d6f5bb503e3a329eac1d5`.
Scope: merge of main since round-2 approval (`3e2f8b3`); conflict resolutions
and integration only. No blockers, majors or new minors. No merge performed.

## Launch focus

Removing `overlay::launch_without_focus` preserves its behavior because
`menu_bar::builder` contains the identical macOS call:
`builder.activate_ignoring_other_apps(false)`; both helpers return the builder
unchanged elsewhere. The production builder at `src-tauri/src/lib.rs:251`
actually calls the replacement. This is direct source equivalence, rather
than an assumption that Accessory policy prevents activation.

Accessory policy is a separate setting. `menu_bar::app(&mut app)` is called
after build and before run (`lib.rs:275`), so tao receives Accessory before
launch finishes, avoiding the Regular-policy Dock flash. In the locked
tao 0.37.2 source, `AppState::launched` first applies the activation policy,
then independently calls `activateIgnoringOtherApps(ignore)` using the
builder's flag. Accessory alone therefore does not replace T2's flag; the
merge correctly retains both settings. Tao also makes visible windows key
during launch; Settings remains configured hidden and the overlay remains
created hidden with its non-activating panel implementation unchanged.

A forwarded second process returns at `lib.rs:240`, before constructing the
Tauri builder/event loop. The primary's `forwarded_launch` parses and delivers
flags, without activating the app or opening Settings. There is no new focus
operation in that path. Opening Settings from the tray deliberately calls
`unminimize`, `show`, and `set_focus` (`lib.rs:534`); this is the requested
window becoming active, unchanged by this merge. The removed helper never
prevented deliberate Settings activation or controlled reopen events.

## What the VM evidence establishes

The [Developer's merged-head run](https://github.com/gargankit90/dictation-app/pull/137#issuecomment-6566027009)
reports a fresh VM, 22 passed, zero failed, and one optional real-API skip.
This reviewer did not operate the VM or the owner's screen.

`e2e_overlay_focus` can catch focus theft from the forwarded `--press` and
`--release` launches: after establishing TextEdit it samples the frontmost
application throughout both launches and the overlay lifecycle, and checks
the actual pasted text. It reportedly passed. Settings/save/reopen and
Quit/relaunch also reportedly passed, including actual native-hotkey pastes.

Those results do **not** demonstrate cold-launch focus preservation:
`e2e_overlay_focus`, `e2e_held_at_launch`, `e2e_mock_paste`, and the
Quit/relaunch and Settings scenarios establish TextEdit **after** launch.
The double activation in `textedit_fresh` can restore a key window lost
earlier. A decisive VM scenario would establish a key TextEdit document,
start frontmost/key-window observation before a cold launch, launch the app,
then dictate and check the text without reactivating TextEdit. No such
merged-head result is supplied. Existing [#138](https://github.com/gargankit90/dictation-app/issues/138)
tracks launch-focus coverage; its suggestion that the current overlay
scenario guards cold launch overstates that scenario's oracle. This is the
previously recorded coverage limitation, not evidence of a new regression:
the relevant builder flag is preserved exactly. Physical launch checks
remain with the Lead/owner.

## Integration

- `crates/app/src/hotkey_setting.rs` is byte-for-byte unchanged since the
  round-2 approval. Its one delivery mutex still protects generation checks,
  held state and sink delivery; handover precedes old-source stop/join.
- Saved-hotkey startup still runs inside setup after reading settings, and
  the resulting `HotkeyControl` is registered for Settings saves.
- `CursorReader::new(SystemTextBeforeCursor::new())` and
  `Pipeline::spawn_with_cursor` remain wired to the same pipeline with that
  configurable hotkey (`lib.rs:370`).
- `updater::plugins` still wraps the production builder and registers both
  `tauri-plugin-updater` and `tauri-plugin-dialog`; the tray retains
  `updater::check_from_tray`. Main's plugin configuration is preserved.
- Combined README fixtures/scenarios and `run.sh` retain both branches'
  scenarios, with launch-at-login last. No changed test weakens the focus
  or paste assertions discussed above.

## Verification

Source comparison included the combined merge diff and both parent views,
the removed helper, the locked tao implementation, and launch/scenario call
paths. Developer evidence: full local gate green and the VM result above;
CI remains unavailable because of GitHub billing. Reviewer-run checks:

- `cargo nextest run -p dictation-pipeline --test hotkey_settings`: 17/17
  passed, including the four fenced handover regressions and rollback cases.
- `cargo nextest run -p dictation-app updater::tests`: 22/22 passed,
  including production plugin registration and the shipped plugin set's
  provision of the dialog/updater state used by the tray action.
- Prettier check of this report and `git diff --check`: passed.

## Checklist (merge integration only)

| Section | Result |
| --- | --- |
| Scope and criteria | OK: combined T2 hotkey and main shell behavior retained |
| Correctness, races, cancellation | OK: activation flag and delivery lock preserved; explicit Settings focus unchanged |
| Security and privacy | OK: no merge-specific secret, transcript, audio, capability or network expansion |
| Architecture | OK: OS launch policy remains in platform; shell composes ports and plugins |
| Rust and TypeScript | OK: no new suppression or blocking work introduced by resolution |
| Licences and reuse | OK: no merge-specific adaptation or new dependency beyond main |
| Tests | OK for integration; cold-launch oracle limitation remains tracked in #138 |
| Git and PR | OK: review-only file change; no product/test/spec/guard edits; no merge |
