# U8 round 2: Continue waits for the Welcome switch to persist, and the Speech
# choice survives a quit. Sharing is switched off and Continue pressed at once,
# five times (Back in between): the saved settings say off, the app never
# generated an install ID (what sending needs) and the log has no send. Then
# the own-key engine is picked on Speech, the app quit from the tray, and the
# relaunch comes back at Speech with that choice kept (progress file and UI).

R2_WINDOW="Set up"
R2_DATA="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop"

r2_launch() { launch --debug-onboarding; }

r2_progress_has() {
  vm "cat $(q "$SETUP_PROGRESS_VM")" | tee -a /dev/stderr | grep -q "$1"
}

e2e_onboarding_round2() {
  fresh_app_state
  settings_reset
  vm "find $(q "$R2_DATA") -iname '*install*' -delete 2>/dev/null; true"
  r2_launch
  expect "onboarding shows at Welcome" wait_for_log 'onboarding shown at launch step=Welcome' 30
  expect "Welcome" ax_ok wait-text "$APP_NAME" "$R2_WINDOW" "Talk instead of type" 15
  local i
  for i in 1 2 3 4 5; do
    ax_ok check-set "$APP_NAME" "$R2_WINDOW" ob-share off || true
    ax press "$APP_NAME" "$R2_WINDOW" Continue >/dev/null 2>&1 || true
    expect "round $i: Speech opens" ax_ok wait-text "$APP_NAME" "$R2_WINDOW" "Which language" 15
    expect "round $i: settings say sharing is off" settings_has_sharing_off
    ax press "$APP_NAME" "$R2_WINDOW" Back >/dev/null 2>&1 || true
    ax_ok wait-text "$APP_NAME" "$R2_WINDOW" "Talk instead of type" 10 || true
  done
  expect "no install ID was made" test -z "$(vm "find $(q "$R2_DATA") -iname '*install*' 2>/dev/null")"
  expect "the log has no send or drop of events" \
    test "$(app_log | grep -c -i -E 'usage events|posthog|batch')" = 0

  ax press "$APP_NAME" "$R2_WINDOW" Continue >/dev/null 2>&1 || true
  expect "Speech" ax_ok wait-text "$APP_NAME" "$R2_WINDOW" "Which language" 15
  expect "choose my own key" \
    ax_ok radio-press "$APP_NAME" "$R2_WINDOW" "Use my own Groq or OpenAI key"
  expect "the choice is saved before any quit" wait_until_progress own_key
  expect "quit from the tray" quit_from_tray
  clear_app_log
  r2_launch
  expect "it comes back at Speech" wait_for_log 'onboarding shown at launch step=Speech' 30
  expect "own key is still the choice (panel shown)" \
    ax_ok wait-text "$APP_NAME" "$R2_WINDOW" "Continue unlocks after Test" 15
  ax dump "$APP_NAME" 2>/dev/null | grep -a -i -E "resum|continue|earlier|picked|kept" | head -10 |
    while IFS= read -r line; do log "  UI: $line"; done
  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_onboarding_round2
  collect_log e2e_onboarding_round2
  fresh_app_state
  settings_reset
}

settings_has_sharing_off() {
  vm "cat $(q "$SETTINGS_FILE_VM")" | tr -d ' \n' | grep -q '"share_usage_events":false'
}

wait_until_progress() {
  local i
  for i in $(seq 1 10); do
    vm "cat $(q "$SETUP_PROGRESS_VM")" 2>/dev/null | grep -q "\"speech_choice\":\"$1\"" && return 0
    sleep 1
  done
  log "  progress: $(vm "cat $(q "$SETUP_PROGRESS_VM")" 2>&1)"
  return 1
}
