# U1 round 2: an existing "dictation-app" install becomes Vocet. Built on the
# throwaway-key machinery of e2e_update_keeps_permissions (its upd_* helpers,
# the fixture server, scripts/updater-test.sh), but the OLD build is the
# last one from before the rename (TR_OLD_REF, productName dictation-app,
# bundle dictation-app.app) and the NEW one is this tree (Vocet). Both are
# signed `dictation-app Dev`, bundle id com.dictationapp.desktop.
#
# 1. The old build is installed as ~/Applications/dictation-app.app, with
#    Microphone and Accessibility granted (the golden image), a saved key
#    (the login keychain), non-default settings (a model and a language) and
#    launch at login on. It starts, and the log shows the engine built from
#    those settings with a key.
# 2. The way users get the new build in the app: the tray's "Check for
#    updates…" offers 0.1.1 and "Install and restart" installs it. Then: the
#    app restarts as 0.1.1, one copy; one bundle with this id in
#    ~/Applications, named for what the updater replaced; its name is Vocet;
#    the settings file, the key (engine log: same provider, model, language,
#    has_key=true) and the login item (the OS says enabled) are as before;
#    Microphone and Accessibility still hold; the updater finds a still newer
#    version (the manifest and the name of the archive work).
# 3. The way the guide tells them to install the DMG's app: Vocet.app is put
#    in ~/Applications and dictation-app.app is removed. Same checks: the
#    settings, the key, the permissions, the login item (one, enabled), no
#    copy of the old name left.
#
# The Terminal-side reboot check of the login item is e2e_launch_at_login's;
# here the login item is what the OS reports (and the Background Task
# Management dump, kept in the evidence file), not a restart.
#
# However it ends, the server is stopped, the key and the login item are
# removed, the settings reset and the run's regular build reinstalled.

TR_DIR=target/e2e-updater-transition
TR_OLD_REF=${E2E_TRANSITION_OLD_REF:-b7ae67e}
TR_OLD=0.1.0
TR_NEW=0.1.1
TR_NEWER=0.1.2
TR_KEY=gsk_transition_dummy_key_0123456789
TR_MODEL=whisper-large-v3
TR_VM_OLD_APP="/Users/$VM_USER/Applications/dictation-app.app"
TR_VM_NEW_APP="/Users/$VM_USER/Applications/Vocet.app"
TR_SETTINGS_VM="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/settings.json"
TR_BUNDLE_ID=com.dictationapp.desktop
TR_APP_NAME_SAVED=""

# The app by its bundle id, which is the same before and after the rename
# (ax.js takes a name or an id); the name Vocet is not true of the old build.
tr_use_app() {
  local exe
  VM_APP=$1
  # The executable is named by the bundle: dictation-app in the old build,
  # Vocet in the new one (also inside dictation-app.app after an update).
  exe=$(vm "defaults read $(q "$VM_APP/Contents/Info") CFBundleExecutable" 2>/dev/null) || exe=$PROC_NAME
  GROQ_VM_APP_BINARY="$VM_APP/Contents/MacOS/${exe:-$PROC_NAME}"
}

# --- Host ------------------------------------------------------------------------

tr_build() {
  if [ "${build:-1}" = 0 ] && [ -f "$TR_DIR/serve/good.json" ] && [ -d "$TR_DIR/old/dictation-app.app" ] &&
    [ -d "$TR_DIR/new/Vocet.app" ]; then
    log "  reusing the transition builds in $TR_DIR (--no-build)"
    return 0
  fi
  security find-identity -p codesigning 2>/dev/null | grep -q '"dictation-app Dev"' || {
    log "  no \"dictation-app Dev\" certificate on this Mac: the builds would be ad hoc and lose the permissions"
    return 1
  }
  local saved=$TR_DIR/regular.app
  mkdir -p "$TR_DIR"
  rm -rf "$saved"
  ditto "$APP_PATH" "$saved"
  log "  building $TR_OLD from $TR_OLD_REF and $TR_NEW from this tree (log: $OUT_DIR/transition-build.log)"
  if ! UPDATER_TEST_DIR=$TR_DIR HOST=127.0.0.1 PORT=$UPD_PORT OLD_REF=$TR_OLD_REF \
    KEEP_OLD=${E2E_TRANSITION_KEEP_OLD:-0} OLD_VERSION=$TR_OLD NEW_VERSION=$TR_NEW scripts/updater-test.sh build >"$OUT_DIR/transition-build.log" 2>&1; then
    log "  the transition builds failed"
    rm -rf "$APP_PATH" && ditto "$saved" "$APP_PATH"
    return 1
  fi
  rm -rf "$APP_PATH" && ditto "$saved" "$APP_PATH"
  mv "$TR_DIR/serve/latest.json" "$TR_DIR/serve/good.json"
  # The same bundle offered as a newer version: the app only has to find it.
  jq --arg v "$TR_NEWER" '.version = $v' "$TR_DIR/serve/good.json" >"$TR_DIR/serve/newer.json"
  jq --arg v "$TR_NEW" '.version = $v' "$TR_DIR/serve/good.json" >"$TR_DIR/serve/uptodate.json"
}

