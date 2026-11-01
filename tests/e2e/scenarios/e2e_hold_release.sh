# Issue #90, M1.6 criterion 4: a real Right Option hold and release, as the
# keyboard sends them (a flagsChanged event for key 61 with the right-alt
# device bit), start and stop a dictation. The events are posted by
# fixtures/right-option.js run in Terminal.app, which has the Accessibility
# grant; the synthetic --press/--release flags aren't used.

e2e_hold_release() {
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  expect "the real hotkey is on (Accessibility granted to the app)" hotkey_is_on
  textedit_fresh

  local before keys
  before=$(idle_count)
  keys=$(real_key_hold 1.5)
  expect "Right Option goes down and up" test "$keys" = "posted down
posted up"
  expect "the dictation gets back to Idle" wait_for_idle_count $((before + 1))
  local phases
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "exactly one dictation, in order" \
    test "$phases" = "Recording Transcribing Inserting Idle"
  expect "no synthetic key flag was involved" \
    sh -c "! printf '%s' \"\$1\" | grep -q 'test hook event'" _ "$(app_log)"
  expect "the transcript is in TextEdit" test "$(textedit_trimmed)" = "$MOCK_TEXT"

  collect_log e2e_hold_release
  textedit_close
  close_terminal
}
