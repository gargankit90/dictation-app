# K1 test report, run 2

PR: #231 at 9f6d636 (head 3ce04c0 adds only the Reviewer's round-2 note)
Result: PASS

Re-check of round 2: hotkey pause while recording (#232), one writer for the key, modifier tracking via event flags, and the Developer's `e2e_hotkey_recorder` scenario.

| # | What | Test(s) | Result | Notes |
| - | ---- | ------- | ------ | ----- |
| 1 | Pause while recording (#232) | Rust `hotkey_settings.rs` and `hotkey_setting.rs` unit tests; `e2e_hotkey_recorder` (Right Option held while recording: no phase logged, nothing pasted) | PASS | Mutation: `paused()` always false makes `a_paused_hotkey_dictates_nothing_and_comes_back_on_resume` and `a_key_held_through_the_end_of_a_pause_does_not_dictate_when_released` fail (plus `criterion_1_the_step_log_and_error_logs_hold_no_transcript_or_key`, which passes unmutated, so it depends on those). Restored. |
| 2 | Record a combination in the real app | extra scenario below | PASS | `Ctrl+Shift+A` posted as real key events: keycap `⌃⇧A`, app logs `hotkey applied hotkey=Ctrl+Shift+A`, settings.json has it. `Ctrl+Shift+Space` can't be tested in the VM: macOS takes Ctrl+Space (input-source switch) before the page sees it, so only the two modifiers were recorded (`LeftCtrl+LeftShift`); vitest covers Space. |
| 3 | Dictate with the combination | extra scenario | PASS with a note | Holds after the first paste `hello world` into TextEdit. The FIRST hold after the app starts or after recording runs the pipeline but pastes nothing. It reproduces with the key set in settings.json (no recorder involved), so it is not K1: issue #239 (tech-debt). |
| 4 | Close Settings mid-recording | extra scenario | PASS | Record pressed, window closed, a hold of the old key (`Ctrl+Shift+A`) right after dictates; reopened Settings shows `⌃⇧A`, not recording; settings.json unchanged. |
| 5 | Existing VM scenarios | `e2e_hotkey_recorder`, `e2e_settings_save_reopen`, `e2e_onboarding_first_run` | PASS | 3 passed, 0 failed, on `dictation-vm` (`dictation-vm-2` was held by another run). |

## Gate
- `npx vitest run`: 10 files, 187 tests pass (was 175). `cargo test -p dictation-pipeline`: pass without the mutation.
- Hosted CI on the PR: `Quality gate (Linux)`, `Platform crate (macOS)` and `DMG (macOS)` all pass.

## Untested behaviour found
- First hold of a combination hotkey pastes nothing (above, #239).
- The extra scenario is not committed (this run's commit is the report only); source below. The `combo.js` fixture posts Ctrl+Shift+A with the modifier device bits, like `post-key.js`.

## Failures
None attributable to K1.

## Appendix A: `combo.js` (VM fixture)

```js
// Posts Ctrl+Shift+A as a person holds it: `osascript -l JavaScript combo.js <hold seconds>`.
ObjC.import("ApplicationServices");

function post(code, down, flags, type) {
  const event = $.CGEventCreateKeyboardEvent(null, code, down);
  if (type !== undefined) $.CGEventSetType(event, type);
  $.CGEventSetFlags(event, flags);
  $.CGEventPost($.kCGHIDEventTap, event);
}

function run(argv) {
  const hold = Number(argv[0]);
  if (!(hold >= 0)) throw new Error("usage: combo.js <hold seconds>");
  if (!$.AXIsProcessTrusted()) throw new Error("untrusted");
  const CTRL = 0x40000 | 0x1;
  const SHIFT = 0x20000 | 0x2;
  let ctrl = false;
  let shift = false;
  try {
    post(59, true, CTRL, $.kCGEventFlagsChanged);
    ctrl = true;
    delay(0.05);
    post(56, true, CTRL | SHIFT, $.kCGEventFlagsChanged);
    shift = true;
    delay(0.05);
    post(0, true, CTRL | SHIFT);
    delay(hold);
    post(0, false, CTRL | SHIFT);
    delay(0.05);
  } finally {
    if (shift) post(56, false, ctrl ? CTRL : 0, $.kCGEventFlagsChanged);
    if (ctrl) post(59, false, 0, $.kCGEventFlagsChanged);
  }
  return "posted ctrl+shift+a";
}
```

## Appendix B: `e2e_k1_extra` (run in the same run.sh as `e2e_hotkey_recorder`, which it reuses helpers from)

```sh
# K1 run 2 extras: record a COMBINATION (Ctrl+Shift+A) in the real app,
# dictate with it, and close Settings mid-recording: the key still works.
# Uses fixtures from e2e_hotkey_recorder.sh (sourced in the same run).

COMBO_VM="/Users/$VM_USER/e2e-run/combo.js"

x_combo_job() { # <hold secs> <app to front first>
  vm_terminal "open -a $(q "$2"); sleep 1; osascript -l JavaScript $(q "$COMBO_VM") $(q "$1")"
}
x_combo_dictates() {
  local before
  before=$(idle_count)
  job_says "posted ctrl+shift+a" x_combo_job 1.5 TextEdit || return 1
  wait_for_idle_count $((before + 1)) 15 || return 1
  log "OBS: TextEdit holds [$(textedit_trimmed)]; frontmost [$(vm_osascript 'tell application "System Events" to get name of first process whose frontmost is true')]; language [$(ax field-get "$APP_NAME" "$SETTINGS_WINDOW" language 2>&1)] hotkey [$(ax field-get "$APP_NAME" "$SETTINGS_WINDOW" hotkey 2>&1)]"
  [ "$(textedit_trimmed)" = "$MOCK_TEXT" ]
}

e2e_k1_extra() {
  arm_key_cleanup e2e_k1_extra right-option right-command left-option left-control left-shift
  KEY_CLEANUP_HOOK=hr_settings_reset
  fresh_app_state
  hr_settings_reset
  launch_full
  vm "mkdir -p $(q "$VM_RUN_DIR")"
  cat /tmp/combo.js | vm "cat > $(q "$COMBO_VM")"
  expect "the app starts on the default hotkey" wait_for_log 'ready .*hotkey=RightAlt'
  expect "Settings opens" hr_open
  expect "Record a different key" hr_record
  job_says "posted ctrl+shift+a" x_combo_job 0.3 "$APP_NAME"
  expect "the keycap shows ⌃⇧A" hr_text "⌃⇧A" 10
  expect "the app applied Ctrl+Shift+A" wait_for_log 'hotkey applied.*hotkey=Ctrl\+Shift\+A' 10
  expect "settings.json has Ctrl+Shift+A" hr_file_has 'Ctrl\+Shift\+A'
  textedit_fresh
  expect "a Ctrl+Shift+A hold dictates at once" x_combo_dictates

  # Close Settings in the middle of recording: the key still works.
  expect "Settings opens again" hr_open
  expect "Record a different key (again)" hr_record
  expect "close Settings mid-recording" ax_ok close "$APP_NAME" "$SETTINGS_WINDOW"
  ax_ok wait-no-window "$APP_NAME" "$SETTINGS_WINDOW" 10 || true
  textedit_fresh
  log "OBS: closed mid-recording; holding the old key right away"
  expect "the old key (Ctrl+Shift+A) dictates right after closing mid-recording" x_combo_dictates
  expect "reopened Settings is not recording and shows the old key" hr_open
  expect "keycap" hr_text "⌃⇧A" 5
  expect "settings.json still has the old key" hr_file_has 'Ctrl\+Shift\+A'

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_k1_extra
  collect_log e2e_k1_extra
  key_scenario_end
}
```
