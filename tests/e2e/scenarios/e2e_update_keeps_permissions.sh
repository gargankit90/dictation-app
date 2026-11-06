# M5.4 criteria 4 and 5: the updater, end to end in the VM, on a THROWAWAY
# key. Never the owner's key and never the public releases repository.
#
# Built on the host by scripts/updater-test.sh (into target/e2e-updater/):
# a throwaway minisign key pair, and two debug builds, 0.1.0 and 0.1.1, both
# signed `dictation-app Dev` (as releases are) and trusting only that key,
# fetching http://127.0.0.1:18741/latest.json. A second throwaway key signs
# the same 0.1.1 bundle for the bad-signature manifest. In the VM,
# fixtures/update-server.pl serves one manifest at a time on loopback.
#
# 1. 0.1.0 is installed; Microphone and Accessibility hold (a --press with
#    no WAV records with no permission error or prompt; the real hotkey is on;
#    a dictation pastes into TextEdit). The manifest says 0.1.0: the launch
#    check finds nothing and shows no dialog; the tray's "Check for
#    updates…" says "You're up to date".
# 2. The manifest offers 0.1.1 signed with the other key: "Update available";
#    "Install and restart" ends in "Couldn't install the update" with the
#    signature message; the app keeps running as 0.1.0, still installed.
# 3. The server is down: "Couldn't check for updates" (can't reach it).
# 4. The manifest offers the good 0.1.1: a relaunch's launch check offers it
#    by itself ("Later" keeps 0.1.0); Check now then installs it after
#    "Install and restart": the app restarts as 0.1.1 by itself (one copy),
#    the bundle on disk is 0.1.1 and signed `dictation-app Dev`.
# 5. After the update Microphone and Accessibility still hold, checked as in
#    step 1, with no prompt.
#
# However it ends, the server is stopped (by its own pid, checked to be this
# fixture), its folder removed and the run's regular build reinstalled, so
# later scenarios run what run.sh built.

UPD_PORT=18741
UPD_DIR=target/e2e-updater
VM_UPD_DIR="/Users/$VM_USER/e2e-update"
VM_UPD_SERVER="$VM_FIXTURES/update-server.pl"
UPD_OLD=0.1.0
UPD_NEW=0.1.1
UPD_SIGNATURE_TEXT="signature didn't match this app's update key"
UPD_SERVER_STOPPED=1

# --- Host: the throwaway builds and manifests ------------------------------------

upd_build() {
  local saved=$UPD_DIR/regular.app
  if [ "${build:-1}" = 0 ] && [ -f "$UPD_DIR/serve/good.json" ] && [ -d "$UPD_DIR/old/$APP_NAME.app" ]; then
    log "  reusing the updater builds in $UPD_DIR (--no-build)"
    return 0
  fi
  security find-identity -p codesigning 2>/dev/null | grep -q '"dictation-app Dev"' || {
    log "  no \"dictation-app Dev\" certificate on this Mac: the builds would be ad hoc and lose the permissions"
    return 1
  }
  mkdir -p "$UPD_DIR"
  # updater-test.sh builds into target/debug/bundle; keep the run's build.
  rm -rf "$saved"
  ditto "$APP_PATH" "$saved"
  log "  building $UPD_OLD and $UPD_NEW with a throwaway key (log: $OUT_DIR/updater-build.log)"
  if ! UPDATER_TEST_DIR=$UPD_DIR HOST=127.0.0.1 PORT=$UPD_PORT \
    OLD_VERSION=$UPD_OLD NEW_VERSION=$UPD_NEW scripts/updater-test.sh build >"$OUT_DIR/updater-build.log" 2>&1; then
    log "  the updater builds failed"
    rm -rf "$APP_PATH" && ditto "$saved" "$APP_PATH"
    return 1
  fi
  rm -rf "$APP_PATH" && ditto "$saved" "$APP_PATH"
  local serve=$UPD_DIR/serve bundle=$UPD_DIR/serve/Vocet_$UPD_NEW.app.tar.gz
  mv "$serve/latest.json" "$serve/good.json"
  # The same version, so the app is up to date.
  jq --arg v "$UPD_OLD" '.version = $v' "$serve/good.json" >"$serve/uptodate.json"
  # The same bundle, signed with another throwaway key.
  rm -f "$UPD_DIR/other.key" "$UPD_DIR/other.key.pub"
  npx tauri signer generate --ci -w "$UPD_DIR/other.key" -p "" >/dev/null 2>&1 || return 1
  cp "$bundle" "$UPD_DIR/other-signed.tar.gz"
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$UPD_DIR/other.key")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    npx tauri signer sign --app-version "$UPD_NEW" "$UPD_DIR/other-signed.tar.gz" >/dev/null 2>&1 || return 1
  jq --arg sig "$(cat "$UPD_DIR/other-signed.tar.gz.sig")" \
    '.platforms[].signature = $sig' "$serve/good.json" >"$serve/badsig.json"
  [ "$(jq -r '.platforms[].signature' "$serve/badsig.json")" != "$(jq -r '.platforms[].signature' "$serve/good.json")" ]
}

