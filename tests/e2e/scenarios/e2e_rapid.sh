# M4.3 criterion 4: five quick dictations in a row. Each press waits for the
# last dictation to finish (a press while Transcribing or Inserting is
# ignored), but there's no pause beyond that. The log shows five complete
# sequences and ends in Idle. The mock answers each dictation with its own
# word (--mock-text-sequence), so TextEdit holds exactly "one" to "five"
# pasted one after another: a paste out of order, doubled, dropped or
# replayed from an earlier dictation changes the document.

e2e_rapid() {
  local cycles=5 i words=(one two three four five) want_text=""
  fresh_app_state
  launch_full "$VM_TONE" --mock-text-sequence "$(
    IFS=,
    printf '%s' "${words[*]}"
  )"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh

  for i in $(seq 1 "$cycles"); do
    expect "dictation $i gets back to Idle" dictate_once 0.4
  done

  local phases want="" text
  phases=$(logged_phases | paste -sd ' ' -)
  for i in $(seq 1 "$cycles"); do
    want="$want${want:+ }Recording Transcribing Inserting Idle"
    want_text="$want_text${words[$((i - 1))]}"
  done
  log "  phases logged: $phases"
  expect "five complete sequences, ending in Idle" test "$phases" = "$want"
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the five transcripts were pasted in order, once each" \
    test "$text" = "$want_text"
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_rapid
  textedit_close
}
