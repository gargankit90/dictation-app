# M4.3 criterion 3: a silent recording pastes nothing. The silence fixture
# is 1.5 s of zeros; the pipeline doesn't send it to the transcriber (#69,
# #72), so no text is pasted and the clipboard isn't touched. The dictation is
# dropped (M4.2's TranscriptDropped): straight back to Idle, no error shown.

e2e_silence() {
  local sentinel="e2e-silence-sentinel-$$"
  fresh_app_state
  launch_full "$VM_SILENCE"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh
  set_clipboard "$sentinel"

  expect "the dictation gets back to Idle" dictate_once 1
  local phases text
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "the pipeline never inserts" test "$phases" = "Recording Transcribing Idle"
  expect "the log says why nothing was transcribed" wait_for_log 'not transcribing.*Silent'
  expect "no error is shown for it" \
    sh -c "! printf '%s' \"\$1\" | grep -q 'dictation failed'" _ "$(app_log)"
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "nothing is pasted" test -z "$text"
  expect "the clipboard is untouched" test "$(clipboard)" = "$sentinel"
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_silence
  textedit_close
}
