# T2 criteria 1 and 7 (P1-03, P2-01): Settings in the real app. Settings… in
# the tray opens the window; the language and the hotkey are typed in as a
# user would (fixtures/ax.js field-set: focus, select all, delete, keys),
# Save shows "Settings saved."; closing the window hides it, and reopening
# it from the tray shows the saved values, which are also in the settings
# file. The new hotkey (Left Option instead of the default Right Option)
# then starts a dictation right away, with no restart, and Right Option no
# longer does; after Quit and a relaunch the saved hotkey still works.
#
# The hotkey is typed in lowercase ("leftalt"): the app stores the canonical
# form, LeftAlt, and the form shows it after the save. The app logs
# `hotkey applied … hotkey=LeftAlt` once the new hotkey listens, and its
# `ready` line names the hotkey it started with.
#
# Real keys are posted, so the key cleanup is armed (Right and Left Option
# up however it ends), and it resets the settings to the defaults (the file
# is removed) so no later scenario starts with this hotkey.

SETTINGS_FILE_VM="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/settings.json"
# Text in the settings window's title ("dictation-app Settings").
SETTINGS_WINDOW=Settings

settings_reset() {
  vm "rm -f $(q "$SETTINGS_FILE_VM")"
}

# settings_file_has <key> <string value>: the saved settings file has
# "<key>": "<value>".
settings_file_has() {
  local json
  json=$(vm "cat $(q "$SETTINGS_FILE_VM")") || {
    log "  no settings file at $SETTINGS_FILE_VM"
    return 1
  }
  if printf '%s' "$json" | grep -Eq "\"$1\"[[:space:]]*:[[:space:]]*\"$2\""; then
    return 0
  fi
  log "  the settings file has no \"$1\": \"$2\": $(printf '%s' "$json" | tr -d '\n')"
  return 1
}

# Opens the settings window from the tray and waits for it.
open_settings() {
  ax_ok tray-click "$APP_NAME" Settings && ax_ok wait-window "$APP_NAME" "$SETTINGS_WINDOW" 10
}

# Closes it with its close button; it hides, the app keeps running.
close_settings() {
  ax_ok close "$APP_NAME" "$SETTINGS_WINDOW" && ax_ok wait-no-window "$APP_NAME" "$SETTINGS_WINDOW" 10
}

# field_shows <field> <value>: the settings window's field holds <value>.
field_shows() {
  job_says "$2" ax field-get "$APP_NAME" "$SETTINGS_WINDOW" "$1"
}

# A hold of <key> (post-key.js name) gives one dictation, pasted into TextEdit
# after what was there.
hold_dictates() {
  local key=$1 want=$2 before
  before=$(idle_count)
  job_says "posted down
posted up" post_key_hold "$key" 1.5 || return 1
  wait_for_idle_count $((before + 1)) 15 || {
    log "  no dictation after the $key hold"
    return 1
  }
  [ "$(textedit_trimmed)" = "$want" ] || {
    log "  TextEdit holds [$(textedit_trimmed)], not [$want]"
    return 1
  }
}

# A hold of Right Option starts nothing (watched for 3 s after the release).
right_option_starts_nothing() {
  local lines
  lines=$(logged_phases | wc -l | tr -d ' ')
  job_says "posted down
posted up" real_key_hold 1.5 || return 1
  sleep 3
  if [ "$(logged_phases | wc -l | tr -d ' ')" != "$lines" ]; then
    log "  Right Option started something: $(logged_phases | paste -sd ' ' -)"
    return 1
  fi
}

e2e_settings_save_reopen() {
  arm_key_cleanup e2e_settings_save_reopen left-option
  KEY_CLEANUP_HOOK=settings_reset
  fresh_app_state
  settings_reset
  launch_full
  expect "the app starts on the default hotkey" wait_for_log 'ready .*hotkey=RightAlt'

  # Change and save.
  expect "Settings… in the tray opens the window" open_settings
  expect "it shows the default hotkey" field_shows hotkey RightAlt
  expect "and no language" field_shows language ""
  expect "type de as the language" ax_ok field-set "$APP_NAME" "$SETTINGS_WINDOW" language de
  expect "type leftalt as the hotkey" ax_ok field-set "$APP_NAME" "$SETTINGS_WINDOW" hotkey leftalt
  expect "press Save" ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" Save
  expect "the window says Settings saved." \
    ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Settings saved." 15
  expect "the app applied the new hotkey" wait_for_log 'hotkey applied.*hotkey=LeftAlt' 10
  expect "the form shows it in canonical form" field_shows hotkey LeftAlt
  expect "the settings file has the language" settings_file_has language de
  expect "and the hotkey" settings_file_has hotkey LeftAlt

  # Close (it hides) and reopen from the tray.
  expect "closing the window hides it" close_settings
  expect "the app keeps running" app_count_is 1
  expect "Settings… opens it again" open_settings
  expect "it shows the saved language" field_shows language de
  expect "and the saved hotkey" field_shows hotkey LeftAlt
  expect "closing it again hides it" close_settings

  # The new hotkey works at once; the old one doesn't.
  textedit_fresh
  expect "a Left Option hold dictates right away, no restart" hold_dictates left-option "$MOCK_TEXT"
  expect "a Right Option hold starts nothing now" right_option_starts_nothing
  expect "TextEdit still has the one dictation" test "$(textedit_trimmed)" = "$MOCK_TEXT"
  collect_log e2e_settings_save_reopen-before-relaunch

  # Quit and relaunch: the saved hotkey is the one listening.
  expect "Quit from the tray ends the app by itself" quit_from_tray
  clear_app_log
  launch_full
  expect "the relaunched app starts on the saved hotkey" wait_for_log 'ready .*hotkey=LeftAlt'
  textedit_fresh
  expect "a Left Option hold dictates after the relaunch" hold_dictates left-option "$MOCK_TEXT"
  expect "and Right Option still starts nothing" right_option_starts_nothing

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_settings_save_reopen
  collect_log e2e_settings_save_reopen
  key_scenario_end
}
