# U9 criterion 3: the owner's stuck setup. From a clean first launch with
# neither Accessibility nor the microphone allowed, step 2 still goes forward:
# Download and continue starts the download and moves to Permissions, and step 2
# never shows the "can't listen to the keyboard" message. Before U9 the hotkey
# waiting for Accessibility counted as a failed save, so the step stayed put and
# showed that message, with the key's raw name.
#
# Accessibility is really off (System Settings' switch, as in
# e2e_accessibility_revoked) and the microphone really reset (as in
# e2e_microphone_denied_then_recover), in this VM's clone only. The app has no
# stand-in for the model's server, so this starts the real download and cancels
# it once the strip shows it: nothing depends on it finishing. The cleanup
# restores both permissions, whatever happens, for the scenarios after it.

NOPERM_WINDOW="Set up"
NOPERM_MODELS="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/models"
NOPERM_LISTEN="can't listen to the keyboard"

noperm_restore() {
  local ok=0
  ax_restore || ok=1
  mic_restore || ok=1
  vm "rm -rf $(q "$NOPERM_MODELS")" || ok=1
  return "$ok"
}

noperm_listen_message_absent() {
  if ax dump "$APP_NAME" 2>/dev/null | grep -aqE "$NOPERM_LISTEN|RightAlt"; then
    log "  the window shows the hotkey's problem or its raw name"
    return 1
  fi
}

e2e_setup_no_permissions() {
  AX_EVIDENCE="$OUT_DIR/e2e_setup_no_permissions.evidence.txt"
  MIC_EVIDENCE=$AX_EVIDENCE
  : >"$AX_EVIDENCE"
  arm_key_cleanup e2e_setup_no_permissions
  KEY_CLEANUP_HOOK=noperm_restore
  fresh_app_state
  settings_reset
  vm "rm -rf $(q "$NOPERM_MODELS")"

  AX_SWITCHED_OFF=1
  expect "Accessibility is switched off for the app" ax_switch off
  expect "the microphone permission is reset" mic_reset

  E2E_TRANSCRIBER=settings launch --debug-onboarding
  expect "onboarding shows at launch, at Welcome" \
    wait_for_log 'onboarding shown at launch step=Welcome' 30
  expect "its window is on screen" ax_ok wait-window "$APP_NAME" "$NOPERM_WINDOW" 10
  expect "the hotkey is off, as for a person who hasn't allowed it yet" \
    wait_for_log 'PermissionMissing' 15
  expect "press Continue" ax_ok press "$APP_NAME" "$NOPERM_WINDOW" Continue

  expect "Speech recommends Parakeet English" \
    ax_ok wait-text "$APP_NAME" "$NOPERM_WINDOW" "Parakeet English" 15
  expect "step 2 doesn't mention the hotkey" noperm_listen_message_absent
  expect "press Hindi" ax_ok radio-press "$APP_NAME" "$NOPERM_WINDOW" Hindi
  expect "choosing a language doesn't fail either" \
    ax_ok wait-no-text "$APP_NAME" "$NOPERM_WINDOW" "$NOPERM_LISTEN" 3
  expect "press English again" ax_ok radio-press "$APP_NAME" "$NOPERM_WINDOW" English
  expect "and Parakeet English comes back" \
    ax_ok wait-text "$APP_NAME" "$NOPERM_WINDOW" "Parakeet English" 15
  expect "press Download and continue" \
    ax_ok press "$APP_NAME" "$NOPERM_WINDOW" "Download and continue"

  expect "it moves to Permissions" \
    ax_ok wait-text "$APP_NAME" "$NOPERM_WINDOW" "Records only while you hold your key" 15
  expect "the download started: the strip shows it" \
    ax_ok wait-text "$APP_NAME" "$NOPERM_WINDOW" "Downloading Parakeet English" 15
  expect "the settings choose the model" settings_file_has transcription local
  ax press "$APP_NAME" "$NOPERM_WINDOW" Cancel >/dev/null 2>&1 || true
  expect "the window never showed the hotkey's raw name" noperm_listen_message_absent

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_setup_no_permissions
  collect_log e2e_setup_no_permissions
}
