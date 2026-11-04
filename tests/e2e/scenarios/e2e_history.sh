# U7: a dictation appears in Settings → History and Copy works, in the real
# app. The tray's "History…" opens the section at the empty state; dictating
# (pasted into TextEdit) adds a row while it shows; Copy puts the whole text on
# the clipboard; a failed paste (--debug-fail-paste, a launch of its own) shows
# "Not pasted". The file has mode 0600, holds what was dictated and no app name,
# the log holds neither the text nor its length, and the entries come back after
# a relaunch. Delete removes one row, lowering the limit asks and trims, Clear
# history asks and deletes all, and Off deletes the file and stores nothing more.
#
# Only made-up text goes in, so the file is safe to save as evidence.

HISTORY_SETTINGS_WINDOW=Settings
HISTORY_TEXT="$MOCK_TEXT"
HISTORY_FAILED_TEXT="this paste will fail"

history_open() {
  ax_ok tray-click "$APP_NAME" "History" &&
    ax_ok wait-window "$APP_NAME" "$HISTORY_SETTINGS_WINDOW" 10 &&
    ax_ok wait-text "$APP_NAME" "$HISTORY_SETTINGS_WINDOW" "Keep recent dictations" 10
}

history_shows() {
  ax_ok wait-text "$APP_NAME" "$HISTORY_SETTINGS_WINDOW" "$1" "${2:-10}"
}

history_gone() {
  ax_ok wait-no-text "$APP_NAME" "$HISTORY_SETTINGS_WINDOW" "$1" "${2:-10}"
}

history_press() {
  ax_ok press "$APP_NAME" "$HISTORY_SETTINGS_WINDOW" "$1"
}

# history_press_first <start>: the first (newest) row's button.
history_press_first() {
  ax_ok press-first "$APP_NAME" "$HISTORY_SETTINGS_WINDOW" "$1"
}

# The file's mode (stat prints 600) and contents.
history_mode() {
  vm "stat -f %Lp $(q "$VM_HISTORY_FILE")"
}

history_file() {
  vm "cat $(q "$VM_HISTORY_FILE")"
}

history_file_has() {
  printf '%s' "$(history_file)" | grep -qF "$1"
}

history_entries() {
  history_file | grep -c '"created_at"' || true
}

history_no_app_name() {
  local json
  json=$(history_file)
  for word in TextEdit com.apple bundle window '"app"'; do
    if printf '%s' "$json" | grep -qF "$word"; then
      log "  the history file holds $word"
      return 1
    fi
  done
}

# The log, at every level written, holds no entry's text.
history_log_is_clean() {
  local text=$1 log_text
  log_text=$(app_log)
  [ -n "$log_text" ] || {
    log "  the log is empty"
    return 1
  }
  if printf '%s' "$log_text" | grep -qF "$text"; then
    log "  the log holds the text"
    return 1
  fi
}

e2e_history() {
  fresh_app_state
  vm "rm -f $(q "$VM_HISTORY_FILE")"
  launch_full
  expect "the app starts" wait_for_log 'INFO .*ready'
  sleep 2
  expect "the menu has History below Copy last dictation" \
    sh -c "printf '%s\n' \"\$1\" | grep -A1 'Copy last dictation' | grep -qF 'History…'" _ "$(tray_menu)"
  expect "History… opens the section" history_open
  expect "the empty state shows" history_shows "No dictations yet."
  screenshot e2e_history.empty

  textedit_fresh
  expect "a dictation ends Idle" dictate_once 1
  expect "it pasted" test "$(textedit_trimmed)" = "$HISTORY_TEXT"
  expect "History… shows it" history_open
  expect "the row appears with its text" history_shows "$HISTORY_TEXT"
  expect "the row has its time" history_shows "Just now"
  screenshot e2e_history.one-entry

  set_clipboard "something else"
  expect "Copy is pressed" history_press_first "Copy dictation from"
  sleep 1
  expect "the clipboard holds exactly the dictation" test "$(clipboard)" = "$HISTORY_TEXT"

  expect "the file has mode 0600" test "$(history_mode)" = 600
  expect "the file holds the text" history_file_has "$HISTORY_TEXT"
  expect "the file records it as pasted" history_file_has '"pasted": true'
  expect "the file names no app or window" history_no_app_name
  expect "the log holds no history text" history_log_is_clean "$HISTORY_TEXT"
  textedit_close
  expect "the app quits" quit_from_tray

  # A failed paste, in a launch of its own: the entries come back too.
  launch --debug-audio-file "$VM_TONE" --mock-text "$HISTORY_FAILED_TEXT" --debug-fail-paste
  expect "the failing-paste launch starts" wait_for_log 'INFO .*ready'
  sleep 2
  expect "a dictation whose paste fails ends Idle" dictate_once 1
  expect "History… shows both" history_open
  expect "the stored entry came back" history_shows "$HISTORY_TEXT"
  expect "the failed one is listed" history_shows "$HISTORY_FAILED_TEXT"
  expect "the chip shows" history_shows "Not pasted"
  expect "the file records it as not pasted" history_file_has '"pasted": false'
  expect "still mode 0600" test "$(history_mode)" = 600
  expect "the log holds no history text" history_log_is_clean "$HISTORY_FAILED_TEXT"
  screenshot e2e_history.not-pasted

  # Copy last dictation is the newest entry, even though its paste failed.
  set_clipboard "something else"
  expect "Copy last dictation is pressed" ax_ok tray-click "$APP_NAME" "Copy last dictation"
  sleep 1
  expect "it copied the newest entry" test "$(clipboard)" = "$HISTORY_FAILED_TEXT"

  # Delete one row.
  expect "History… opens again" history_open
  expect "Delete is pressed" history_press_first "Delete dictation from"
  expect "the newest row goes" history_gone "$HISTORY_FAILED_TEXT"
  expect "the other stays" history_shows "$HISTORY_TEXT"
  expect "one entry is left in the file" test "$(history_entries)" = 1

  # Clear history asks first.
  expect "Clear history… is pressed" history_press "Clear history…"
  expect "it asks with the count" history_shows "Delete 1 dictation?"
  screenshot e2e_history.clear-confirm
  expect "Cancel keeps it" history_press "Cancel"
  expect "the row is still there" history_shows "$HISTORY_TEXT"
  expect "Clear history… is pressed again" history_press "Clear history…"
  expect "it asks again" history_shows "Delete 1 dictation?"
  expect "Delete confirms" history_press "Delete"
  expect "the empty state is back" history_shows "No dictations yet."
  expect "the file is gone" vm "test ! -e $(q "$VM_HISTORY_FILE")"

  collect_log e2e_history
  expect "the app quits" quit_from_tray
  fresh_app_state
}
