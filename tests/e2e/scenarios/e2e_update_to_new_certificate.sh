# N1 criterion 5 (#206): the move from the build signed `dictation-app Dev` to a
# build signed by another certificate, as the first Developer ID release will
# be. A second self-signed certificate, "Vocet Stand-in", stands in for Developer
# ID. It is made in a throwaway keychain inside the VM's clone, and the update is
# signed there too, so nothing is added to any keychain on the host. The update
# key is the throwaway one from scripts/updater-test.sh.
#
# 1. 0.1.0, signed dictation-app Dev, is set up the way a tester's is: Microphone
#    and Accessibility granted, setup finished, non-default settings, three
#    history entries, a saved key and launch at login on.
# 2. The update offers 0.1.1 and installs it: the app restarts as 0.1.1, signed by
#    the stand-in, with no quarantine flag.
# 3. The app sees what is gone: the log says so, and the window opens at the
#    permissions step with the plain notice.
# 4. Recovery, with no reinstall: macOS's Keychain prompt is answered (the
#    password over VNC, then Always Allow), the microphone prompt is allowed, and
#    Vocet's old row in Accessibility is removed and turned on again when macOS
#    lists it anew, as the notice says. Turning the old row off and on is not
#    enough: tried, and macOS keeps saying no.
#    The running app usually takes the grant by itself; if macOS doesn't hand it
#    over, the card's Relaunch and resume brings the window back with Done unlocked.
# 5. Dictation works with the real hotkey and the real paste, and a recording from
#    the microphone meets no permission error. Settings, history and the key are
#    as they were; the login item's state is recorded as evidence.
#
# What the real Developer ID may do differently: docs/releasing.md.
#
# However it ends, the server is stopped, the keychain and login item removed, the
# run's regular build reinstalled, and Microphone and Accessibility given back to
# it the same way (nc_restore_permissions), so that later scenarios keep them.

NC_DIR=target/e2e-new-certificate
NC_CERT="Vocet Stand-in"
NC_KEY=gsk_new_certificate_dummy_key_0123456789
NC_MODEL=whisper-large-v3
NC_VM_DIR="/Users/$VM_USER/e2e-new-certificate"
NC_KEYCHAIN="$NC_VM_DIR/standin.keychain-db"
NC_KEYCHAIN_PASSWORD=standin
NC_SETTINGS_VM="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/settings.json"
NC_PROGRESS_VM="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/onboarding-progress.json"
NC_NOTICE="macOS is asking for these again"
NC_DONE_LOCKED="Done unlocks when both are allowed"
# Set when the running app didn't take the grant and Relaunch was pressed.
NC_RELAUNCHED=0
NC_EVIDENCE=""

nc_note() {
  log "  $*"
  printf '%s\n' "$*" >>"$NC_EVIDENCE"
}

# --- Signing in the VM -----------------------------------------------------------

# The stand-in certificate, in a throwaway keychain in the VM. The keychain is
# created on the search list, so codesign and the app's own Keychain code can't
# mistake it for the login keychain only because it is named explicitly below.
nc_make_certificate() {
  vm "rm -rf $(q "$NC_VM_DIR") && mkdir -p $(q "$NC_VM_DIR")"
  # shellcheck disable=SC2087 # Expanded here on purpose: the names are ours.
  vm sh -s <<EOF || return 1
set -e
cd $(q "$NC_VM_DIR")
cat >cert.cnf <<'CNF'
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NC_CERT
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
CNF
/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 30 -config cert.cnf \
  -keyout key.pem -out cert.pem 2>/dev/null
/usr/bin/openssl pkcs12 -export -inkey key.pem -in cert.pem -name $(q "$NC_CERT") \
  -passout pass:$NC_KEYCHAIN_PASSWORD -out cert.p12
security create-keychain -p $NC_KEYCHAIN_PASSWORD $(q "$NC_KEYCHAIN")
security unlock-keychain -p $NC_KEYCHAIN_PASSWORD $(q "$NC_KEYCHAIN")
security import cert.p12 -k $(q "$NC_KEYCHAIN") -P $NC_KEYCHAIN_PASSWORD -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k $NC_KEYCHAIN_PASSWORD $(q "$NC_KEYCHAIN") >/dev/null
rm -f key.pem cert.p12
EOF
}

