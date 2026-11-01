# M4.3 criterion 1: a whole dictation in the real app. The tone fixture
# stands in for the microphone, the mock transcriber answers "hello world",
# and the real paste puts it into TextEdit, read back with osascript.

e2e_mock_paste() {
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh

  expect "the dictation gets back to Idle" dictate_once 1
  local phases text
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "the full state sequence is logged in order" \
    test "$phases" = "Recording Transcribing Inserting Idle"
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the transcript is in TextEdit" test "$text" = "$MOCK_TEXT"
  expect "the transcript is not in the log" \
    sh -c "! printf '%s' \"\$1\" | grep -q \"\$2\"" _ "$(app_log)" "$MOCK_TEXT"
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_mock_paste
  textedit_close
}
