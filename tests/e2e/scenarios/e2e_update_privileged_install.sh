# N1 criterion 1 (#206): is an Apple Events entitlement needed? The updater has a
# path for a bundle the app can't replace itself: AppleScript inside this process,
# `do shell script ... with administrator privileges` (tauri-plugin-updater). It
# runs under the hardened runtime with no automation entitlement. This takes that
# path for real: 0.1.0 in a root-owned folder (only in this VM's clone), Check now,
# Install and restart, macOS's password prompt answered over VNC, and the app
# restarts as 0.1.1.
#
# However it ends, the folder and server are removed and the regular build is
# reinstalled.

PI_DIR=/Library/VocetPrivilegedTest
PI_VM_HOME_APP=""

pi_sudo() {
  printf '%s\n' "${VM_PASSWORD:-lume}" | vm "sudo -S -p '' sh -c $(q "$1")"
}

pi_cleanup() {
  local ok=0
  fresh_app_state || ok=1
  upd_dismiss_dialogs || ok=1
  if [ "$UPD_SERVER_STOPPED" = 0 ] && ! upd_server_stop; then ok=1; fi
  vm "rm -rf $(q "$VM_UPD_DIR")" || ok=1
  pi_sudo "rm -rf $PI_DIR" || ok=1
  close_terminal || true
  VM_APP=$PI_VM_HOME_APP
  install_app "$APP_PATH" || ok=1
  return "$ok"
}

pi_cleanup_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  [ "$status" = 0 ] || collect_log e2e_update_privileged_install
  pi_cleanup || {
    log "  FAIL: the cleanup after e2e_update_privileged_install failed"
    [ "$status" != 0 ] || status=1
  }
  exit "$status"
}

e2e_update_privileged_install() {
  local said i
  PI_VM_HOME_APP=$VM_APP
  trap pi_cleanup_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM HUP

  upd_build || die "could not build the throwaway update"
  vm "rm -rf $(q "$VM_UPD_DIR") && mkdir -p $(q "$VM_UPD_DIR/www" "$VM_UPD_DIR/manifests")"
  tar -C "$UPD_DIR/serve" -cf - "Vocet_$UPD_NEW.app.tar.gz" |
    VM_CMD_TIMEOUT=300 vm "tar -C $(q "$VM_UPD_DIR/www") -xf -"
  tar -C "$UPD_DIR/serve" -cf - good.json | vm "tar -C $(q "$VM_UPD_DIR/manifests") -xf -"

  fresh_app_state
  expect "no update dialog is left from before" upd_dismiss_dialogs
  # The old build, where this user can't replace it.
  tar -C "$UPD_DIR/old" -cf - "$APP_NAME.app" | VM_CMD_TIMEOUT=300 vm "rm -rf ~/e2e-pi && mkdir ~/e2e-pi && tar -C ~/e2e-pi -xf -"
  pi_sudo "rm -rf $PI_DIR && mkdir $PI_DIR && mv /Users/$VM_USER/e2e-pi/$APP_NAME.app $PI_DIR/ && chown -R root:wheel $PI_DIR && chmod 755 $PI_DIR && rm -rf /Users/$VM_USER/e2e-pi"
  vm "rm -rf $(q "$PI_VM_HOME_APP")" || true
  VM_APP=$PI_DIR/$APP_NAME.app
  expect "this user can't write to the folder" vm "! test -w $(q "$PI_DIR")"
  expect "its signature is intact" vm "codesign --verify --deep --strict $(q "$VM_APP")"

  upd_serve good
  expect "the update server starts" upd_server_start
  clear_app_log
  vm "open -n $(q "$VM_APP") --args --transcriber mock --debug-update-check"
  expect "0.1.0 starts" wait_for_log "INFO .*starting version=\"$UPD_OLD\""
  expect "the launch check offers $UPD_NEW" upd_dialog "$UPD_NEW is available; you have $UPD_OLD" "Install and restart"

  # macOS's password prompt: its own process, in front, taking the password over VNC.
  for i in $(seq 1 20); do
    vm "pgrep -x SecurityAgent" >/dev/null 2>&1 && break
    sleep 1
  done
  expect "macOS asks for an administrator's password" vm "pgrep -x SecurityAgent"
  sleep 2
  screenshot e2e_update_privileged_install-prompt
  vm_osascript 'tell application "System Events" to set frontmost of process "SecurityAgent" to true' || true
  sleep 1
  said=$(python3 -I "$E2E_DIR/vnc_keys.py" "$(vnc_url)" "${VM_PASSWORD:-lume}" RETURN 2>&1) || log "  VNC: $said"
  expect "the app restarts as $UPD_NEW by itself" wait_for_log "INFO .*starting version=\"$UPD_NEW\"" 40
  expect "$UPD_NEW is installed in the root-owned folder" test "$(upd_installed_version)" = "$UPD_NEW"
  expect "the updated bundle's signature verifies" vm "codesign --verify --deep --strict $(q "$VM_APP")"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_update_privileged_install
  collect_log e2e_update_privileged_install
  trap - EXIT INT TERM HUP
  if ! pi_cleanup; then
    log "  FAIL: the cleanup after e2e_update_privileged_install failed"
    SCENARIO_FAILED=1
  fi
}