# nc_sign_update: signs the throwaway 0.1.1 with the stand-in in the VM (hardened,
# the same entitlements), brings the bundle back to the host, signs that with the
# throwaway update key and puts the bundle and manifest in the server's folder.
# codesign finds an untrusted identity only through the search list, so the
# keychain is on the VM's list for this one step.
nc_sign_update() {
  local new=$UPD_DIR/new/$APP_NAME.app bundle=$NC_DIR/${APP_NAME}_$UPD_NEW.app.tar.gz
  rm -rf "$NC_DIR" && mkdir -p "$NC_DIR"
  tar -C "$(dirname "$new")" -cf - "$(basename "$new")" |
    VM_CMD_TIMEOUT=300 vm "mkdir -p $(q "$NC_VM_DIR/new") && tar -C $(q "$NC_VM_DIR/new") -xf -" || return 1
  vm "cat >$(q "$NC_VM_DIR/entitlements.plist")" <src-tauri/Entitlements.plist || return 1
  # shellcheck disable=SC2087 # Expanded here on purpose: the names are ours.
  vm sh -s <<EOF || return 1
set -e
cd $(q "$NC_VM_DIR/new")
security unlock-keychain -p $NC_KEYCHAIN_PASSWORD $(q "$NC_KEYCHAIN")
before=\$(security list-keychains -d user | tr -d '"')
security list-keychains -d user -s \$before $(q "$NC_KEYCHAIN")
status=0
codesign --force --sign $(q "$NC_CERT") --options runtime --timestamp=none \
  --entitlements $(q "$NC_VM_DIR/entitlements.plist") $(q "$APP_NAME.app") || status=\$?
security list-keychains -d user -s \$before
[ "\$status" = 0 ]
codesign --verify --deep --strict $(q "$APP_NAME.app")
EOF
  VM_CMD_TIMEOUT=300 vm "cd $(q "$NC_VM_DIR/new") && COPYFILE_DISABLE=1 tar -czf - $(q "$APP_NAME.app")" >"$bundle"
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$UPD_DIR/test.key")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    npx tauri signer sign --app-version "$UPD_NEW" "$bundle" >/dev/null 2>&1 || return 1
  scripts/make-latest-json.sh "$UPD_NEW" "A test update, signed by another certificate." \
    "$bundle" "$bundle.sig" "http://127.0.0.1:$UPD_PORT/$(basename "$bundle")" >"$NC_DIR/good.json"
  vm "rm -rf $(q "$VM_UPD_DIR") && mkdir -p $(q "$VM_UPD_DIR/www" "$VM_UPD_DIR/manifests")"
  tar -C "$NC_DIR" -cf - "$(basename "$bundle")" | VM_CMD_TIMEOUT=300 vm "tar -C $(q "$VM_UPD_DIR/www") -xf -"
  tar -C "$NC_DIR" -cf - good.json | vm "tar -C $(q "$VM_UPD_DIR/manifests") -xf -"
}

# --- Before the update -----------------------------------------------------------

nc_write_finished_setup() {
  vm "printf '%s' '{\"step\":\"ready\",\"welcome_passed\":true,\"completed\":true,\"try_outcome\":\"passed\",\"download_paused\":false,\"speech_choice\":\"own_key\",\"sharing_was_off\":false}' >$(q "$NC_PROGRESS_VM")"
}

# A tester's state: finished setup, non-default settings, three history entries.
nc_seed_state() {
  vm "mkdir -p $(q "$(dirname "$NC_SETTINGS_VM")")"
  vm "printf '%s' $(q "{\"version\":2,\"provider\":\"groq\",\"model\":\"$NC_MODEL\",\"language\":\"en\",\"hotkey\":\"RightAlt\",\"launch_at_login\":true,\"transcription\":\"cloud\",\"local_model\":\"parakeet-unified-en-0.6b-q4_k_m\",\"share_usage_events\":false}") >$(q "$NC_SETTINGS_VM")"
  nc_write_finished_setup
  history_seed 3
}

# <file>'s checksum in the VM, so what is compared afterwards is byte for byte.
nc_checksum() {
  vm "shasum -a 256 <$(q "$1")"
}

# --- The Keychain prompt ----------------------------------------------------------