# --- VM ----------------------------------------------------------------------------

# tr_install <host .app> <vm path>: copies the bundle like install_app, to
# a given path, and tells Launch Services.
tr_install() {
  local app=$1 dest=$2
  vm "mkdir -p ~/Applications && rm -rf $(q "$dest")"
  tar -C "$(dirname "$app")" -cf - "$(basename "$app")" |
    VM_CMD_TIMEOUT=600 vm "tar -C ~/Applications -xf -"
  # tar names the folder as on the host; give it the wanted name.
  if [ "$(basename "$app")" != "$(basename "$dest")" ]; then
    vm "mv ~/Applications/$(q "$(basename "$app")") $(q "$dest")"
  fi
  vm "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f $(q "$dest")" || true
  vm "codesign --verify --deep $(q "$dest")"
}

# The .app bundles in ~/Applications that carry this bundle id.
tr_copies() {
  vm 'for a in ~/Applications/*.app; do [ "$(defaults read "$a/Contents/Info" CFBundleIdentifier 2>/dev/null)" = '"$TR_BUNDLE_ID"' ] && echo "$a"; done; true'
}

tr_copy_count() {
  tr_copies | grep -c . || true
}

tr_bundle_field() {
  vm "defaults read $(q "$1/Contents/Info") $2"
}

# What Background Task Management says about this app's login items: kept in
# the evidence file, and the count of its entries for this id.
tr_btm() {
  printf '%s\n' "${VM_PASSWORD:-lume}" |
    vm "sudo -S -p '' sfltool dumpbtm 2>&1 | grep -iE -B4 -A12 'dictationapp|dictation-app|vocet' || echo '(no entry)'"
}

TR_EVIDENCE=""

# After an update macOS asks, in a SecurityAgent dialog, whether the new
# build may read the key the old one saved (the finding this scenario
# records). The dialog covers TextEdit and would fail the permission checks
# for a reason that has nothing to do with permissions, so a loop in the VM
# closes it (the app then just can't read the key) for the next 4 minutes.
tr_prompt_killer() {
  vm "nohup sh -c 'i=0; while [ \$i -lt 120 ]; do pkill -x SecurityAgent; sleep 2; i=\$((i + 1)); done' >/dev/null 2>&1 </dev/null &" || true
}

# <app path>: the BTM dump has exactly one app entry with this bundle id and
# its URL is <app path>. Its Name (what Login Items shows) is kept as
# evidence: macOS keeps the name from the first registration.
tr_one_login_item() {
  local dump
  dump=$(printf '%s\n' "${VM_PASSWORD:-lume}" | vm "sudo -S -p '' sfltool dumpbtm 2>&1")
  [ "$(grep -c "Bundle Identifier: $TR_BUNDLE_ID" <<<"$dump")" = 1 ] || return 1
  tr_note "login item name in Login Items: $(grep -B12 "Bundle Identifier: $TR_BUNDLE_ID" <<<"$dump" | grep -m1 ' Name: ')"
  grep -B12 "Bundle Identifier: $TR_BUNDLE_ID" <<<"$dump" | grep -q "URL: file://$1/"
}
tr_note() {
  log "  $*"
  printf '%s\n' "$*" >>"$TR_EVIDENCE"
}

