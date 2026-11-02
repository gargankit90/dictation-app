# Issue #90, M1.6 B1: Right Option is already down when the app starts. The
# app must not start recording for a key it never saw go down, nor do
# anything when that key comes up; a normal hold afterwards works. The
# events are posted by fixtures/right-option.js run in Terminal.app. However
# it ends (even if the launch fails with the key down), the key is released
# and the app quit (arm_key_cleanup, lib_full.sh).

e2e_held_at_launch() {
  # Before the first key event, so there is no way to leave the key down.
  arm_key_cleanup e2e_held_at_launch
  fresh_app_state
  expect "Right Option goes down" job_says "posted down" real_key down
  inject_failure after_key_down
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  expect "the real hotkey is on (Accessibility granted to the app)" hotkey_is_on
  textedit_fresh

  sleep 5
  expect "no recording starts while the key is held from before launch" \
    test -z "$(logged_phases)"
  expect "Right Option goes up" job_says "posted up" real_key up
  sleep 2
  expect "releasing it starts or stops nothing" test -z "$(logged_phases)"

  local before
  before=$(idle_count)
  expect "a normal hold goes down and up" job_says "posted down
posted up" real_key_hold 1.5
  expect "the dictation gets back to Idle" wait_for_idle_count $((before + 1))
  local phases
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "exactly one dictation, in order" \
    test "$phases" = "Recording Transcribing Inserting Idle"
  expect "the transcript is in TextEdit" test "$(textedit_trimmed)" = "$MOCK_TEXT"

  collect_log e2e_held_at_launch
  key_scenario_end
}
