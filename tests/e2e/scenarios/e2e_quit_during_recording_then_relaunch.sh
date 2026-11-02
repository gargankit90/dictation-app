# T2 criterion 7 (P2-05): the user quits from the tray while a dictation is
# recording. The app must exit by itself (no kill) within QUIT_TIMEOUT
# seconds, paste nothing from the abandoned recording, leave no key held,
# and start again normally: a relaunch dictates and pastes. A Quit that
# doesn't end the app fails the scenario even though the app is then killed
# for the next step (quit_from_tray, lib_full.sh).

e2e_quit_during_recording_then_relaunch() {
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh

  launch --press
  expect "the press reaches the app" \
    wait_for_log 'test hook event=Pressed from="a second launch" delivered=true'
  expect "it is recording" wait_for_log 'state changed phase=Recording'

  expect "Quit in the tray menu ends the app by itself, mid-recording" quit_from_tray
  expect "the log says Quit came from the tray" wait_for_log 'quit from the tray' 1
  expect "and that the pipeline stopped" wait_for_log 'INFO [a-z_:]+: stopped( |$)' 1
  local phases
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged before Quit: $phases"
  expect "the abandoned recording was never pasted (no Inserting)" \
    sh -c "! printf '%s\n' \"\$1\" | grep -q Inserting" _ "$phases"
  expect "TextEdit got nothing" textedit_is_empty
  expect "no key is left held (Right Option up)" job_says "right option up" real_key state
  collect_log e2e_quit_during_recording_then_relaunch-before-quit

  # Start again: a normal dictation works.
  clear_app_log
  launch_full
  expect "the relaunched app logs ready" wait_for_log 'INFO .*ready'
  expect "it started once" test "$(log_count 'INFO [a-z_:]+: starting( |$)')" = 1
  textedit_fresh
  expect "a dictation after the relaunch gets back to Idle" dictate_once 1
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged after the relaunch: $phases"
  expect "one full dictation" test "$phases" = "Recording Transcribing Inserting Idle"
  local text
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the transcript is in TextEdit" test "$text" = "$MOCK_TEXT"
  expect "one app is running" app_count_is 1

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_quit_during_recording_then_relaunch
  collect_log e2e_quit_during_recording_then_relaunch
  textedit_close
  close_terminal
  fresh_app_state
}