tr_write_settings() {
  vm "mkdir -p $(q "$(dirname "$TR_SETTINGS_VM")") && printf '%s' $(q "{\"version\":2,\"provider\":\"groq\",\"model\":\"$TR_MODEL\",\"language\":\"en\",\"hotkey\":\"RightAlt\",\"launch_at_login\":true,\"transcription\":\"cloud\",\"local_model\":\"parakeet-unified-en-0.6b-q4_k_m\",\"share_usage_events\":false}") > $(q "$TR_SETTINGS_VM")"
}

# The settings file still says what was written.
tr_settings_kept() {
  local text
  text=$(vm "cat $(q "$TR_SETTINGS_VM")")
  grep -q "\"model\":\"$TR_MODEL\"" <<<"$text" && grep -q '"language":"en"' <<<"$text" &&
    grep -q '"launch_at_login":true' <<<"$text"
}

# The running app built its engine from the saved settings, with the key.
# The engine was built from the saved settings (model and language), whatever
# the keychain said about the key.
tr_engine_settings_kept() {
  wait_for_log "engine settings provider=groq model=$TR_MODEL language=Some\\(\"en\"\\) has_key="
}

# The saved key is still in the login keychain (its metadata, never its
# value), put there by the old build. Whether the NEW build may read it
# without macOS asking is a separate question (the log says): after an
# update macOS can ask once for access to the keychain item.
tr_key_in_keychain() {
  local out
  out=$(printf '%s\n' "${VM_PASSWORD:-lume}" |
    vm "read -r pw && security unlock-keychain -p \"\$pw\" \"$GROQ_KEYCHAIN_FILE\" && unset pw && security find-generic-password -s $TR_BUNDLE_ID -a groq-api-key 2>&1 | grep -c 'class: \"genp\"'") || true
  [ "$out" = 1 ]
}

# Removes the key the scenario stored, with the security tool (the app that
# made it may be gone or a different build).
tr_key_remove() {
  printf '%s\n' "${VM_PASSWORD:-lume}" |
    vm "read -r pw && security unlock-keychain -p \"\$pw\" \"$GROQ_KEYCHAIN_FILE\" && unset pw && (security delete-generic-password -s $TR_BUNDLE_ID -a groq-api-key >/dev/null 2>&1 || true)"
  GROQ_KEY_STORED=0
}

tr_engine_from_settings() {
  if wait_for_log "engine settings provider=groq model=$TR_MODEL language=Some\\(\"en\"\\) has_key=true"; then
    return 0
  fi
  tr_note "the engine lines in the log:"
  tr_note "$(app_log | grep -iE 'engine|keychain|key' | cut -c1-300 | tail -12)"
  return 1
}

# The OS's view of the launch-at-login of the app at $VM_APP.
tr_login_is_enabled() {
  local said
  said=$(login_item status)
  tr_note "login item (from $(basename "$VM_APP")): $said"
  [ "$said" = "launch at login: enabled" ]
}

tr_cleanup() {
  local ok=0
  fresh_app_state || ok=1
  upd_dismiss_dialogs || ok=1
  if [ "$UPD_SERVER_STOPPED" = 0 ] && ! upd_server_stop; then ok=1; fi
  vm "rm -rf $(q "$VM_UPD_DIR")" || ok=1
  # The login item off, from whichever copy is installed.
  local app
  for app in "$TR_VM_OLD_APP" "$TR_VM_NEW_APP"; do
    if vm "test -d $(q "$app")"; then
      tr_use_app "$app"
      [ "$(login_item off 2>&1)" = "launch at login: disabled" ] || {
        log "  cleanup: could not turn the login item off from $app"
        ok=1
      }
    fi
  done
  vm "rm -f $(q "$TR_SETTINGS_VM")" || ok=1
  tr_key_remove || ok=1
  textedit_close
  close_terminal || true
  vm "rm -rf $(q "$TR_VM_OLD_APP") $(q "$TR_VM_NEW_APP")" || ok=1
  APP_NAME=$TR_APP_NAME_SAVED
  tr_use_app "/Users/$VM_USER/Applications/$APP_NAME.app"
  install_app "$APP_PATH" || ok=1
  return "$ok"
}

tr_cleanup_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  [ "$status" = 0 ] || collect_log e2e_update_from_dictation_app
  tr_cleanup || {
    log "  FAIL: the cleanup after e2e_update_from_dictation_app failed"
    [ "$status" != 0 ] || status=1
  }
  exit "$status"
}

