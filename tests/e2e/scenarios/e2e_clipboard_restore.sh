# M4.3 criteria 1 and 2, M1.5 criterion 2: a whole dictation in the real app,
# and the paste borrows the clipboard and gives it back. The tone fixture
# stands in for the microphone (no permission needed), the mock transcriber
# answers, and the real paste puts the text into TextEdit. The full state
# sequence is logged, never the transcript. A sentinel put on the clipboard
# with pbcopy is still there after a dictation, and the transcript isn't left
# on it. (Folded in from e2e_mock_paste and e2e_press_release, SIMP1.)

e2e_clipboard_restore() {
  local sentinel="e2e-clipboard-sentinel-$$"
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh
  set_clipboard "$sentinel"
  expect "the sentinel is on the clipboard before" test "$(clipboard)" = "$sentinel"

  expect "the app records the fixture, not the microphone" \
    wait_for_log 'test hook: recording a file, not the microphone'
  expect "the dictation gets back to Idle" dictate_once 1
  local phases
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "the full state sequence is logged in order" \
    test "$phases" = "Recording Transcribing Inserting Idle"
  expect "the transcript was pasted" test "$(textedit_trimmed)" = "$MOCK_TEXT"
  expect "the transcript is not in the log" \
    sh -c "! printf '%s' \"\$1\" | grep -q \"\$2\"" _ "$(app_log)" "$MOCK_TEXT"
  expect "the sentinel is back on the clipboard" test "$(clipboard)" = "$sentinel"
  expect "the transcript is not left on the clipboard" \
    sh -c "! printf '%s' \"\$1\" | grep -q \"\$2\"" _ "$(clipboard)" "$MOCK_TEXT"

  # A second dictation: the clipboard comes back again.
  expect "a second dictation gets back to Idle" dictate_once 1
  expect "the sentinel is back again" test "$(clipboard)" = "$sentinel"
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_clipboard_restore
  textedit_close
}
