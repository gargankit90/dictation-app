# U8 criterion 9 evidence: the onboarding window alone (screencapture -l), each
# step, the download strip and the resume state, in light and dark. The
# appearance is read when the app launches, so each mode has a launch of its
# own. Little is asserted; the pictures go to the run's output for the PR, and
# the appearance is put back to light at the end. Downloads the model twice.

OBS_WINDOW="Set up"
OBS_MODELS="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/models"

obs_launch() {
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_FIXTURES/speech-16k.wav" --debug-onboarding
}

obs_appearance() {
  vm "osascript -e 'tell application \"System Events\" to tell appearance preferences to set dark mode to $([ "$1" = dark ] && echo true || echo false)'"
  sleep 2
}

# The onboarding window's fields, from the cua-driver daemon, which may capture
# windows where an ssh session or a Terminal job may not.
obs_window() {
  vm "~/.local/bin/cua-driver call list_windows '{}'" |
    jq -c '[.windows[] | select(.app_name == "'"$APP_NAME"'" and (.title | contains("Set up")))] | first // empty'
}

# obs_shot <name>: the window, as <name>.png in the run's output.
obs_shot() {
  local window id pid png="$OUT_DIR/onboarding-$1.png"
  sleep 1
  window=$(obs_window)
  if [ -z "$window" ]; then
    log "  no window for $1"
    return 0
  fi
  id=$(printf '%s' "$window" | jq -r .window_id)
  pid=$(printf '%s' "$window" | jq -r .pid)
  vm "rm -f /tmp/obs.png; ~/.local/bin/cua-driver call get_window_state '{\"pid\":$pid,\"window_id\":$id,\"include_accessibility_tree\":false,\"screenshot_out_file\":\"/tmp/obs.png\"}' >/dev/null" || true
  vm "base64 -i /tmp/obs.png" | base64 -d >"$png" || true
  log "  window screenshot $png: $(wc -c <"$png") bytes"
}

obs_size() {
  local window id pid x y
  window=$(obs_window)
  [ -n "$window" ] || return 0
  id=$(printf '%s' "$window" | jq -r .window_id)
  pid=$(printf '%s' "$window" | jq -r .pid)
  x=$(printf '%s' "$window" | jq -r .bounds.x)
  y=$(printf '%s' "$window" | jq -r .bounds.y)
  vm "~/.local/bin/cua-driver call set_window_frame '{\"pid\":$pid,\"window_id\":$id,\"x\":$x,\"y\":$y,\"width\":$1,\"height\":$2}' >/dev/null" || true
}

obs_mode() {
  local mode=$1
  fresh_app_state
  settings_reset
  vm "rm -rf $(q "$OBS_MODELS")"
  obs_appearance "$mode"
  obs_launch
  expect "$mode: the window" ax_ok wait-window "$APP_NAME" "$OBS_WINDOW" 30
  expect "$mode: Welcome" ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Talk instead of type" 30
  obs_shot "1-welcome-$mode"
  ax_ok press "$APP_NAME" "$OBS_WINDOW" Continue
  expect "$mode: Speech" ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Parakeet English" 15
  obs_shot "2-speech-english-$mode"
  ax_ok radio-press "$APP_NAME" "$OBS_WINDOW" Hindi
  expect "$mode: Hindi" ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Multilingual" 10
  obs_shot "2-speech-hindi-$mode"
  # The longest labels, at the smallest window.
  obs_size 680 520
  obs_shot "2-speech-hindi-680x520-$mode"
  obs_size 740 560
  ax_ok radio-press "$APP_NAME" "$OBS_WINDOW" English
  ax_ok radio-press "$APP_NAME" "$OBS_WINDOW" "Use my own Groq or OpenAI key"
  expect "$mode: the own key panel" ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Continue unlocks after Test" 10
  obs_shot "2-speech-own-key-$mode"
  ax_ok radio-press "$APP_NAME" "$OBS_WINDOW" "Recommended for your language"
  ax_ok press "$APP_NAME" "$OBS_WINDOW" "Download and continue"
  # The VM downloads in seconds, so each state below cancels, removes what was
  # fetched and starts again to be sure it is on screen for its picture.
  expect "$mode: the strip" ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Downloading Parakeet English" 15
  obs_shot "3-permissions-download-strip-$mode"
  ax_ok press "$APP_NAME" "$OBS_WINDOW" Cancel
  expect "$mode: paused" ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Download paused" 10
  obs_shot "3-permissions-download-paused-$mode"
  obs_size 680 520
  obs_shot "3-permissions-680x520-$mode"
  obs_size 740 560
  ax_ok press "$APP_NAME" "$OBS_WINDOW" Continue
  ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Your dictation key" 10
  obs_shot "4-try-model-not-downloaded-$mode"
  vm "rm -rf $(q "$OBS_MODELS")"
  ax_ok press "$APP_NAME" "$OBS_WINDOW" Resume
  expect "$mode: practice waits, with the percentage" \
    ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Getting the model ready: " 10
  obs_shot "4-try-waiting-for-model-$mode"
  ax_ok press "$APP_NAME" "$OBS_WINDOW" Cancel
  ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Practice starts once it is" 10
  ax_ok press "$APP_NAME" "$OBS_WINDOW" Back
  ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Two permissions" 10
  vm "rm -rf $(q "$OBS_MODELS")"
  ax_ok press "$APP_NAME" "$OBS_WINDOW" Resume
  ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Downloading Parakeet English" 10
  expect "$mode: quit mid-download" quit_from_tray
  clear_app_log
  obs_launch
  expect "$mode: resumed" ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Continued from" 30
  obs_shot "3-resume-after-quit-$mode"
  ax_ok press "$APP_NAME" "$OBS_WINDOW" Continue
  ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "Your dictation key" 10
  VM_TERMINAL_TIMEOUT=1510 ax_ok wait-no-text "$APP_NAME" "$OBS_WINDOW" "Getting the model ready" 1500
  # The test hook's press isn't held back while practice waits, as the key is.
  expect "$mode: the model loaded" wait_for_log 'on-device model loaded' 120
  sleep 2
  obs_shot "4-try-open-$mode"
  textedit_fresh
  dictate_once 2
  expect "$mode: the missed paste" ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "didn't reach the box" 10
  obs_shot "4-try-paste-missed-$mode"
  # The panel scrolls into view when it appears: at the smallest window too.
  obs_size 680 520
  ax_ok field-set "$APP_NAME" "$OBS_WINDOW" try-field ""
  textedit_fresh
  dictate_once 2
  sleep 2
  obs_shot "4-try-paste-missed-680x520-$mode"
  obs_size 740 560
  ax_ok field-set "$APP_NAME" "$OBS_WINDOW" try-field ""
  dictate_once 2
  expect "$mode: it works" ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "It works." 10
  obs_shot "4-try-success-$mode"
  ax_ok press "$APP_NAME" "$OBS_WINDOW" Continue
  expect "$mode: Ready" ax_ok wait-text "$APP_NAME" "$OBS_WINDOW" "You're set" 10
  obs_shot "5-ready-$mode"
  textedit_close
}

e2e_onboarding_screenshots() {
  obs_mode light
  obs_mode dark
  obs_appearance light
  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_onboarding_screenshots
  collect_log e2e_onboarding_screenshots
  fresh_app_state
  settings_reset
  vm "rm -rf $(q "$OBS_MODELS")"
}
