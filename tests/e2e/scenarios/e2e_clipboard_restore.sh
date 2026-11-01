# M4.3 criterion 2: the paste borrows the clipboard and gives it back. A
# sentinel put there with pbcopy is still there after a dictation, and the
# transcript isn't left on it.

e2e_clipboard_restore() {
  local sentinel="e2e-clipboard-sentinel-$$"
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh
  set_clipboard "$sentinel"
  expect "the sentinel is on the clipboard before" test "$(clipboard)" = "$sentinel"

  expect "the dictation gets back to Idle" dictate_once 1
  expect "the transcript was pasted" test "$(textedit_trimmed)" = "$MOCK_TEXT"
  expect "the sentinel is back on the clipboard" test "$(clipboard)" = "$sentinel"
  expect "the transcript is not left on the clipboard" \
    sh -c "! printf '%s' \"\$1\" | grep -q \"\$2\"" _ "$(clipboard)" "$MOCK_TEXT"

  # A second dictation: the clipboard comes back again.
  expect "a second dictation gets back to Idle" dictate_once 1
  expect "the sentinel is back again" test "$(clipboard)" = "$sentinel"

  collect_log e2e_clipboard_restore
  textedit_close
}