# --- VM: the fixture server ------------------------------------------------------

# upd_serve <manifest>: <manifest> (good, badsig, uptodate) is latest.json.
upd_serve() {
  vm "cp $(q "$VM_UPD_DIR/manifests/$1.json") $(q "$VM_UPD_DIR/www/latest.json")"
}

upd_server_start() {
  local i
  vm "rm -f $(q "$VM_UPD_DIR/pid") && nohup perl $(q "$VM_UPD_SERVER" "$VM_UPD_DIR" "$UPD_PORT") >$(q "$VM_UPD_DIR/server.log") 2>&1 </dev/null &"
  for i in $(seq 1 20); do
    if vm "curl -s -o /dev/null -w '%{http_code}' -m 2 http://127.0.0.1:$UPD_PORT/latest.json" 2>/dev/null | grep -qx 200; then
      UPD_SERVER_STOPPED=0
      log "  the update server is up on port $UPD_PORT (pid $(vm "cat $(q "$VM_UPD_DIR/pid")"))"
      return 0
    fi
    sleep 0.5
  done
  log "  the update server didn't answer: $(vm "cat $(q "$VM_UPD_DIR/server.log")" 2>&1)"
  return 1
}

# Stops this run's server: the pid it wrote, and only if that process is
# update-server.pl serving this folder. True once it is gone.
upd_server_stop() {
  vm "pid=\$(cat $(q "$VM_UPD_DIR/pid") 2>/dev/null) || exit 0
    case \$(ps -p \"\$pid\" -o command= 2>/dev/null) in
      *update-server.pl*$(q "$VM_UPD_DIR")*) kill \"\$pid\" ;;
      *) exit 0 ;;
    esac
    i=0; while kill -0 \"\$pid\" 2>/dev/null && [ \$i -lt 20 ]; do sleep 0.25; i=\$((i + 1)); done
    ! kill -0 \"\$pid\" 2>/dev/null && rm -f $(q "$VM_UPD_DIR/pid")" && UPD_SERVER_STOPPED=1
}

upd_server_requests() {
  vm "grep -c '^get path=$1 ' $(q "$VM_UPD_DIR/events") || true"
}

# --- VM: the app -----------------------------------------------------------------

# The installed bundle's version.
upd_installed_version() {
  vm "defaults read $(q "$VM_APP/Contents/Info") CFBundleShortVersionString"
}

# The update dialogs have no parent window, so macOS shows them through
# CFUserNotificationDisplayAlert: they are UserNotificationCenter's windows,
# like the microphone prompt, not the app's.
UPD_DIALOG_APP=UserNotificationCenter

# upd_dialog <text> <button>: within about 30 s a dialog shows <text>;
# presses <button> in that dialog (ax.js alert-press: the window that shows
# the text, since the alerts have no title). Saves both UI trees if not.
upd_dialog() {
  local text=$1 button=$2
  if VM_TERMINAL_TIMEOUT=45 ax_ok alert-press "$UPD_DIALOG_APP" "$text" "$button" 30; then
    return 0
  fi
  log "  no dialog with [$text] and a [$button] button"
  {
    echo "--- $UPD_DIALOG_APP"
    ax dump "$UPD_DIALOG_APP" 2>&1
  } >"$OUT_DIR/e2e_update_keeps_permissions-dialog-$(date +%H%M%S).ax.txt" || true
  dump_ui "e2e_update_keeps_permissions-$(date +%H%M%S)"
  return 1
}

