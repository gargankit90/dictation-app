# M4.3 criterion 4: five quick dictations in a row. Each press waits for the
# last dictation to finish (a press while Transcribing or Inserting is
# ignored), but there's no pause beyond that. The log shows five complete
# sequences and ends in Idle, and TextEdit holds the transcript five times.
# The mock answers the same text every time, so the order of the pastes
# comes from the state sequence, not from the text.

e2e_rapid() {
  local cycles=5 i
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh

  for i in $(seq 1 "$cycles"); do
    expect "dictation $i gets back to Idle" dictate_once 0.4
  done

  local phases want="" text count
  phases=$(logged_phases | paste -sd ' ' -)
  for i in $(seq 1 "$cycles"); do
    want="$want${want:+ }Recording Transcribing Inserting Idle"
  done
  log "  phases logged: $phases"
  expect "five complete sequences, ending in Idle" test "$phases" = "$want"
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  count=$(printf '%s' "$text" | grep -o "$MOCK_TEXT" | wc -l | tr -d ' ')
  expect "the transcript was pasted five times" test "$count" = "$cycles"
  expect "nothing else was pasted" \
    test "$(printf '%s' "$text" | sed "s/$MOCK_TEXT//g" | tr -d '[:space:]')" = ""
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_rapid
  textedit_close
}
