# U1 round 3, the control for e2e_update_from_dictation_app: does macOS ask
# whether the updated build may read the saved API key ("wants to access key
# ... in your keychain") when NOTHING is renamed? Both builds come from the
# same pre-rename commit (E2E_TRANSITION_OLD_REF, default b7ae67e), product
# dictation-app, the old one 0.1.0 and the updated one 0.1.1, signed with the
# same `dictation-app Dev` certificate, bundle id com.dictationapp.desktop.
#
# The old build saves a key (the login keychain, through the app), reads it
# (has_key=true); the updater installs 0.1.1; the new process reads the key
# again, and the scenario records whether SecurityAgent put up its dialog
# (a screenshot, and the log's "no answer from the keychain"). It is a
# measurement, not a requirement: it passes either way and says what it saw
# in the evidence file. Optional: it builds two apps (E2E_KEYCHAIN_CONTROL=1).

KC_DIR=target/e2e-updater-control
KC_REF=${E2E_TRANSITION_OLD_REF:-b7ae67e}
KC_OLD=0.1.0
KC_NEW=0.1.1
KC_KEY=gsk_control_dummy_key_0123456789
KC_APP="/Users/$VM_USER/Applications/dictation-app.app"

kc_build() {
  if [ "${build:-1}" = 0 ] && [ -f "$KC_DIR/serve/good.json" ] && [ -d "$KC_DIR/new/dictation-app.app" ]; then
    log "  reusing the control builds in $KC_DIR (--no-build)"
    return 0
  fi
  local saved=$KC_DIR/regular.app
  mkdir -p "$KC_DIR"
  rm -rf "$saved"
  ditto "$APP_PATH" "$saved"
  log "  building $KC_OLD and $KC_NEW from $KC_REF (log: $OUT_DIR/control-build.log)"
  if ! UPDATER_TEST_DIR=$KC_DIR HOST=127.0.0.1 PORT=$UPD_PORT OLD_REF=$KC_REF NEW_REF=$KC_REF \
    OLD_VERSION=$KC_OLD NEW_VERSION=$KC_NEW scripts/updater-test.sh build >"$OUT_DIR/control-build.log" 2>&1; then
    log "  the control builds failed"
    rm -rf "$APP_PATH" && ditto "$saved" "$APP_PATH"
    return 1
  fi
  rm -rf "$APP_PATH" && ditto "$saved" "$APP_PATH"
  mv "$KC_DIR/serve/latest.json" "$KC_DIR/serve/good.json"
}

kc_note() {
  log "  $*"
  printf '%s\n' "$*" >>"$KC_EVIDENCE"
}

kc_engine_has_key() {
  wait_for_log 'engine settings provider=groq .*has_key=true' "${1:-20}"
}

kc_cleanup() {
  local ok=0
  fresh_app_state || ok=1
  upd_dismiss_dialogs || ok=1
  if [ "$UPD_SERVER_STOPPED" = 0 ] && ! upd_server_stop; then ok=1; fi
  vm "rm -rf $(q "$VM_UPD_DIR")" || ok=1
  vm "pkill -x SecurityAgent" >/dev/null 2>&1 || true
  tr_use_app "$KC_APP"
  tr_key_remove || ok=1
  vm "rm -rf $(q "$KC_APP")" || ok=1
  textedit_close
  close_terminal || true
  APP_NAME=$KC_APP_NAME_SAVED
  tr_use_app "/Users/$VM_USER/Applications/$APP_NAME.app"
  install_app "$APP_PATH" || ok=1
  return "$ok"
}

kc_cleanup_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  kc_cleanup || {
    log "  FAIL: the cleanup after e2e_update_keychain_control failed"
    [ "$status" != 0 ] || status=1
  }
  exit "$status"
}

e2e_update_keychain_control() {
  if [ "${E2E_KEYCHAIN_CONTROL:-0}" != 1 ]; then
    log "  skipped: set E2E_KEYCHAIN_CONTROL=1 (builds two apps)"
    SCENARIO_SKIPPED=1
    return 0
  fi
  KC_EVIDENCE="$OUT_DIR/e2e_update_keychain_control.evidence.txt"
  : >"$KC_EVIDENCE"
  KC_APP_NAME_SAVED=$APP_NAME
  trap kc_cleanup_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM HUP
  APP_NAME=com.dictationapp.desktop

  kc_build || die "could not build the control apps"
  expect "the old and the updated build have the same name" \
    test "$(plutil -extract CFBundleName raw "$KC_DIR/old/dictation-app.app/Contents/Info.plist")" = \
    "$(plutil -extract CFBundleName raw "$KC_DIR/new/dictation-app.app/Contents/Info.plist")"
  expect "the updated build is $KC_NEW" \
    test "$(plutil -extract CFBundleShortVersionString raw "$KC_DIR/new/dictation-app.app/Contents/Info.plist")" = "$KC_NEW"

  vm "rm -rf $(q "$VM_UPD_DIR") && mkdir -p $(q "$VM_UPD_DIR/www" "$VM_UPD_DIR/manifests")"
  tar -C "$KC_DIR/serve" -cf - "dictation-app_$KC_NEW.app.tar.gz" |
    VM_CMD_TIMEOUT=300 vm "tar -C $(q "$VM_UPD_DIR/www") -xf -"
  tar -C "$KC_DIR/serve" -cf - good.json | vm "tar -C $(q "$VM_UPD_DIR/manifests") -xf -"

  fresh_app_state
  expect "no update dialog is left from before" upd_dismiss_dialogs
  vm "rm -rf $(q "$KC_APP") $(q "/Users/$VM_USER/Applications/Vocet.app")"
  tr_install "$KC_DIR/old/dictation-app.app" "$KC_APP"
  tr_use_app "$KC_APP"

  groq_key_store "$KC_KEY"
  upd_serve good
  expect "the update server starts" upd_server_start
  clear_app_log
  E2E_TRANSCRIBER=settings launch_full "$VM_TONE" --debug-update-check
  expect "the old build starts" wait_for_log "INFO .*starting version=\"$KC_OLD\""
  expect "the old build reads the key it saved" kc_engine_has_key
  expect "the launch check offers $KC_NEW" upd_dialog "$KC_NEW is available; you have $KC_OLD" Later
  expect "the old build's tray check offers it again" upd_tray_check_legacy
  expect "Install and restart" upd_dialog "$KC_NEW is available; you have $KC_OLD" "Install and restart"
  expect "the app restarts as $KC_NEW" wait_for_log "INFO .*starting version=\"$KC_NEW\""
  sleep 3
  fresh_app_state
  clear_app_log
  E2E_TRANSCRIBER=settings launch_full "$VM_TONE"
  expect "the updated build starts" wait_for_log "INFO .*starting version=\"$KC_NEW\""
  if kc_engine_has_key 25; then
    kc_note "CONTROL RESULT: no prompt. The same-name build read the key saved by the old build."
  else
    kc_note "CONTROL RESULT: the same-name update could not read the key: $(app_log | grep -E 'API key|has_key' | cut -c1-240 | tail -2)"
    screenshot u1-control-keychain-prompt
    kc_note "SecurityAgent running: $(vm "pgrep -x SecurityAgent | wc -l" | tr -d ' ')"
  fi
  collect_log e2e_update_keychain_control
  trap - EXIT INT TERM HUP
  if ! kc_cleanup; then
    log "  FAIL: the cleanup after e2e_update_keychain_control failed"
    SCENARIO_FAILED=1
  fi
}
