# M1.5 criterion 2: --press then --release runs the stand-in pipeline
# through every state, and the log never holds the transcript. Since M2.2
# the first launch records a fixture WAV (--debug-audio-file) instead of the
# microphone, so no microphone permission is needed (M2.2 criterion 1 end
# to end).

e2e_press_release() {
  fresh_app_state

  # The first launch carries --press itself; the release comes from a
  # second launch a second later (longer than the minimum hold).
  launch --debug-audio-file "$VM_TONE" --press
  expect "the app records the fixture, not the microphone" \
    wait_for_log 'test hook: recording a file, not the microphone'
  expect "the first launch's --press is delivered" \
    wait_for_log 'test hook event=Pressed from="this launch" delivered=true'
  sleep 1
  launch --release
  expect "the pipeline gets back to Idle" wait_for_log 'state changed phase=Idle'

  local phases
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "the full state sequence is logged in order" \
    test "$phases" = "Recording Transcribing Inserting Idle"
  # FIXED_TRANSCRIPT in crates/app/src/stand_ins.rs.
  expect "the transcript is not in the log" \
    sh -c "! printf '%s' \"\$1\" | grep -q 'This is a test dictation'" _ "$(app_log)"
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_press_release
}
