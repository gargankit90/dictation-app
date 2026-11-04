# K1 criterion 6 (#170): the Settings key recorder in the real app. Record
# Right Command by pressing it (a real key posted from Terminal), see the
# keycap say "Right ⌘", the settings file say RightCmd, dictate with it at
# once, relaunch and dictate again. A refused key keeps recording and Esc
# cancels. Pressing the CURRENT key (Right Option) while recording must not
# start a dictation (#232): no phase is logged and nothing is pasted.
#
# A Terminal job that ends brings Terminal to the front, and the recorder
# cancels when its window loses focus, so each check that depends on the
# recording runs in the same job as the key press.

VM_TYPE_KEYS="$VM_FIXTURES/type-keys.js"
VM_KEYS="$VM_FIXTURES/keys.js"
SETTINGS_FILE_VM="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/settings.json"
# Text in the settings window's title ("dictation-app Settings").
SETTINGS_WINDOW=Settings

hr_settings_reset() { vm "rm -f $(q "$SETTINGS_FILE_VM")"; }
hr_file_has() { vm "cat $(q "$SETTINGS_FILE_VM")" | grep -Eq "\"hotkey\"[[:space:]]*:[[:space:]]*\"$1\""; }
hr_text() { ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "$1" "${2:-10}"; }
hr_open() { ax_ok tray-click "$APP_NAME" Settings && ax_ok wait-window "$APP_NAME" "$SETTINGS_WINDOW" 10; }
hr_record() { ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" "Record a different key" && hr_text "Press a key" 5; }

# hr_hold_dictates <key>: a hold gives one dictation, pasted into TextEdit.
hr_hold_dictates() {
  local key=$1 before
  before=$(idle_count)
  job_says "posted down
posted up" post_key_hold "$key" 1.5 || return 1
  wait_for_idle_count $((before + 1)) 15 || return 1
  [ "$(textedit_trimmed)" = "$MOCK_TEXT" ]
}

# hr_job_shows <script> <key> <pattern>: posts <key> with the app in front
# and, in the same job, dumps the settings window; it must match <pattern>.
hr_job_shows() {
  local script=$1 key=$2 pattern=$3 shown
  shown=$(vm_terminal "open -a $(q "$APP_NAME"); sleep 1; osascript -l JavaScript $(q "$script") $(q "$key"); sleep 1; osascript -l JavaScript $(q "$VM_AX") dump $(q "$APP_NAME") $(q "$SETTINGS_WINDOW")" 2>&1)
  printf '%s' "$shown" | grep -Eq "$pattern" || {
    log "  after $key the window shows: $(printf '%s' "$shown" | grep -iE 'Press a|plain|Right|Record|Cancel' | tr -s ' ' | cut -c1-110 | paste -sd '|' -)"
    return 1
  }
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

  # A plain letter is refused with a reason and recording goes on; Esc cancels.
  expect "Record a different key starts recording" hr_record
  expect "a plain letter is refused and recording goes on" \
    hr_job_shows "$VM_TYPE_KEYS" a 'Press a key'
  expect "the refusal says why" hr_text "plain" 5
  expect "Esc cancels recording" \
    hr_job_shows "$VM_KEYS" escape 'Record a different key'
  expect "Esc cancelled: keycap back to Right ⌥" hr_text "Right ⌥" 5
  expect "nothing was saved" test "$(vm "test -e $(q "$SETTINGS_FILE_VM") && echo yes || echo no")" = no

  # The CURRENT key while recording: the global hotkey must not dictate.
  # TextEdit is set up first (that blurs Settings and would end a recording).
  # Then ONE Terminal job brings Settings forward, starts recording, checks
  # that the recorder is listening, and only then holds the key, so nothing
  # takes focus between the check and the key.
  local before_phases hold
  textedit_fresh
  before_phases=$(logged_phases | wc -l | tr -d ' ')
  hold=$(vm_terminal "open -a $(q "$APP_NAME"); sleep 1; osascript -l JavaScript $(q "$VM_AX") press $(q "$APP_NAME") $(q "$SETTINGS_WINDOW") 'Record a different key'; sleep 1; osascript -l JavaScript $(q "$VM_AX") dump $(q "$APP_NAME") $(q "$SETTINGS_WINDOW") | grep -q 'Press a key' || { echo NOT-LISTENING; exit 3; }; echo LISTENING; down=0; release() { if [ \$down = 1 ]; then osascript -l JavaScript $(q "$VM_POST_KEY") right-option up; fi; }; trap release EXIT; trap 'exit 1' INT TERM HUP; down=1; osascript -l JavaScript $(q "$VM_POST_KEY") right-option down || exit; sleep 1.5 & wait \$!; osascript -l JavaScript $(q "$VM_POST_KEY") right-option up || exit; down=0; echo HELD" 2>&1) || true
  log "OBS: hold job said: $(printf '%s' "$hold" | tr '\n' ' ')"
  expect "the recorder was listening when the key went down" \
    sh -c 'printf "%s" "$1" | grep -q LISTENING && printf "%s" "$1" | grep -q HELD' _ "$hold"
  sleep 3
  expect "holding the current key while recording starts no dictation" \
    test "$(logged_phases | wc -l | tr -d ' ')" = "$before_phases"
  expect "and nothing is pasted" test -z "$(textedit_trimmed)"
  # The recorder ended with the key (it records the current key again); the
  # hotkey works as before.
  textedit_fresh
  expect "afterwards a Right Option hold dictates as usual" hr_hold_dictates right-option

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
