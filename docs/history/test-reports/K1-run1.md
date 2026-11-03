# K1 test report, run 1

PR: #231 at d40c434
Result: PASS

| # | Criterion | Test(s) | Result | Notes |
| - | --------- | ------- | ------ | ----- |
| 1 | keycap, record, sided modifiers, alone-modifier, Esc, reset, same stored text | `hotkeyKeys.test.ts`, `HotkeyRecorder.test.tsx`, Rust `parses_what_the_recorder_writes`; VM scenario `e2e_hotkey_recorder` | PASS | Real WKWebView: Right ⌘ arrives as `MetaRight`, keycap shows `Right ⌘`, settings.json has `RightCmd`. Esc cancels and saves nothing (checked in the same Terminal job, so Terminal's focus steal can't fake it). Reset to default is covered by vitest only. |
| 2 | refusals with a plain reason; conflicts keep old key | vitest (refusal, "shows the app's message", Settings "keeps the old key"); VM: plain letter | PASS | VM: `a` is refused ("A plain A would start dictation…"), keycap still says `Press a key…`, nothing saved. |
| 3 | works at once and after relaunch | VM: Right ⌘ hold dictates right after recording (no relaunch), again after Quit and relaunch (`ready hotkey=RightCmd`); Settings still shows `Right ⌘` | PASS | |
| 4 | Onboarding uses the same recorder | `Onboarding.test.tsx`; `e2e_onboarding_first_run` | PASS | |
| 5 | no extra macOS permission | code read: page `keydown`/`keyup` listeners only; the VM has only today's grants | PASS | |
| 6 | [manual] VM | `e2e_hotkey_recorder` (new, appendix), `e2e_settings_save_reopen`, `e2e_onboarding_first_run` | PASS | Run on `dictation-vm` because `dictation-vm-2` was held by the post-merge e2e run (pid 83278). |

## Gate
- `npx vitest run`: 10 files, 175 tests pass. `cargo test -p dictation-core recorder`: the new spec test passes. CI/gate not rebuilt (as the role says).
- Mutation check 1: `MetaRight` mapped to the Left side: 3 tests fail (`HotkeyRecorder` "records a modifier pressed and released alone", `hotkeyKeys` "…with its side", "…several modifiers alone in canonical order"). Restored.
- Mutation check 2: plain-letter refusal disabled: 3 tests fail (refuses a plain letter; refuses plain text keys; starts afresh after a refusal). Restored.
- VM e2e: `e2e_settings_save_reopen` PASS, `e2e_onboarding_first_run` PASS, `e2e_hotkey_recorder` PASS (1 passed, 0 failed).

## What the user sees: pressing the CURRENT key (Right ⌥) while recording
A Right ⌥ hold (1.5 s) while Settings is recording starts a real dictation: log `Recording Transcribing Inserting Idle`, overlay shown then hidden. The paste went to the Settings window and no text appeared anywhere (language field empty, hotkey field `RightAlt`, TextEdit untouched; no field had focus). The recorder ended by itself: it recorded `RightAlt` (the current key), settings.json still `RightAlt`, Settings says "Dictation key saved.". Nothing was left recording and no text was pasted, so this is not a FAIL. It is a minor: the PR already lists it; follow-up issue #232 (tech-debt). With a text field focused the transcript would land in it (not tried).

