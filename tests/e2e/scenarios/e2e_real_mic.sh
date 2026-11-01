# M2.2: one dictation from the VM's real microphone. Needs the microphone
# granted to the app in the golden image (done in M4.3, issue #92) and an
# input device in the VM (its audio may be silent: see below); without the grant the app logs "Microphone
# permission is missing." (M2.2 criterion 6) and this fails. Skip it with
# E2E_REAL_MIC=0 on a VM that lacks either.

e2e_real_mic() {
  if [ "${E2E_REAL_MIC:-1}" = 0 ]; then
    log "  skipped: E2E_REAL_MIC=0"
    SCENARIO_SKIPPED=1
    return 0
  fi
  fresh_app_state

  launch --press
  expect "the first launch's --press is delivered" \
    wait_for_log 'test hook event=Pressed from="this launch" delivered=true'
  sleep 1
  launch --release
  expect "the pipeline gets back to Idle" wait_for_log 'state changed phase=Idle'

  local phases
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "no microphone permission error" \
    sh -c "! printf '%s' \"\$1\" | grep -q 'Microphone permission is missing'" _ "$(app_log)"
  expect "no microphone failure" \
    sh -c "! printf '%s' \"\$1\" | grep -q 'the microphone failed'" _ "$(app_log)"
  # The VM's microphone is whatever Lume feeds it (the host's, in a quiet
  # room), so it may hear only silence, which the pipeline doesn't transcribe
  # (#69, #72). Either way the capture itself worked and the pipeline is
  # back in Idle.
  if app_log | grep -q 'not transcribing.*verdict=Silent'; then
    log "  note: the VM's microphone heard silence (nothing transcribed or pasted)"
    expect "the sequence is logged in order (silent recording)" \
      test "$phases" = "Recording Transcribing Idle"
  else
    expect "the full state sequence is logged in order" \
      test "$phases" = "Recording Transcribing Inserting Idle"
  fi
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_real_mic
}
