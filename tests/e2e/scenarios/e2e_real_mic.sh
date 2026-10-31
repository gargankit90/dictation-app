# M2.2: one dictation from the VM's real microphone. Skipped unless
# E2E_REAL_MIC=1, since it needs microphone access granted to the app in the
# golden image and an input device in the VM. Without the grant the app logs
# "Microphone permission is missing." (M2.2 criterion 6) and this fails.

e2e_real_mic() {
  if [ "${E2E_REAL_MIC:-0}" != 1 ]; then
    log "  skipped: set E2E_REAL_MIC=1 to record from the VM's microphone"
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
  expect "the full state sequence is logged in order" \
    test "$phases" = "Recording Transcribing Inserting Idle"
  expect "one app is running" test "$(app_count)" = 1

  collect_log e2e_real_mic
}