## Untested behaviour found
- Harness note: a Terminal job that ends brings Terminal to the front, and the recorder cancels on blur by design, so checks after a key press must run in the same job as the press (the scenario does).
- Not covered by an automated test in the real app: an OS refusal to register a key (vitest only, with a mocked save error); both sides of one modifier; a combination such as Ctrl+Shift+Space (`keys.js` can't post it; vitest covers it).
- Blur keeps the old refusal message but cancels recording (seen in the VM): harmless.
- The new VM scenario is not committed (this run's commit was limited to the report). It is reproduced below; the Developer or Lead may add it to `tests/e2e/scenarios/`, `ALL_SCENARIOS` in `run.sh` and the README table.

## Failures
None.

## Appendix: `e2e_hotkey_recorder` (as run, 1 passed)

```sh
# K1 criterion 6 (#170): the Settings key recorder in the real app. Record
# Right Command by pressing it (a real key posted from Terminal), see the
# keycap say "Right ⌘", the settings file say RightCmd, dictate with it at
# once, relaunch and dictate again. Refused keys and Esc keep recording /
# cancel. Also: pressing the CURRENT key (Right Option) while recording.
# Observations are logged ("OBS:") as well as checked.

VM_TYPE_KEYS="$VM_FIXTURES/type-keys.js"
VM_KEYS="$VM_FIXTURES/keys.js"
SETTINGS_FILE_VM="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/settings.json"
SETTINGS_WINDOW=Settings

hr_settings_reset() { vm "rm -f $(q "$SETTINGS_FILE_VM")"; }
hr_file_has() { vm "cat $(q "$SETTINGS_FILE_VM")" | grep -Eq "\"hotkey\"[[:space:]]*:[[:space:]]*\"$1\""; }
hr_text() { ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "$1" "${2:-10}"; }
hr_open() { ax_ok tray-click "$APP_NAME" Settings && ax_ok wait-window "$APP_NAME" "$SETTINGS_WINDOW" 10; }
hr_record() { ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" "Record a different key" && hr_text "Press a key" 5; }
# A keys.js / type-keys.js job with the app in front first.
hr_keys() {
  local script=$1
  shift
  vm_terminal "open -a $(q "$APP_NAME"); sleep 1; osascript -l JavaScript $(q "$script") $(q "$@")"
}
hr_hold_dictates() {
  local key=$1 before
  before=$(idle_count)
  job_says "posted down
posted up" post_key_hold "$key" 1.5 || return 1
  wait_for_idle_count $((before + 1)) 15 || return 1
  [ "$(textedit_trimmed)" = "$MOCK_TEXT" ]
}

e2e_hotkey_recorder() {
  arm_key_cleanup e2e_hotkey_recorder right-option right-command left-option
  KEY_CLEANUP_HOOK=hr_settings_reset
  fresh_app_state
  hr_settings_reset
  launch_full
  expect "the app starts on the default hotkey" wait_for_log 'ready .*hotkey=RightAlt'
  expect "Settings opens" hr_open
  expect "the keycap shows Right ⌥" hr_text "Right ⌥"

  # A plain letter is refused and recording goes on; Esc cancels.
  expect "Record a different key starts recording" hr_record
  # The check runs in the same Terminal job as the key: a later job would
  # bring Terminal to the front and blur the window, which cancels recording.
  local shown
  shown=$(vm_terminal "open -a $(q "$APP_NAME"); sleep 1; osascript -l JavaScript $(q "$VM_TYPE_KEYS") a; sleep 1; osascript -l JavaScript $(q "$VM_AX") dump $(q "$APP_NAME") $(q "$SETTINGS_WINDOW")" 2>&1)
  log "OBS: right after the letter a: $(printf '%s' "$shown" | grep -iE 'Press a|plain|Right|Cancel' | tr -s ' ' | cut -c1-110 | paste -sd '|' -)"
  expect "the keycap still says Press a key… after the refusal" sh -c 'printf "%s" "$1" | grep -q "Press a key"' _ "$shown"
  expect "with the reason shown" sh -c 'printf "%s" "$1" | grep -q "plain"' _ "$shown"
  expect "a plain letter is refused with a reason" hr_text "plain" 5
  shown=$(vm_terminal "open -a $(q "$APP_NAME"); sleep 1; osascript -l JavaScript $(q "$VM_KEYS") escape; sleep 1; osascript -l JavaScript $(q "$VM_AX") dump $(q "$APP_NAME") $(q "$SETTINGS_WINDOW")" 2>&1)
  expect "Esc cancels recording (checked in the same job)" sh -c 'printf "%s" "$1" | grep -q "Record a different key" && ! printf "%s" "$1" | grep -q "Press a key"' _ "$shown"
  expect "Esc cancelled: keycap back to Right ⌥" hr_text "Right ⌥" 5
  expect "nothing was saved" test "$(vm "test -e $(q "$SETTINGS_FILE_VM") && echo yes || echo no")" = no

  # The CURRENT key while recording (the global hotkey is still live).
  local before_phases
  expect "Record again" hr_record
  textedit_fresh
  before_phases=$(logged_phases | wc -l | tr -d ' ')
  job_says "posted down
posted up" post_key_hold right-option 1.5 "$APP_NAME"
  sleep 3
  log "OBS: phases after the Right Option hold during recording: $(logged_phases | tail -n +$((before_phases + 1)) | paste -sd ' ' -)"
  log "OBS: TextEdit holds [$(textedit_trimmed)]"
  log "OBS: Settings window: $(ax windows "$APP_NAME" | tr '\n' '|')"
  log "OBS: overlay: $(ax overlay "$APP_NAME")"
  log "OBS: hotkey field [$(ax field-get "$APP_NAME" "$SETTINGS_WINDOW" hotkey 2>&1)] language field [$(ax field-get "$APP_NAME" "$SETTINGS_WINDOW" language 2>&1)]"
  log "OBS: dump: $(ax dump "$APP_NAME" "$SETTINGS_WINDOW" 2>&1 | grep -iE 'hello|Press a|Right|Record|Cancel|Reset|saved' | head -12 | tr -s ' ' | paste -sd '|' -)"
  log "OBS: settings.json: $(vm "cat $(q "$SETTINGS_FILE_VM") 2>&1" | tr -d '\n' | cut -c1-200)"
  log "OBS: log: $(app_log | grep -E 'hotkey|phase|overlay' | tail -8 | cut -c28- | paste -sd '|' -)"
  collect_log e2e_hotkey_recorder-current-key
  log "OBS: keycap/state: $(ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" 'Press a key' 2 && echo 'still shows Press a key' || echo 'not recording any more')"

  # Right Command.
  vm "pkill -x TextEdit; true" >/dev/null 2>&1
  expect "back in Settings" hr_open
  ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" Cancel || true
  expect "Record a different key" hr_record
  job_says "posted down
posted up" post_key_hold right-command 0.4 "$APP_NAME"
  expect "the keycap shows Right ⌘" hr_text "Right ⌘" 10
  expect "the app applied RightCmd" wait_for_log 'hotkey applied.*hotkey=RightCmd' 10
  expect "settings.json has RightCmd" hr_file_has RightCmd
  textedit_fresh
  expect "a Right ⌘ hold dictates at once" hr_hold_dictates right-command
  screenshot hotkey_recorder.after_record

  expect "Quit from the tray" quit_from_tray
  clear_app_log
  launch_full
  expect "relaunched on RightCmd" wait_for_log 'ready .*hotkey=RightCmd'
  textedit_fresh
  expect "a Right ⌘ hold dictates after relaunch" hr_hold_dictates right-command
  expect "Settings still shows Right ⌘" hr_open
  expect "keycap" hr_text "Right ⌘" 5

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_hotkey_recorder
  collect_log e2e_hotkey_recorder
  key_scenario_end
}
```
