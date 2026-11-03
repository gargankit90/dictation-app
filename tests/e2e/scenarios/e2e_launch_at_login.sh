# M5.3 criterion 3 and T4b (#123): launch at login, turned on and off the way
# a user does it (the Settings window's checkbox and Save), across two
# reboots of the VM.
#
# 1. The login item is off (the OS's state, read by the debug flag
#    --debug-launch-at-login-status, which runs the code behind Settings).
# 2. The app is started; in Settings (from the tray) the box is unchecked; it
#    is checked and saved ("Settings saved."); the window is closed and
#    reopened and the box shows the OS's state, checked, and the OS says
#    enabled. Quit from the tray.
# 3. macOS restarts (System Events `restart`, boot time checked): exactly one
#    copy starts by itself once the GUI session is back, logs `starting` and
#    `ready` once, with no test hook; and it can dictate: Right Option down,
#    Esc once the log shows Recording, Right Option up (real key events from
#    Terminal): `Recording Idle`, never Transcribing (so no request), nothing
#    pasted, the clipboard untouched.
# 4. In Settings the box is checked; it is unchecked and saved; reopened, it
#    shows unchecked and the OS says disabled. Quit from the tray.
# 5. macOS restarts again: the GUI session comes back (the positive control:
#    the Dock and the user's GUI domain), and for LOGIN_QUIET_SECS (at least
#    60 s, and at least three times what the first auto-start took) no copy
#    runs and no log is written. A login item that reports "disabled" but
#    stays registered fails here.
# 6. A manual launch starts one copy, ready: the build itself still works.
#
# Last in the run, since it reboots the VM twice (about five minutes). However
# it ends, the login item is turned off and the settings reset (the key
# cleanup's hook), and the keys are released.

# On the clipboard before the auto-started copy's dictation; must be there
# after it.
LOGIN_SENTINEL="t4b login item sentinel"

# login_item <on|off|status>: runs the flag in the VM's GUI session and
# prints what it said.
login_item() {
  local out=/tmp/e2e-login-item.txt
  vm "rm -f $out; open -W -n -g --stdout $out --stderr $out $(q "$VM_APP") --args --debug-launch-at-login-$1; cat $out"
}