# How many of UserNotificationCenter's dialogs show <text>: 0 when it isn't
# running (no dialog has been shown since the VM started). Any other failure
# prints what ax said, which is not a number, so a check on it fails.
upd_dialog_count() {
  local said status=0
  said=$(ax alert-count "$UPD_DIALOG_APP" "$1" 2>&1) || status=$?
  if [ "$status" = 0 ]; then
    printf '%s' "$said"
  elif grep -q "no running app named $UPD_DIALOG_APP" <<<"$said"; then
    printf 0
  else
    printf 'ax failed: %s' "$said"
  fi
}

# No update dialog came up within <secs>.
upd_no_dialog() {
  local secs=$1 count
  sleep "$secs"
  count=$(upd_dialog_count "Vocet")
  log "  dialogs naming Vocet: $count"
  [ "$count" = 0 ]
}

# Dismisses any update dialog left on screen (they outlive the app: they are
# UserNotificationCenter's). True once none is left.
upd_dismiss_dialogs() {
  local text said
  for text in "Vocet" "update"; do
    [ "$(upd_dialog_count "$text")" != 0 ] || continue
    said=$(ax alert-dismiss "$UPD_DIALOG_APP" "$text" Later OK 2>&1) || true
    log "  $UPD_DIALOG_APP: $said"
  done
  [ "$(upd_dialog_count "Vocet")" = 0 ] && [ "$(upd_dialog_count "update")" = 0 ]
}

# Presses a Settings section's radio button. The window is up before its page
# has drawn the tabs, so the press is tried again for a few seconds.
upd_section() {
  local i
  for i in 1 2 3 4 5 6 7 8 9 10; do
    ax_ok radio-press "$APP_NAME" Settings "$1" && return 0
    sleep 1
  done
  return 1
}

# U12: the check moved from the tray to Settings, General.
upd_tray_check() {
  ax_ok tray-click "$APP_NAME" Settings &&
    ax_ok wait-window "$APP_NAME" Settings 10 &&
    upd_section General &&
    ax_ok wait-text "$APP_NAME" Settings "Check for updates" 10 &&
    ax_ok press "$APP_NAME" Settings "Check now"
}

# The same check in a build from before U12 (the pre-rename dictation-app),
# which has no Settings sections: it is "Check for updates…" in the tray.
upd_tray_check_legacy() {
  ax_ok tray-click "$APP_NAME" "Check for updates…"
}

# --- Cleanup -----------------------------------------------------------------------

upd_cleanup() {
  local ok=0
  fresh_app_state || ok=1
  if ! upd_dismiss_dialogs; then
    log "  cleanup: an update dialog is still on screen"
    ok=1
  fi
  if [ "$UPD_SERVER_STOPPED" = 0 ] && ! upd_server_stop; then
    log "  cleanup: the update server is still running"
    ok=1
  fi
  vm "rm -rf $(q "$VM_UPD_DIR")" || ok=1
  textedit_close
  close_terminal || true
  # The run's own build again, for the scenarios after this one.
  install_app "$APP_PATH" || ok=1
  return "$ok"
}

upd_cleanup_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  [ "$status" = 0 ] || collect_log e2e_update_keeps_permissions
  upd_cleanup || {
    log "  FAIL: the cleanup after e2e_update_keeps_permissions failed"
    [ "$status" != 0 ] || status=1
  }
  exit "$status"
}

# --- The scenario ------------------------------------------------------------------

