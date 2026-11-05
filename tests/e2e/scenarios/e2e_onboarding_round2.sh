# U8 round 2: the Speech choice survives a quit. The own-key engine is picked on
# Speech, the app quit from the tray, and the relaunch comes back at Speech with
# that choice kept (progress file and UI). U10 took out the part that switched
# sharing off on Welcome: the switch is on Ready now, and its writes and the exit
# guard are covered in src/Onboarding.test.tsx.

R2_WINDOW="Set up"

r2_launch() { launch --debug-onboarding; }

r2_progress_has() {
  vm "cat $(q "$SETUP_PROGRESS_VM")" | tee -a /dev/stderr | grep -q "$1"
}

e2e_onboarding_round2() {
  fresh_app_state
  settings_reset
  r2_launch
  expect "onboarding shows at Welcome" wait_for_log 'onboarding shown at launch step=Welcome' 30
  expect "Welcome" ax_ok wait-text "$APP_NAME" "$R2_WINDOW" "Talk instead of type" 15
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

wait_until_progress() {
  local i
  for i in $(seq 1 10); do
    vm "cat $(q "$SETUP_PROGRESS_VM")" 2>/dev/null | grep -q "\"speech_choice\":\"$1\"" && return 0
    sleep 1
  done
  log "  progress: $(vm "cat $(q "$SETUP_PROGRESS_VM")" 2>&1)"
  return 1
}