# The cleanup hook: the login item off, so a run with --no-reset doesn't
# leave the app starting at every login, and the default settings.
login_item_cleanup() {
  local ok=0
  [ "$(login_item off 2>&1)" = "launch at login: disabled" ] || {
    log "  cleanup: could not turn the login item off"
    ok=1
  }
  settings_reset || ok=1
  return "$ok"
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

# Restarts macOS inside the VM (a clean shutdown, so what the scenario wrote,
# such as the removed log, is on disk) and waits for ssh and the GUI session
# to come back. Dies if the VM did not really reboot.
reboot_vm() {
  local before after i
  before=$(vm "sysctl -n kern.boottime")
  log "rebooting $VM"
  vm "sync"
  vm "osascript -e 'tell application \"System Events\" to restart'" || true
  # ssh answers for a while after the restart is asked for: wait for it to go.
  for i in $(seq 1 120); do
    vm true 2>/dev/null || break
    sleep 1
  done
  wait_for_vm
  after=$(vm "sysctl -n kern.boottime")
  [ "$before" != "$after" ] || die "$VM did not reboot"
}

# The user's GUI session is up: its launchd domain and the Dock.
gui_session_up() {
  vm 'launchctl print "gui/$(id -u)" >/dev/null 2>&1 && pgrep -x Dock >/dev/null'
}

# login_box_ready: waits up to 20 s for the "Launch at login" box to exist.
# Settings' window is shown before its form: it renders "Loading…" until the
# saved settings are read, and the box isn't there yet (after a reboot this
# takes longest).
login_box_ready() {
  local i
  for i in $(seq 1 20); do
    if ax check-get "$APP_NAME" "$SETTINGS_WINDOW" launch-at-login >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  log "  the Launch at login box never appeared"
  return 1
}

# login_box_shows <checked|unchecked>: Settings' "Launch at login" box.
login_box_shows() {
  login_box_ready &&
    job_says "$1" ax check-get "$APP_NAME" "$SETTINGS_WINDOW" launch-at-login
}

# login_box_save <on|off>: sets the box like a user and saves; "Settings
# saved." must show.
login_box_save() {
  local want=checked
  [ "$1" = on ] || want=unchecked
  login_box_ready &&
    job_says "$want" ax check-set "$APP_NAME" "$SETTINGS_WINDOW" launch-at-login "$1" &&
    ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" Save &&
    ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Settings saved." 45
}

# login_item_is <enabled|disabled>: what the OS says.
login_item_is() {
  local said
  said=$(login_item status)
  log "  the OS says: $said"
  [ "$said" = "launch at login: $1" ]
}

# login_setting <on|off>: Settings from the tray, the box saved <on|off>,
# the window closed and reopened showing it, closed again; then the OS's
# state matches.
login_setting() {
  local before=unchecked after=checked state=enabled
  if [ "$1" = off ]; then
    before=checked after=unchecked state=disabled
  fi
  expect "Settings opens from the tray" open_settings
  expect "the box shows the OS's state: $before" login_box_shows "$before"
  expect "the box is set $1 and saved" login_box_save "$1"
  expect "Settings closes" close_settings
  expect "and opens again" open_settings
  expect "the box shows $after" login_box_shows "$after"
  expect "Settings closes again" close_settings
  expect "the OS says $state" login_item_is "$state"
}

e2e_launch_at_login() {
  local started_after quiet i
  arm_key_cleanup e2e_launch_at_login
  KEY_CLEANUP_HOOK=login_item_cleanup
  fresh_app_state
  settings_reset

  # 1, 2. Off; on through Settings.
  expect "it starts off" login_item_is disabled
  launch
  expect "the app starts" wait_for_log 'INFO .*ready'
  login_setting on
  expect "Quit from the tray ends the app" quit_from_tray
  clear_app_log
  vm "sync"

  # 3. The first reboot: it starts by itself, once, and dictates.
  reboot_vm
  started_after=$SECONDS
  expect "the app starts by itself after the reboot" wait_for_app 180
  started_after=$((SECONDS - started_after))
  log "  it was running ${started_after}s after the GUI session was up"
  expect "it is up and ready" wait_for_log 'INFO .*ready' 60
  sleep 3
  expect "exactly one copy runs" app_count_is 1
  expect "it started once" test "$(log_count 'INFO .*starting')" = 1
  # No test flag was given: it was started by the login item, not by us.
  expect "nothing launched it from the test side" test "$(log_count 'test hook')" = 0
  expect "its hotkey is on" hotkey_is_on
  textedit_fresh
  set_clipboard "$LOGIN_SENTINEL"
  expect "Right Option down, Esc while recording, Right Option up" \
    job_says "posted down
pressed 1
posted up" hold_escape_release
  expect "it is back in Idle" wait_for_idle_count 1 10
  sleep 2
  expect "it recorded and dropped it: Recording then Idle (no request)" \
    test "$(logged_phases | paste -sd ' ' -)" = "Recording Idle"
  expect "nothing was pasted" textedit_is_empty
  expect "the clipboard is untouched" test "$(clipboard)" = "$LOGIN_SENTINEL"
  collect_log e2e_launch_at_login.first-boot

  # 4. Off through Settings.
  login_setting off
  expect "Quit from the tray ends the app" quit_from_tray
  clear_app_log
  vm "sync"

  # 5. The second reboot: it stays off.
  reboot_vm
  expect "the GUI session is back (Dock, GUI domain)" gui_session_up
  quiet=$((started_after * 3))
  [ "$quiet" -ge "${LOGIN_QUIET_SECS:-60}" ] || quiet=${LOGIN_QUIET_SECS:-60}
  log "  watching ${quiet}s for a copy that shouldn't start"
  for i in $(seq 1 $((quiet / 2))); do
    if [ "$(app_count)" != 0 ]; then
      break
    fi
    sleep 2
  done
  expect "no copy started by itself" app_count_is 0
  expect "and none logged anything" test -z "$(app_log)"
  expect "the OS still says disabled" login_item_is disabled

  # 6. A manual launch still works.
  launch
  expect "a manual launch starts the app" wait_for_log 'INFO .*ready' 30
  expect "one copy" app_count_is 1

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_launch_at_login
  collect_log e2e_launch_at_login
  key_scenario_end
}