# Answers macOS's "wants to use your confidential information" prompt as a person
# would: the password over VNC (it refuses software keys, like the Accessibility
# sheet), then Always Allow. True if no prompt is left.
nc_answer_keychain_prompt() {
  local url said count i
  count=$(ax alert-count SecurityAgent "wants to" 2>&1) || count=0
  if [ "$count" = 0 ]; then
    nc_note "no Keychain prompt on screen"
    return 0
  fi
  nc_note "macOS asked for the key: $count prompt(s)"
  screenshot e2e_update_to_new_certificate-keychain-prompt
  url=$(vnc_url)
  # The keys go to the frontmost window; ax.js just ran in Terminal.
  vm_osascript 'tell application "System Events" to set frontmost of process "SecurityAgent" to true' || true
  sleep 1
  screenshot e2e_update_to_new_certificate-keychain-front
  said=$(python3 -I "$E2E_DIR/vnc_keys.py" "$url" "${VM_PASSWORD:-lume}" 2>&1) || {
    nc_note "typing the password failed: $said"
    return 1
  }
  sleep 1
  said=$(ax alert-press SecurityAgent "wants to" "Always Allow" 15 2>&1) || {
    nc_note "pressing Always Allow failed: $said"
    return 1
  }
  for i in 1 2 3 4 5 6 7 8 9 10; do
    count=$(ax alert-count SecurityAgent "wants to" 2>&1) || count=0
    [ "$count" != 0 ] || return 0
    sleep 1
  done
  nc_note "a Keychain prompt is still up"
  return 1
}

# --- Recovery ---------------------------------------------------------------------

# A picture of the setup window, for what a person sees.
nc_window_shot() {
  vm "osascript -e 'tell application \"System Events\" to set frontmost of first process whose name is \"$PROC_NAME\" to true'" >/dev/null 2>&1 || true
  sleep 1
  screenshot "e2e_update_to_new_certificate-$1"
}

nc_recover_microphone() {
  MIC_EVIDENCE=$NC_EVIDENCE
  ax_ok press "$APP_NAME" "Set up" "Allow Microphone" || return 1
  mic_prompt_answer Allow
}

# True once the running app has taken Accessibility up (no relaunch).
nc_ax_granted() {
  wait_for_log 'hotkey restored; shown to the user|the hotkey listens again' 20
}

# What the notice tells a person to do: remove Vocet's old row, click Allow, and
# turn on the row macOS adds. The sheets take the password over VNC; ax.js runs in
# Terminal, which takes the front, so System Settings comes back first.
nc_recover_accessibility() {
  local said
  vm "open $(q "$AX_SETTINGS_URL")" || return 1
  ax_ok wait-window "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" 15 >/dev/null || return 1
  sleep 2
  nc_note "the old row reads: $(ax check-get "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" "$AX_TOGGLE" 2>&1)"
  ax_ok row-select "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" "$APP_NAME" || return 1
  ax_ok press "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" Remove || return 1
  sleep 2
  vm "open -a $(q "$AX_SETTINGS_APP")" || return 1
  sleep 1
  said=$(python3 -I "$E2E_DIR/vnc_keys.py" "$(vnc_url)" "${VM_PASSWORD:-lume}" RETURN 2>&1) || {
    nc_note "typing the password failed: $said"
    return 1
  }
  sleep 3
  said=$(ax check-get "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" "$AX_TOGGLE" 2>&1) && {
    nc_note "the old row is still there: $said"
    return 1
  }
  nc_allow_and_turn_on
}

# Allow in the window makes macOS list the app; then its switch goes on.
nc_allow_and_turn_on() {
  local said
  ax_ok press "$APP_NAME" "Set up" "Allow Accessibility" || return 1
  sleep 3
  said=$(ax check-get "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" "$AX_TOGGLE" 2>&1) || true
  nc_note "after Allow, the new row reads: $said"
  [ "$said" = unchecked ] || return 1
  AX_EVIDENCE=$NC_EVIDENCE
  # Its answer isn't the result (it also waits for a sheet that has gone); the
  # running app taking Accessibility up is.
  ax_switch on || nc_note "ax_switch on didn't see the switch settle"
  quit_system_settings
  nc_ax_granted && return 0
  # macOS doesn't always hand a grant to the running app. The card offers this for
  # that, and a person would press it.
  nc_note "the running app didn't see the grant in 20 s: Relaunch and resume"
  NC_RELAUNCHED=1
  ax_ok press "$APP_NAME" "Set up" "Relaunch and resume" || return 1
  ax_ok wait-text "$APP_NAME" "Set up" "Two permissions" 25 || return 1
  ax_ok wait-no-text "$APP_NAME" "Set up" "Allow Accessibility" 15 &&
    ax_ok wait-no-text "$APP_NAME" "Set up" "$NC_DONE_LOCKED" 15
}

