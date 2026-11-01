# M5.3 criterion 3: launch at login works after a reboot of the VM.
#
# The app turns its login item on (the debug flag --debug-launch-at-login-on
# runs the code behind the Settings window's "Launch at login" toggle, then
# exits), the VM is powered off and started again, and the app must be
# running once the user's GUI session is back, with nobody having launched
# it. Then the item is turned off again. The flag runs through `open` so it
# is in the GUI session, which login items belong to; its output comes back
# through a file.

# login_item <on|off|status>: runs the flag in the VM's GUI session and
# prints what it said.
login_item() {
  local out=/tmp/e2e-login-item.txt
  vm "rm -f $out; open -W -n -g --stdout $out --stderr $out $(q "$VM_APP") --args --debug-launch-at-login-$1; cat $out"
}

# Turns the login item off however the scenario ends, so a run with
# --no-reset doesn't leave the app starting at every login.
login_item_cleanup() {
  login_item off >/dev/null 2>&1 || true
}

# Waits up to `timeout` seconds for exactly one running app.
wait_for_app() {
  local timeout=${1:-180} i
  for i in $(seq 1 "$timeout"); do
    if [ "$(app_count 2>/dev/null)" = 1 ]; then
      return 0
    fi
    sleep 1
  done
  return 1
}

# Powers the VM off and starts it again, with the same disk, and waits for
# ssh and the GUI session.
reboot_vm() {
  log "rebooting $VM"
  lume stop "$VM" >/dev/null 2>&1 || true
  lume run "$VM" --no-display --detach >/dev/null || die "lume run failed"
  wait_for_vm
}

e2e_launch_at_login() {
  fresh_app_state
  trap login_item_cleanup EXIT
  local status

  status=$(login_item status)
  log "  at the start: $status"
  expect "it starts off" test "$status" = "launch at login: disabled"
  expect "the app is not running" test "$(app_count)" = 0

  status=$(login_item on)
  log "  after turning it on: $status"
  expect "turning it on works" test "$status" = "launch at login: enabled"
  expect "the flag didn't leave an app running" test "$(app_count)" = 0

  reboot_vm
  expect "the app starts by itself after the reboot" wait_for_app 180
  expect "it is up and ready" wait_for_log 'INFO .*ready' 60
  expect "it started once" test "$(app_log | grep -c 'INFO .*starting')" = 1
  collect_log e2e_launch_at_login

  fresh_app_state
  status=$(login_item off)
  log "  after turning it off: $status"
  expect "turning it off works" test "$status" = "launch at login: disabled"
  trap - EXIT
}