# --- The scenario ------------------------------------------------------------------

e2e_update_from_dictation_app() {
  local pid
  TR_EVIDENCE="$OUT_DIR/e2e_update_from_dictation_app.evidence.txt"
  : >"$TR_EVIDENCE"
  TR_APP_NAME_SAVED=$APP_NAME
  trap tr_cleanup_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM HUP
  APP_NAME=$TR_BUNDLE_ID

  tr_build || die "could not build the old and the new app"
  expect "the old build is the old product name" \
    test "$(plutil -extract CFBundleName raw "$TR_DIR/old/dictation-app.app/Contents/Info.plist")" = dictation-app
  expect "the new build is Vocet" \
    test "$(plutil -extract CFBundleName raw "$TR_DIR/new/Vocet.app/Contents/Info.plist")" = Vocet
  expect "both have the same bundle id" \
    test "$(plutil -extract CFBundleIdentifier raw "$TR_DIR/old/dictation-app.app/Contents/Info.plist")" = \
    "$(plutil -extract CFBundleIdentifier raw "$TR_DIR/new/Vocet.app/Contents/Info.plist")"

  vm "rm -rf $(q "$VM_UPD_DIR") && mkdir -p $(q "$VM_UPD_DIR/www" "$VM_UPD_DIR/manifests")"
  tar -C "$TR_DIR/serve" -cf - "Vocet_$TR_NEW.app.tar.gz" |
    VM_CMD_TIMEOUT=300 vm "tar -C $(q "$VM_UPD_DIR/www") -xf -"
  tar -C "$TR_DIR/serve" -cf - good.json newer.json uptodate.json |
    vm "tar -C $(q "$VM_UPD_DIR/manifests") -xf -"

  fresh_app_state
  expect "no update dialog is left from before" upd_dismiss_dialogs
  vm "rm -rf $(q "$TR_VM_OLD_APP") $(q "$TR_VM_NEW_APP")"
  tr_install "$TR_DIR/old/dictation-app.app" "$TR_VM_OLD_APP"
  tr_use_app "$TR_VM_OLD_APP"
  expect "the old build is installed as dictation-app.app" test "$(tr_bundle_field "$VM_APP" CFBundleShortVersionString)" = "$TR_OLD"

  # 1. The old install: permissions, key, settings, login item.
  expect "the old build has Microphone and Accessibility" permissions_hold "$TR_OLD"
  fresh_app_state
  tr_write_settings
  groq_key_store "$TR_KEY"
  expect "launch at login is turned on in the old build" test "$(login_item on 2>&1)" = "launch at login: enabled"
  tr_note "BTM before the update:"
  tr_note "$(tr_btm)"
  upd_serve good
  expect "the update server starts" upd_server_start
  clear_app_log
  E2E_TRANSCRIBER=settings launch_full "$VM_TONE" --debug-update-check
  expect "the old build starts" wait_for_log "INFO .*starting version=\"$TR_OLD\""
  expect "its engine comes from the settings, with a key" tr_engine_from_settings
  expect "the OS says launch at login is on" tr_login_is_enabled

  # 2. The update from inside the app.
  expect "the launch check offers $TR_NEW" upd_dialog "$TR_NEW is available; you have $TR_OLD" Later
  expect "the tray's check offers it again" upd_tray_check
  expect "Install and restart" upd_dialog "$TR_NEW is available; you have $TR_OLD" "Install and restart"
  expect "the app restarts as $TR_NEW by itself" wait_for_log "INFO .*starting version=\"$TR_NEW\""
  expect "it logged the install before the restart" wait_for_log "update installed; restarting"
  sleep 3
  expect "one copy runs" test "$(app_count)" = 1
  tr_note "after the in-app update, ~/Applications holds: $(tr_copies | tr '\n' ' ')"
  expect "exactly one bundle with this id is in ~/Applications" test "$(tr_copy_count)" = 1
  expect "it is named Vocet inside" test "$(tr_bundle_field "$(tr_copies | head -1)" CFBundleName)" = Vocet
  expect "and it is $TR_NEW" test "$(tr_bundle_field "$(tr_copies | head -1)" CFBundleShortVersionString)" = "$TR_NEW"
  expect "the updated bundle verifies" vm "codesign --verify --deep --strict $(q "$(tr_copies | head -1)")"
  expect "it is signed dictation-app Dev" \
    sh -c 'grep -qx "Authority=dictation-app Dev" <<EOF
$1
EOF' _ "$(vm "codesign -dvv $(q "$(tr_copies | head -1)") 2>&1")"
  tr_use_app "$(tr_copies | head -1)"
  expect "the settings file is as it was" tr_settings_kept
  fresh_app_state
  clear_app_log
  E2E_TRANSCRIBER=settings launch_full "$VM_TONE"
  expect "the updated app starts" wait_for_log "INFO .*starting version=\"$TR_NEW\""
  expect "its engine has the same settings" tr_engine_settings_kept
  expect "the key is still in the keychain" tr_key_in_keychain
  tr_note "reading the key after the update: $(app_log | grep -E 'API key|has_key' | cut -c1-260 | tail -3)"
  expect "the OS still says launch at login is on" tr_login_is_enabled
  tr_note "BTM after the in-app update:"
  tr_note "$(tr_btm)"
  expect "Background Task Management has one login item for the app, at the updated bundle" tr_one_login_item "$VM_APP"
  tr_prompt_killer
  expect "the updated app has Microphone and Accessibility" permissions_hold "$TR_NEW"
  # The updater still finds updates: this app asks the same endpoint.
  upd_serve newer
  fresh_app_state
  clear_app_log
  launch_full "$VM_TONE" --debug-update-check
  expect "the updated app starts again" wait_for_log "INFO .*starting version=\"$TR_NEW\""
  expect "it finds a newer version ($TR_NEWER) by itself" \
    upd_dialog "$TR_NEWER is available; you have $TR_NEW" Later

  # 3. Moving to Vocet.app by the DMG: the new bundle in, the old one out.
  fresh_app_state
  upd_serve uptodate
  tr_install "$TR_DIR/new/Vocet.app" "$TR_VM_NEW_APP"
  vm "rm -rf $(q "$TR_VM_OLD_APP")"
  tr_use_app "$TR_VM_NEW_APP"
  tr_note "after the DMG-style move, ~/Applications holds: $(tr_copies | tr '\n' ' ')"
  expect "exactly one bundle with this id is left, Vocet.app" \
    test "$(tr_copies | tr -d '\n')" = "$TR_VM_NEW_APP"
  clear_app_log
  E2E_TRANSCRIBER=settings launch_full "$VM_TONE"
  expect "Vocet.app starts" wait_for_log "INFO .*starting version=\"$TR_NEW\""
  expect "its engine has the same settings" tr_engine_settings_kept
  expect "the key is still in the keychain" tr_key_in_keychain
  tr_note "reading the key after the move: $(app_log | grep -E 'API key|has_key' | cut -c1-260 | tail -3)"
  expect "the settings file is as it was" tr_settings_kept
  expect "the OS says launch at login is on for Vocet.app" tr_login_is_enabled
  tr_note "BTM after the DMG-style move:"
  tr_note "$(tr_btm)"
  expect "Background Task Management has one login item for the app, at Vocet.app" tr_one_login_item "$TR_VM_NEW_APP"
  tr_prompt_killer
  expect "Vocet.app has Microphone and Accessibility" permissions_hold "$TR_NEW"

  # Screenshots for U1's criterion 9: Vocet's first-run window, and its tray
  # menu open (a click on an item that isn't there leaves the menu open).
  fresh_app_state
  tr_key_remove
  login_item off >/dev/null 2>&1 || true
  vm "rm -f $(q "$TR_SETTINGS_VM")"
  sleep 8
  clear_app_log
  launch_full "$VM_TONE" --debug-onboarding
  if wait_for_log 'onboarding shown at launch' 20; then
    sleep 2
    screenshot u1-onboarding-window
  fi
  ax tray-click "$APP_NAME" "No such item" >/dev/null 2>&1 || true
  sleep 1
  screenshot u1-tray-menu-open

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_update_from_dictation_app
  collect_log e2e_update_from_dictation_app
  trap - EXIT INT TERM HUP
  if ! tr_cleanup; then
    log "  FAIL: the cleanup after e2e_update_from_dictation_app failed"
    SCENARIO_FAILED=1
  fi
}