# Gives the regular build back what the scenario took from it. The golden image's
# grants were for dictation-app Dev, and the stand-in's replaced them. macOS lists an
# app in Accessibility by itself only once, so both are reset first (this VM's clone
# only). A finished setup, with no key to read, opens the window at the permissions.
nc_restore_permissions() {
  local ok=0
  fresh_app_state || ok=1
  printf '%s\n' "${VM_PASSWORD:-lume}" |
    vm "sudo -S -p '' tccutil reset Accessibility $(q "$MIC_BUNDLE_ID")" >/dev/null 2>&1 || ok=1
  vm "tccutil reset Microphone $(q "$MIC_BUNDLE_ID")" >/dev/null 2>&1 || ok=1
  vm "mkdir -p $(q "$(dirname "$NC_SETTINGS_VM")")" || ok=1
  vm "printf '%s' '{\"version\":2,\"transcription\":\"local\",\"local_model\":\"parakeet-unified-en-0.6b-q4_k_m\"}' >$(q "$NC_SETTINGS_VM")" || ok=1
  nc_write_finished_setup || ok=1
  clear_app_log
  launch --debug-onboarding || ok=1
  wait_for_log 'INFO .*ready' || ok=1
  ax_ok wait-text "$APP_NAME" "Set up" "Allow Microphone" 15 || ok=1
  nc_recover_microphone || ok=1
  vm "open $(q "$AX_SETTINGS_URL")" || ok=1
  ax_ok wait-window "$AX_SETTINGS_APP" "$AX_SETTINGS_WINDOW" 15 >/dev/null || ok=1
  nc_allow_and_turn_on || ok=1
  fresh_app_state || ok=1
  vm "rm -f $(q "$NC_SETTINGS_VM") $(q "$NC_PROGRESS_VM")" || ok=1
  [ "$ok" = 0 ] || log "  cleanup: could not give the regular build its permissions back; reset the VM"
  return "$ok"
}

# --- Cleanup ----------------------------------------------------------------------

# Gives the regular build its permissions back: this scenario changed what macOS
# holds for this bundle id (a later scenario on the same VM expects them). The
# regular build shares the golden image's certificate, so a new entry needs the
# same steps a person takes: removing and adding the row, and allowing the prompt.
nc_cleanup() {
  local ok=0
  fresh_app_state || ok=1
  upd_dismiss_dialogs || ok=1
  if [ "$UPD_SERVER_STOPPED" = 0 ] && ! upd_server_stop; then ok=1; fi
  [ "$(login_item off 2>&1)" = "launch at login: disabled" ] || ok=1
  vm "rm -f $(q "$NC_SETTINGS_VM") $(q "$NC_PROGRESS_VM")" || ok=1
  tr_key_remove || ok=1
  vm "security delete-keychain $(q "$NC_KEYCHAIN")" >/dev/null 2>&1 || true
  vm "rm -rf $(q "$NC_VM_DIR") $(q "$VM_UPD_DIR")" || ok=1
  textedit_close
  close_terminal || true
  install_app "$APP_PATH" || ok=1
  nc_restore_permissions || ok=1
  return "$ok"
}

nc_cleanup_on_exit() {
  local status=$?
  trap - EXIT INT TERM HUP
  set +e
  [ "$status" = 0 ] || collect_log e2e_update_to_new_certificate
  nc_cleanup || {
    log "  FAIL: the cleanup after e2e_update_to_new_certificate failed"
    [ "$status" != 0 ] || status=1
  }
  exit "$status"
}

# --- The scenario -----------------------------------------------------------------

