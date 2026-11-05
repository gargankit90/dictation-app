# U11 criterion 10 evidence: the setup window alone, from the running app, in light
# and dark: Welcome's demo as a run of frames and with Reduce Motion, Speech's
# cards, both states of Connect your key, and Try it at both sizes with a two-key
# shortcut. Down the own-key route on the local provider (a made-up key), so nothing
# downloads. The pictures go to the run's output as setup-*.png; little is asserted.

SRS_WINDOW="Set up"
SRS_FRAMES=14

srs_restore() {
  local ok=0
  provider_cleanup || ok=1
  srs_reduce_motion false || ok=1
  obs_appearance light || ok=1
  return "$ok"
}

# The window server's setting; the web view follows it.
srs_reduce_motion() {
  vm "defaults write com.apple.universalaccess reduceMotion -bool $1; killall cfprefsd" >/dev/null 2>&1 || true
  sleep 2
  vm "defaults read com.apple.universalaccess reduceMotion" 2>/dev/null | grep -q "$([ "$1" = true ] && echo 1 || echo 0)"
}

# srs_shot <name>: the window now, without waiting: the demo is meant to move.
srs_shot() {
  local window id pid png="$OUT_DIR/setup-$1.png"
  window=$(obs_window)
  [ -n "$window" ] || return 0
  id=$(printf '%s' "$window" | jq -r .window_id)
  pid=$(printf '%s' "$window" | jq -r .pid)
  vm "rm -f /tmp/srs.png; ~/.local/bin/cua-driver call get_window_state '{\"pid\":$pid,\"window_id\":$id,\"include_accessibility_tree\":false,\"screenshot_out_file\":\"/tmp/srs.png\"}' >/dev/null" || true
  vm "base64 -i /tmp/srs.png" | base64 -d >"$png" || true
}

srs_settled_shot() {
  sleep 1
  srs_shot "$1"
}

srs_mode() {
  local mode=$1 frame
  fresh_app_state
  settings_reset
  provider_clear_key_if_any
  obs_appearance "$mode"
  srs_launch
  expect "$mode: Welcome" ax_ok wait-text "$APP_NAME" "$SRS_WINDOW" "Talk instead of type" 30
  # The demo at its usual size, then at the smallest.
  for frame in $(seq -w 1 "$SRS_FRAMES"); do srs_shot "welcome-$mode-$frame"; done
  obs_size 680 520
  for frame in $(seq -w 1 "$SRS_FRAMES"); do srs_shot "welcome-680x520-$mode-$frame"; done
  obs_size 740 560

  # Reduce Motion: applied, then the same screen.
  expect "$mode: Reduce Motion on" srs_reduce_motion true
  sleep 1
  srs_settled_shot "welcome-reduce-motion-$mode"
  sleep 2
  srs_shot "welcome-reduce-motion-later-$mode"
  expect "$mode: Reduce Motion off" srs_reduce_motion false

  ax_ok press "$APP_NAME" "$SRS_WINDOW" "Get started"
  expect "$mode: Speech" ax_ok wait-text "$APP_NAME" "$SRS_WINDOW" "English · 477 MB" 15
  srs_settled_shot "speech-english-$mode"
  ax_ok radio-press "$APP_NAME" "$SRS_WINDOW" Hindi
  expect "$mode: Hindi" ax_ok wait-text "$APP_NAME" "$SRS_WINDOW" "30 languages · 590 MB" 10
  srs_settled_shot "speech-hindi-$mode"
  ax_ok radio-press "$APP_NAME" "$SRS_WINDOW" English
  ax_ok radio-press "$APP_NAME" "$SRS_WINDOW" "Use my own Groq or OpenAI key"
  expect "$mode: no form yet" ax_ok wait-no-text "$APP_NAME" "$SRS_WINDOW" "Test & Save" 2
  srs_settled_shot "speech-own-key-selected-$mode"
  ax_ok press "$APP_NAME" "$SRS_WINDOW" Continue
  expect "$mode: Connect your key" ax_ok wait-text "$APP_NAME" "$SRS_WINDOW" "Connect your key" 10
  expect "$mode: no key yet" ax_ok wait-text "$APP_NAME" "$SRS_WINDOW" "Continue unlocks after Test" 10
  srs_settled_shot "key-none-$mode"
  obs_size 680 520
  srs_settled_shot "key-none-680x520-$mode"
  obs_size 740 560
  ax_ok secret-set "$APP_NAME" "$SRS_WINDOW" api-key "$PROVIDER_GOOD_KEY"
  ax_ok press "$APP_NAME" "$SRS_WINDOW" "Test & Save"
  expect "$mode: Key saved" ax_ok wait-text "$APP_NAME" "$SRS_WINDOW" "Key saved" 15
  GROQ_KEY_STORED=1
  srs_settled_shot "key-saved-$mode"
  obs_size 680 520
  srs_settled_shot "key-saved-680x520-$mode"
  obs_size 740 560
  ax_ok press "$APP_NAME" "$SRS_WINDOW" Replace
  expect "$mode: Replace shows the field" ax_ok wait-text "$APP_NAME" "$SRS_WINDOW" "Test & Save" 5
  srs_settled_shot "key-replace-$mode"
  ax_ok press "$APP_NAME" "$SRS_WINDOW" Continue
  expect "$mode: Permissions" ax_ok wait-text "$APP_NAME" "$SRS_WINDOW" "Records only while you hold your key" 15
  srs_settled_shot "permissions-$mode"
  ax_ok press "$APP_NAME" "$SRS_WINDOW" Continue
  expect "$mode: Try it" ax_ok wait-text "$APP_NAME" "$SRS_WINDOW" "Your dictation key" 15
  srs_settled_shot "try-$mode"
  obs_size 680 520
  srs_settled_shot "try-680x520-$mode"
  ax_ok press "$APP_NAME" "$SRS_WINDOW" "Change key"
  expect "$mode: recording" ax_ok wait-text "$APP_NAME" "$SRS_WINDOW" "Press a key" 5
  srs_settled_shot "try-recording-680x520-$mode"
  ax_ok press "$APP_NAME" "$SRS_WINDOW" Cancel
  obs_size 740 560
}

srs_launch() {
  launch_on_provider --debug-onboarding
}

provider_clear_key_if_any() {
  GROQ_KEY_STORED=1
  provider_cleanup >/dev/null 2>&1 || true
  provider_start "$PROVIDER_GOOD_KEY"
}

e2e_setup_redesign_screens() {
  arm_key_cleanup e2e_setup_redesign_screens
  KEY_CLEANUP_HOOK=srs_restore
  srs_mode light
  quit_from_tray || true
  srs_mode dark
  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_setup_redesign_screens
  collect_log e2e_setup_redesign_screens
  srs_restore
  fresh_app_state
  settings_reset
}
