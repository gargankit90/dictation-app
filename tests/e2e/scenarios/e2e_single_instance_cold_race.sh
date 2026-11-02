# T2 criterion 2 (P1-01, #82): several launches at once, none waiting for
# another to be ready, as when a user double-clicks the app a few times or a
# login item and a click race. Exactly one becomes the app: one process left,
# one "starting" and one "ready" in the log, one engine, one tray icon. Every
# other launch is handed to it (the app logs each forwarded launch's ignored
# pipeline flags), none fails on its own. A later launch is still forwarded,
# and a dictation pastes.

# How many copies start at once.
COLD_RACE_COPIES=4

e2e_single_instance_cold_race() {
  local copies=$COLD_RACE_COPIES list flags info
  fresh_app_state

  # One ssh command starts them all, in the background in the VM, so they are
  # milliseconds apart. Each has a first launch's flags (the mock, the fixture
  # and the mock text): none can know it will lose.
  list=$(seq 1 "$copies" | paste -sd ' ' -)
  flags=$(q --transcriber mock --debug-audio-file "$VM_TONE" --mock-text "$MOCK_TEXT")
  log "  starting $copies copies at once"
  vm "for i in $list; do open -g -n $(q "$VM_APP") --args $flags & done; wait"

  expect "the app logs ready" wait_for_log 'INFO [a-z_:]+: ready( |$)' 30
  expect "exactly one copy is left running" app_count_is 1 20
  # The losers forward and exit; give a straggler time to show up.
  wait_for_log_count 'ignoring the debug audio and mock text flags' $((copies - 1)) 20 || true
  expect "still exactly one copy a moment later" app_count_is 1
  expect "the app was set up once (one starting)" \
    test "$(log_count 'INFO [a-z_:]+: starting( |$)')" = 1
  expect "and became ready once" test "$(log_count 'INFO [a-z_:]+: ready( |$)')" = 1
  expect "one engine was built" test "$(log_count 'INFO [a-z_:]+: engine( |$)')" = 1
  expect "every other launch was handed to it ($((copies - 1)) forwarded)" \
    test "$(log_count 'ignoring the debug audio and mock text flags')" = $((copies - 1))
  expect "no launch failed to reach it" \
    test "$(log_count 'not delivered|could not hand this launch')" = 0
  info=$(tray_info) || log "  could not read the tray: $info"
  log "  tray: $(printf '%s' "$info" | paste -sd ' ' -)"
  expect "one tray icon" test "$(tray_field count "$info")" = 1

  # A later launch with test flags still reaches the one app.
  textedit_fresh
  expect "a dictation from later launches gets back to Idle" dictate_once 1
  expect "the later --press was forwarded and delivered" \
    wait_for_log 'test hook event=Pressed from="a second launch" delivered=true'
  expect "and the --release" \
    wait_for_log 'test hook event=Released from="a second launch" delivered=true'
  local phases text
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "one full dictation" test "$phases" = "Recording Transcribing Inserting Idle"
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the transcript is in TextEdit" test "$text" = "$MOCK_TEXT"
  expect "still one app running" app_count_is 1

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_single_instance_cold_race
  collect_log e2e_single_instance_cold_race
  textedit_close
  close_terminal
  fresh_app_state
}