e2e_update_to_new_certificate() {
  local settings_before history_before details pid
  NC_EVIDENCE="$OUT_DIR/e2e_update_to_new_certificate.evidence.txt"
  : >"$NC_EVIDENCE"
  trap nc_cleanup_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM HUP

  upd_build || die "could not build the throwaway update"
  nc_make_certificate || die "could not make the stand-in certificate in the VM"
  nc_sign_update || die "could not sign the update with the stand-in"

  fresh_app_state
  expect "no update dialog is left from before" upd_dismiss_dialogs
  install_app "$UPD_DIR/old/$APP_NAME.app"
  expect "0.1.0 has Microphone and Accessibility" permissions_hold "$UPD_OLD"
  fresh_app_state

  # 1. A tester's state, before the update.
  nc_seed_state
  groq_key_store "$NC_KEY"
  expect "launch at login is on" test "$(login_item on 2>&1)" = "launch at login: enabled"
  settings_before=$(nc_checksum "$NC_SETTINGS_VM")
  history_before=$(nc_checksum "$VM_HISTORY_FILE")
  nc_note "0.1.0's designated requirement: $(vm "codesign -dr - $(q "$VM_APP") 2>&1 | tail -1")"
  upd_serve good
  expect "the update server starts" upd_server_start
  clear_app_log

  # 2. As a person: no test flags, so the launch check runs by itself.
  vm "open -n $(q "$VM_APP")"
  expect "0.1.0 starts" wait_for_log "INFO .*starting version=\"$UPD_OLD\""
  expect "0.1.0 reads the key it saved" wait_for_log 'engine settings provider=groq .*has_key=true'
  expect "the launch check offers $UPD_NEW" upd_dialog "$UPD_NEW is available; you have $UPD_OLD" "Install and restart"
  expect "the app restarts as $UPD_NEW by itself" wait_for_log "INFO .*starting version=\"$UPD_NEW\""
  sleep 3
  expect "one copy runs" test "$(app_count)" = 1
  expect "$UPD_NEW is installed" test "$(upd_installed_version)" = "$UPD_NEW"
  expect "the updated bundle's signature verifies" vm "codesign --verify --deep --strict $(q "$VM_APP")"
  details=$(vm "codesign -dvv $(q "$VM_APP") 2>&1")
  expect "it is signed by the stand-in" grep -qx "Authority=$NC_CERT" <<<"$details"
  expect "it has the hardened runtime" grep -q 'flags=.*runtime' <<<"$details"
  expect "no quarantine flag came with the update" e2e_not_quarantined "$VM_APP"
  nc_note "$UPD_NEW's designated requirement: $(vm "codesign -dr - $(q "$VM_APP") 2>&1 | tail -1")"

  # 3. It sees what is gone, and says so. The key is read at launch, so macOS's
  # prompt may be up first.
  sleep 5
  screenshot e2e_update_to_new_certificate-after-update
  nc_note "the log right after the update:"
  nc_note "$(app_log | grep -E 'onboarding (shown|not shown)|PermissionMissing|API key|has_key' | cut -c1-260 | tail -8)"
  expect "the log says the hotkey is off (Accessibility is gone)" \
    wait_for_log 'PermissionMissing: the hotkey is off until Accessibility is granted' 10
  expect "setup opens by itself" wait_for_log 'onboarding shown at launch'
  expect "the Keychain prompt is answered" nc_answer_keychain_prompt
  expect "the window is at the permissions step" ax_ok wait-text "$APP_NAME" "Set up" "Two permissions" 25
  expect "it says plainly what happened" ax_ok wait-text "$APP_NAME" "Set up" "$NC_NOTICE" 10
  textedit_close
  close_terminal || true
  nc_window_shot notice
  pid=$(app_pid)

  # 4. Recovery. The microphone first: macOS has forgotten the answer, so it asks.
  expect "the microphone is given back through macOS's prompt" nc_recover_microphone
  expect "Accessibility is given back as the notice says" nc_recover_accessibility
  expect "both are allowed: Done is unlocked" ax_ok wait-no-text "$APP_NAME" "Set up" "$NC_DONE_LOCKED" 15
  close_terminal || true
  nc_window_shot both-allowed
  if [ "$NC_RELAUNCHED" = 0 ]; then
    expect "the same process recovered: no relaunch, no reinstall" test "$pid" = "$(app_pid)"
  else
    expect "it recovered through Relaunch: no reinstall" test "$(upd_installed_version)" = "$UPD_NEW"
  fi
  expect "Done closes the window: no second setup" ax_ok press "$APP_NAME" "Set up" Done
  expect "the log says setup finished" wait_for_log 'onboarding finished' 10

  # 5. Everything else stayed.
  expect "the settings file is byte for byte the same" test "$(nc_checksum "$NC_SETTINGS_VM")" = "$settings_before"
  expect "the history file is byte for byte the same" test "$(nc_checksum "$VM_HISTORY_FILE")" = "$history_before"
  expect "the key is still in the login keychain" tr_key_in_keychain
  expect "the new build reads the key" wait_for_log 'engine settings provider=groq .*has_key=true' 30
  nc_note "login item after the update, as the OS says: $(login_item status 2>&1)"
  nc_note "$(tr_btm | head -30)"
  expect "a dictation and a microphone recording work" permissions_hold "$UPD_NEW"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_update_to_new_certificate
  collect_log e2e_update_to_new_certificate
  trap - EXIT INT TERM HUP
  if ! nc_cleanup; then
    log "  FAIL: the cleanup after e2e_update_to_new_certificate failed"
    SCENARIO_FAILED=1
  fi
}
