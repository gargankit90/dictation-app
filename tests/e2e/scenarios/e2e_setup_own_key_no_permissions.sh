# U9 criterion 1, the own-key route (Tester, run 1): from a clean first launch
# with Accessibility really switched off, choosing "Use my own key", Test & Save
# against the local provider (_provider_helpers.sh; a made-up key) and Continue
# moves to Permissions. Step 2 never shows the "can't listen to the keyboard"
# message or the raw key name. U9's own scenario covers the download route.

OWNKEY_WINDOW="Set up"
OWNKEY_LISTEN="can't listen to the keyboard"

ownkey_restore() {
  local ok=0
  ax_restore || ok=1
  provider_cleanup || ok=1
  return "$ok"
}

ownkey_message_absent() {
  if ax dump "$APP_NAME" 2>/dev/null | grep -aqE "$OWNKEY_LISTEN|RightAlt"; then
    log "  the window shows the hotkey's problem or its raw name"
    return 1
  fi
}

e2e_setup_own_key_no_permissions() {
  AX_EVIDENCE="$OUT_DIR/e2e_setup_own_key_no_permissions.evidence.txt"
  : >"$AX_EVIDENCE"
  arm_key_cleanup e2e_setup_own_key_no_permissions
  KEY_CLEANUP_HOOK=ownkey_restore
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"
  AX_SWITCHED_OFF=1
  expect "Accessibility is switched off for the app" ax_switch off

  expect "the app starts on the local provider" launch_on_provider --debug-onboarding
  expect "onboarding shows at launch, at Welcome" \
    wait_for_log 'onboarding shown at launch step=Welcome' 30
  expect "its window is on screen" ax_ok wait-window "$APP_NAME" "$OWNKEY_WINDOW" 10
  expect "the hotkey is off" wait_for_log 'PermissionMissing' 15
  expect "press Get started" ax_ok press "$APP_NAME" "$OWNKEY_WINDOW" "Get started"
  expect "choose my own key" \
    ax_ok radio-press "$APP_NAME" "$OWNKEY_WINDOW" "Use my own Groq or OpenAI key"
  expect "press Continue" ax_ok press "$APP_NAME" "$OWNKEY_WINDOW" Continue
  expect "the next screen is Connect your key" \
    ax_ok wait-text "$APP_NAME" "$OWNKEY_WINDOW" "Connect your key" 10
  expect "type the key the provider takes" \
    ax_ok secret-set "$APP_NAME" "$OWNKEY_WINDOW" api-key "$PROVIDER_GOOD_KEY"
  expect "press Test & Save" ax_ok press "$APP_NAME" "$OWNKEY_WINDOW" "Test & Save"
  expect "the window says the key is saved" \
    ax_ok wait-text "$APP_NAME" "$OWNKEY_WINDOW" "Key saved" 15
  GROQ_KEY_STORED=1
  expect "step 2 doesn't mention the hotkey" ownkey_message_absent
  expect "press Continue" ax_ok press "$APP_NAME" "$OWNKEY_WINDOW" Continue
  expect "it moves to Permissions" \
    ax_ok wait-text "$APP_NAME" "$OWNKEY_WINDOW" "Records only while you hold your key" 15
  expect "the window never showed the hotkey's raw name" ownkey_message_absent

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_setup_own_key_no_permissions
  collect_log e2e_setup_own_key_no_permissions
}
