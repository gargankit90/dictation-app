# M5.5b criterion 5 (#88): the real hotkey dies and comes back by itself,
# without a relaunch. A real Right Option hold dictates; then the debug
# build's --debug-hotkey-outage hook kills the hotkey's listener the way
# revoking Accessibility does (its sink gets Failed, and every new listener
# fails to start with PermissionDenied). The app shows "Hotkey unavailable"
# in the tray and keeps retrying; a hold does nothing. --debug-hotkey-recover
# lets listeners start again: the app's supervisor starts a new event tap by
# itself, the tray clears, and the next real hold dictates.
#
# Why a test hook and not tccutil: revoking Accessibility in the VM works,
# but granting it again shows a password sheet that refuses synthetic keys
# (README, "macOS notes"), so it would need the golden image changed or a
# person at the VM's display. The hook breaks the same place, the hotkey
# sources the app's factory builds, and the restart is the real one: a new
# CGEventTap through handy-keys.
#
# Uses tray_shows, tray_is_clear and TRAY_TOOLTIP from
# e2e_tray_error_clears_on_next_recording.sh (every scenario file is sourced
# first).

# The tray while the hotkey doesn't work: the status line and the tooltip,
# "Choose another key…" as the one fix (U5).
TRAY_HOTKEY_SHOWN="Right ⌥ isn't working. Choose another key."
TRAY_HOTKEY_ERROR="The dictation key isn't working. Allow Vocet in System Settings → Privacy & Security → Accessibility; it starts working again by itself."

e2e_hotkey_listener_recovers() {
  arm_key_cleanup e2e_hotkey_listener_recovers
  fresh_app_state
  launch_full
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  expect "the real hotkey is on (Accessibility granted to the app)" hotkey_is_on
  expect "the tray starts with no title" tray_is_clear
  textedit_fresh

  local before recordings
  before=$(idle_count)
  expect "a real Right Option hold works first (the job exits 0)" job_says "posted down
posted up" real_key_hold 1.5
  expect "that dictation gets back to Idle" wait_for_idle_count $((before + 1))

  # The listener dies.
  launch --debug-hotkey-outage
  expect "the outage hook reaches the app" \
    wait_for_log 'test hook outage command=Begin from="a second launch"'
  expect "the app says the hotkey isn't working" \
    wait_for_log "shown to the user error=The dictation hotkey isn.t working"
  expect "the tray says the hotkey isn't working, with its fix" \
    env_tray_fix "Choose another key…" tray_shows "! $TRAY_HOTKEY_SHOWN" "Vocet: $TRAY_HOTKEY_SHOWN" 10
  expect "the supervisor is retrying (Accessibility looks missing)" \
    wait_for_log 'PermissionMissing: the hotkey is off until Accessibility is granted; it starts by itself then' 10

  # While it is down a hold does nothing.
  recordings=$(logged_phases | grep -c '^Recording$' || true)
  expect "a hold while it is down posts its keys" job_says "posted down
posted up" real_key_hold 1.5
  sleep 2
  expect "and starts no recording" \
    test "$(logged_phases | grep -c '^Recording$' || true)" = "$recordings"

  # Access is back: nothing else is done; the app restarts the hotkey.
  launch --debug-hotkey-recover
  expect "the recover hook reaches the app" \
    wait_for_log 'test hook outage command=End from="a second launch"'
  expect "the hotkey listens again by itself, within a few seconds" \
    wait_for_log 'the hotkey listens again' 10
  expect "the app says it is restored" wait_for_log 'hotkey restored; shown to the user'
  expect "the tray is ready again" tray_is_clear 10

  textedit_fresh
  before=$(idle_count)
  expect "a real Right Option hold after it came back (the job exits 0)" job_says "posted down
posted up" real_key_hold 1.5
  expect "it dictates: back to Idle" wait_for_idle_count $((before + 1))
  local text
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the transcript is in TextEdit" test "$text" = "$MOCK_TEXT"
  expect "no synthetic key flag was involved" \
    sh -c "! printf '%s' \"\$1\" | grep -q 'test hook event'" _ "$(app_log)"
  expect "the app was never relaunched" test "$(app_log | grep -c 'INFO .*starting')" = 1
  expect "one app is running" test "$(app_count)" = 1

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_hotkey_listener_recovers
  collect_log e2e_hotkey_listener_recovers
  key_scenario_end
}
