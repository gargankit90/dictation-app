# M1.5 criterion 1: the app launches; a second launch with --press forwards
# the flag to it and doesn't leave a second app running. The first launch
# records a fixture WAV instead of the microphone (M2.2), so the press needs
# no microphone permission.

e2e_startup() {
  fresh_app_state

  launch --debug-audio-file "$VM_TONE"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  expect "one app is running" test "$(app_count)" = 1
  # Not a check: the flags work either way. Says whether the VM had granted
  # Accessibility, for the report.
  if app_log | grep -q 'PermissionMissing'; then
    log "  note: real hotkey off (Accessibility missing in this VM)"
  else
    log "  note: real hotkey on"
  fi

  launch --press
  expect "the second launch's --press reaches the running app" \
    wait_for_log 'test hook event=Pressed from="a second launch" delivered=true'
  expect "the pipeline starts recording" wait_for_log 'state changed phase=Recording'
  # The second process exits as soon as it has forwarded.
  sleep 3
  expect "still only one app is running" test "$(app_count)" = 1
  expect "the app was set up once" test "$(app_log | grep -c 'INFO .*starting')" = 1

  collect_log e2e_startup
}
