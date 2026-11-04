# T4a, #114: a user whose key is wrong fixes it in Settings and dictation
# comes back, in the same process and after a relaunch.
#
# The local provider (_provider_helpers.sh) accepts one made-up key. The
# keychain starts with another one: dictating shows the provider's
# rejection, pastes nothing and leaves the clipboard alone. Then the right
# key is typed into the real Settings window (Speech, the own-key panel's
# password field, as a person types) and Test & Save (U6: no global Save)
# tests it against the provider with the typed key, saves it, says
# "✓ Connected · saved in Keychain", and the engine is rebuilt. A wrong key
# typed first is refused by the test and not saved.
# Without a relaunch (same pid) the next dictation pastes the transcript,
# sent with the new key, and the tray's error is gone; after Quit and a
# relaunch it still does. Neither key is ever in the app's log, settings,
# caches or web storage, or in the provider's files: the app read it from
# the keychain.

# The app's own files in the VM, where a key must never be.
APP_FILES_VM="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop
$VM_LOG_DIR
/Users/$VM_USER/Library/Caches/com.dictationapp.desktop
/Users/$VM_USER/Library/WebKit/com.dictationapp.desktop
/Users/$VM_USER/Library/Preferences/com.dictationapp.desktop.plist"

# no_file_holds <key>: no file of the app's (APP_FILES_VM) or the provider's
# holds <key>. The key reaches grep on stdin, never as an argument. A control
# file holding the key is searched too and must be found, so a search that
# can't work (a broken grep, no folders) fails instead of passing; at least
# the settings folder, the log folder and the provider's must exist.
no_file_holds() {
  local out paths=() path found searched
  while IFS= read -r path; do paths+=("$path"); done <<<"$APP_FILES_VM"
  paths+=("$VM_PROVIDER_DIR")
  out=$(printf '%s\n' "$1" | vm "sh -c '
    IFS= read -r key
    control=\$(mktemp -d)
    printf \"%s\\n\" \"\$key\" >\"\$control/holds-the-key\"
    n=0
    for p in \"\$control\" \"\$@\"; do
      [ -e \"\$p\" ] || continue
      n=\$((n + 1))
      printf \"%s\\n\" \"\$key\" | grep -rlF -f - \"\$p\"
    done
    rm -rf \"\$control\"
    echo searched=\$n
  ' _ $(q "${paths[@]}")") || {
    log "  could not search the VM's files"
    return 1
  }
  searched=$(printf '%s\n' "$out" | sed -n 's/^searched=//p')
  found=$(printf '%s\n' "$out" | grep -v '^searched=' | grep -v '/holds-the-key$' || true)
  if ! printf '%s\n' "$out" | grep -q '/holds-the-key$'; then
    log "  the search didn't find the key in its control file: [$out]"
    return 1
  fi
  if [ "${searched:-0}" -lt 4 ]; then
    log "  only ${searched:-0} places were searched (the control and fewer than 3 folders)"
    return 1
  fi
  [ -z "$found" ] && return 0
  log "  these files hold the key: $found"
  return 1
}

e2e_settings_key_provider_recovery() {
  local pid rebuilt
  arm_key_cleanup e2e_settings_key_provider_recovery
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"
  provider_key_store "$PROVIDER_BAD_KEY"

  expect "the app starts on the local provider" launch_on_provider
  expect "with the saved (wrong) key" wait_for_log 'engine settings provider=groq .*has_key=true'
  pid=$(app_pid)
  log "  the app's pid: $pid"
  textedit_fresh
  set_clipboard "$PROVIDER_SENTINEL"

  # The wrong key.
  expect "a dictation with the wrong key ends Idle" dictate_once 1
  expect "it reached the provider with the wrong key" provider_request_has 1 auth=bad
  expect "which rejected it" provider_wait_event '^replied n=1 status=401'
  expect "the user is told the key was rejected" \
    wait_for_log 'shown to the user error=Transcription failed: The provider rejected your API key'
  expect "nothing was inserted" test "$(phases_since 0)" = "Recording Transcribing Idle"
  expect "TextEdit got nothing (read back exactly)" textedit_is ""
  expect "the clipboard is untouched" test "$(clipboard)" = "$PROVIDER_SENTINEL"
  expect "the tray shows the error, and still fits in the menu bar (#151)" \
    tray_shows "$TRAY_ERROR_TITLE" "$TRAY_TOOLTIP: The provider rejected your API key. Check or re-enter it in Settings. (key_rejected)"
  expect "the wrong key wasn't retried" provider_requests_are 1

  # Fix it in Settings, as a user would.
  expect "Settings… in the tray opens the window" open_settings
  expect "the Speech section opens" ax_ok radio-press "$APP_NAME" "$SETTINGS_WINDOW" Speech
  expect "it says a key is saved" \
    ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "A key is saved in your Keychain." 10
  expect "there is no Save button" sh -c '! ax press "$1" "$2" Save >/dev/null 2>&1' _ "$APP_NAME" "$SETTINGS_WINDOW"
  # The wrong key again: Test & Save tests it and saves nothing.
  expect "type the wrong key into the key field" \
    ax_ok secret-set "$APP_NAME" "$SETTINGS_WINDOW" api-key "$PROVIDER_BAD_KEY"
  expect "press Test & Save" ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" "Test & Save"
  expect "the window says the key was rejected" \
    ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "rejected" 15
  expect "the test reached the local provider with the typed key" provider_request_has 2 auth=bad
  expect "and it did not say saved" \
    sh -c '! ax wait-text "$1" "$2" "saved in Keychain" 2 >/dev/null 2>&1' _ "$APP_NAME" "$SETTINGS_WINDOW"
  rebuilt=$(log_count 'engine rebuilt')
  expect "type the right key into the key field" \
    ax_ok secret-set "$APP_NAME" "$SETTINGS_WINDOW" api-key "$PROVIDER_GOOD_KEY"
  expect "press Test & Save" ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" "Test & Save"
  expect "the window says it is connected and saved" \
    ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "✓ Connected · saved in Keychain" 15
  expect "the test reached the local provider with the typed key" provider_request_has 3 auth=good
  expect "the engine was rebuilt from the new key" \
    wait_for_log_count 'engine rebuilt' $((rebuilt + 1)) 10
  expect "closing the window hides it" close_settings

  # No relaunch: the next dictation pastes.
  textedit_fresh
  expect "a dictation after the save ends Idle" dictate_once 1
  expect "it was sent with the new key" provider_request_has 4 auth=good
  expect "with the default model, no language" provider_request_has 4 "model=whisper-large-v3-turbo language=-"
  expect "the provider answered it" provider_wait_event '^replied n=4 status=200'
  expect "TextEdit holds exactly the transcript" textedit_is "$(esc_b64 "$PROVIDER_TEXT")"
  expect "it went through Inserting" \
    test "$(phases_since 3)" = "Recording Transcribing Inserting Idle"
  expect "the clipboard is the user's again" test "$(clipboard)" = "$PROVIDER_SENTINEL"
  expect "the tray's error is gone" tray_is_clear
  expect "the same process did it (no relaunch)" test "$(app_pid)" = "$pid"
  expect "no file holds the wrong key" no_file_holds "$PROVIDER_BAD_KEY"
  expect "no file holds the right key" no_file_holds "$PROVIDER_GOOD_KEY"
  collect_log e2e_settings_key_provider_recovery-before-relaunch

  # And after a relaunch.
  expect "Quit from the tray ends the app by itself" quit_from_tray
  clear_app_log
  expect "the app starts again on the local provider" launch_on_provider
  expect "with a saved key" wait_for_log 'engine settings provider=groq .*has_key=true'
  textedit_fresh
  expect "a dictation after the relaunch ends Idle" dictate_once 1
  expect "it was sent with the new key" provider_request_has 5 auth=good
  expect "TextEdit holds exactly the transcript again" textedit_is "$(esc_b64 "$PROVIDER_TEXT")"
  expect "five requests in all (no retry, nothing extra)" provider_requests_are 5
  expect "no file holds the right key after the relaunch" no_file_holds "$PROVIDER_GOOD_KEY"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_settings_key_provider_recovery
  collect_log e2e_settings_key_provider_recovery
  key_scenario_end
}