e2e_update_keeps_permissions() {
  local pid
  trap upd_cleanup_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM HUP

  upd_build || die "could not build the throwaway update"
  [ "$(jq -r .version "$UPD_DIR/serve/good.json")" = "$UPD_NEW" ] || die "the good manifest isn't $UPD_NEW"
  expect "the bad manifest's signature isn't the good one's" \
    test "$(jq -r '.platforms[].signature' "$UPD_DIR/serve/badsig.json")" != "$(jq -r '.platforms[].signature' "$UPD_DIR/serve/good.json")"
  expect "the old build is $UPD_OLD" \
    test "$(plutil -extract CFBundleShortVersionString raw "$UPD_DIR/old/$APP_NAME.app/Contents/Info.plist")" = "$UPD_OLD"

  # The server's files in the VM: the bundle and the three manifests.
  vm "rm -rf $(q "$VM_UPD_DIR") && mkdir -p $(q "$VM_UPD_DIR/www" "$VM_UPD_DIR/manifests")"
  tar -C "$UPD_DIR/serve" -cf - "Vocet_$UPD_NEW.app.tar.gz" |
    VM_CMD_TIMEOUT=300 vm "tar -C $(q "$VM_UPD_DIR/www") -xf -"
  tar -C "$UPD_DIR/serve" -cf - good.json badsig.json uptodate.json |
    vm "tar -C $(q "$VM_UPD_DIR/manifests") -xf -"

  fresh_app_state
  expect "no update dialog is left from before" upd_dismiss_dialogs
  install_app "$UPD_DIR/old/$APP_NAME.app"
  expect "0.1.0 is installed" test "$(upd_installed_version)" = "$UPD_OLD"

  # 1. Permissions before; up to date.
  expect "0.1.0 has Microphone and Accessibility" permissions_hold "$UPD_OLD"
  upd_serve uptodate
  expect "the update server starts" upd_server_start
  fresh_app_state
  clear_app_log
  launch_full "$VM_TONE" --debug-update-check
  expect "0.1.0 starts" wait_for_log "INFO .*starting version=\"$UPD_OLD\""
  expect "it checks for updates at launch" wait_for_log 'checking for updates at launch'
  expect "and finds none" wait_for_log "up to date current=\"$UPD_OLD\""
  expect "the launch check shows no dialog when up to date" upd_no_dialog 3
  expect "Settings, General has Check now" upd_tray_check
  expect "Check now says You're up to date" upd_dialog "You're up to date" OK

  # 2. A bad signature.
  upd_serve badsig
  pid=$(app_pid)
  expect "Check now finds the bad 0.1.1" upd_tray_check
  expect "it offers it" upd_dialog "$UPD_NEW is available; you have $UPD_OLD" "Install and restart"
  expect "the install is refused for its signature" upd_dialog "$UPD_SIGNATURE_TEXT" OK
  expect "the bundle was downloaded (and then refused)" \
    test "$(upd_server_requests "/Vocet_$UPD_NEW.app.tar.gz")" -ge 1
  expect "the app keeps running, the same process" test "$(app_pid)" = "$pid"
  expect "0.1.0 is still installed" test "$(upd_installed_version)" = "$UPD_OLD"
  expect "the log says the update failed" wait_for_log 'WARN .*update failed'

  # 3. The server is down.
  expect "the update server stops" upd_server_stop
  expect "Check now runs" upd_tray_check
  expect "it says it couldn't check" upd_dialog "Couldn't reach the update server" OK
  expect "the app keeps running" test "$(app_pid)" = "$pid"

  # 4. The good update: offered at launch, installed from the tray.
  upd_serve good
  expect "the update server starts again" upd_server_start
  expect "Quit from the tray ends the app by itself" quit_from_tray
  clear_app_log
  launch_full "$VM_TONE" --debug-update-check
  expect "0.1.0 starts again" wait_for_log "INFO .*starting version=\"$UPD_OLD\""
  expect "the launch check offers $UPD_NEW by itself" \
    upd_dialog "$UPD_NEW is available; you have $UPD_OLD" Later
  expect "Later keeps 0.1.0" wait_for_log 'update postponed'
  expect "still 0.1.0 on disk" test "$(upd_installed_version)" = "$UPD_OLD"
  expect "Check now offers it again" upd_tray_check
  expect "Install and restart" upd_dialog "$UPD_NEW is available; you have $UPD_OLD" "Install and restart"
  expect "the app restarts as $UPD_NEW by itself" wait_for_log "INFO .*starting version=\"$UPD_NEW\""
  expect "it logged the install before the restart" wait_for_log "update installed; restarting"
  sleep 2
  expect "one copy runs" test "$(app_count)" = 1
  expect "$UPD_NEW is installed" test "$(upd_installed_version)" = "$UPD_NEW"
  expect "the updated bundle's signature verifies" vm "codesign --verify --deep --strict $(q "$VM_APP")"
  expect "it is signed dictation-app Dev" \
    sh -c 'grep -qx "Authority=dictation-app Dev" <<EOF
$1
EOF' _ "$(vm "codesign -dvv $(q "$VM_APP") 2>&1")"
  expect "no quarantine flag came with the update" e2e_not_quarantined "$VM_APP"

  # 5. Permissions after.
  expect "$UPD_NEW still has Microphone and Accessibility" permissions_hold "$UPD_NEW"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_update_keeps_permissions
  collect_log e2e_update_keeps_permissions
  trap - EXIT INT TERM HUP
  if ! upd_cleanup; then
    log "  FAIL: the cleanup after e2e_update_keeps_permissions failed"
    SCENARIO_FAILED=1
  fi
}
