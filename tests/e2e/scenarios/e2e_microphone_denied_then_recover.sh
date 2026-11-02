# T4b, #126: a first-time user who denies the microphone, then allows it.
#
# Only in the test VM's clone (dictation-vm or dictation-vm-2), never the
# golden image: the app's microphone permission is reset there
# (`tccutil reset Microphone <bundle id>`), so the signed app starts
# ungranted. It is launched without the file-audio bypass (no
# --debug-audio-file, which also skips the permission check), on the mock
# transcriber answering MOCK_TEXT.
#
# 1. A press: macOS shows its microphone prompt; "Don’t Allow" is pressed
#    (ax.js, in UserNotificationCenter). The app logs the answer
#    (`microphone prompt answered granted=false`); the scenario fails if the
#    denial was never reached.
# 2. Another press: the actionable error (where to allow it, and to reopen
#    the app) in the log, rendered in the overlay and in the tray's tooltip;
#    no second prompt; the app never left Idle; nothing pasted, the clipboard
#    untouched. Settings opens and closes.
# 3. The microphone is allowed in System Settings › Privacy & Security ›
#    Microphone (its switch for the app, pressed through AX). A press in the
#    running app is only logged: macOS doesn't tell a running app about this
#    grant (T4b, docs/ENGINEERING.md), which is why the error says to reopen
#    it. Quit from the tray, a relaunch, and a dictation records the real
#    microphone with no permission error.
#
# However it ends, the grant is restored (mic_restore, the cleanup hook): if
# the scenario didn't see the grant work, the permission is reset again and
# "Allow" pressed in the prompt, and a relaunched app must record. Later
# scenarios (e2e_real_mic, the launch-at-login dictation) need it.
#
# Evidence (macOS version, the app's signature, the switch before and after,
# the prompt's answers) goes to <scenario>.evidence.txt in the run's output.

MIC_BUNDLE_ID=com.dictationapp.desktop
# DictationError::PermissionMissing(Microphone)'s text (crates/core).
MIC_ERROR="Microphone permission is missing. Allow it in System Settings, Privacy & Security, Microphone, then reopen the app."
MIC_PROMPT_APP=UserNotificationCenter
MIC_PROMPT_TEXT="would like to access the microphone"
MIC_SETTINGS_URL='x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone'
# System Settings' window and its switch for the app (the switch's
# AXIdentifier).
MIC_SETTINGS_APP="System Settings"
MIC_SETTINGS_WINDOW=Microphone
MIC_TOGGLE="$APP_NAME"_Toggle
MIC_SENTINEL="t4b microphone sentinel"
# 1 once the scenario has seen a dictation record after the grant.
MIC_GRANTED=0
MIC_EVIDENCE=""

mic_evidence() {
  printf '%s\n' "$*" >>"$MIC_EVIDENCE"
}

# The permission reset, in this VM's clone only.
mic_reset() {
  case $VM in
    dictation-vm | dictation-vm-2) ;;
    *)
      log "  refusing to reset the microphone permission in $VM"
      return 1
      ;;
  esac
  vm "tccutil reset Microphone $(q "$MIC_BUNDLE_ID")"
}

quit_system_settings() {
  vm "osascript -e 'tell application \"System Settings\" to quit'" >/dev/null 2>&1 || true
}

# mic_prompt_answer <button>: waits (up to 15 s) for the microphone prompt
# and presses <button> ("Allow" or "Don’t Allow").
mic_prompt_answer() {
  local i
  for i in $(seq 1 10); do
    if ax wait-text "$MIC_PROMPT_APP" "" "$MIC_PROMPT_TEXT" 1 >/dev/null 2>&1; then
      job_says "pressed $1" ax press "$MIC_PROMPT_APP" "" "$1" || return 1
      mic_evidence "prompt answered: $1"
      return 0
    fi
    sleep 0.5
  done
  log "  no microphone prompt within 15 s"
  return 1
}

# No microphone prompt is on screen (UserNotificationCenter has no window,
# or isn't running).
no_mic_prompt() {
  local said status=0
  said=$(ax wait-text "$MIC_PROMPT_APP" "" "$MIC_PROMPT_TEXT" 1 2>&1) || status=$?
  [ "$status" != 0 ] && return 0
  log "  a microphone prompt is showing: $said"
  return 1
}

# The app's switch in System Settings › Privacy & Security › Microphone, which
# is opened first: "checked" or "unchecked".
mic_switch() {
  vm "open $(q "$MIC_SETTINGS_URL")" || return 1
  ax_ok wait-window "$MIC_SETTINGS_APP" "$MIC_SETTINGS_WINDOW" 15 >/dev/null || return 1
  ax check-get "$MIC_SETTINGS_APP" "$MIC_SETTINGS_WINDOW" "$MIC_TOGGLE"
}

# One dictation of the real microphone (--press, 1.5 s, --release) into
# TextEdit, which comes back in front first. True if it got back to Idle
# with no permission error, recorded, and pasted the mock's text (or, if the
# VM's microphone heard only silence, dropped it as e2e_real_mic allows).
mic_dictation_records() {
  local before phases mark recordings
  mark=$(phase_mark)
  before=$(idle_count)
  recordings=$(log_count 'state changed phase=Recording')
  launch --press
  wait_for_log_count 'state changed phase=Recording' $((recordings + 1)) 10 || {
    log "  no Recording: $(app_log | grep -E 'dictation failed|shown to the user' | tail -n 2 | paste -sd '|' -)"
    return 1
  }
  textedit_front || return 1
  sleep 1.5
  launch --release
  wait_for_idle_count $((before + 1)) 20 || return 1
  phases=$(phases_since "$mark")
  log "  phases: $phases"
  if app_log | grep -q 'Microphone permission is missing'; then
    log "  the permission error is in the log"
    return 1
  fi
  case $phases in
    "Recording Transcribing Inserting Idle") textedit_is "$(esc_b64 "$MOCK_TEXT")" ;;
    "Recording Transcribing Idle") app_log | grep -q 'not transcribing.*verdict=Silent' ;;
    *) return 1 ;;
  esac
}

# The cleanup hook: the app has been quit. If the scenario didn't see the
# grant work, grant it again through the prompt and check a dictation
# records. A failure says the VM may be left without the microphone.
mic_restore() {
  local ok=0
  quit_system_settings
  if [ "$MIC_GRANTED" != 1 ]; then
    log "  cleanup: restoring the microphone permission"
    clear_app_log
    if mic_reset && launch --mock-text "$MOCK_TEXT" && wait_for_log 'INFO .*ready' &&
      launch --press && mic_prompt_answer "Allow" &&
      launch --release && wait_for_log 'microphone prompt answered granted=true' 10 &&
      fresh_app_state && launch --mock-text "$MOCK_TEXT" && wait_for_log 'INFO .*ready' &&
      textedit_fresh && mic_dictation_records; then
      log "  cleanup: the microphone is granted again"
    else
      log "  cleanup: could not restore the microphone permission; reset the VM"
      ok=1
    fi
    fresh_app_state || ok=1
  fi
  settings_reset || ok=1
  return "$ok"
}

e2e_microphone_denied_then_recover() {
  local switch said
  MIC_EVIDENCE="$OUT_DIR/e2e_microphone_denied_then_recover.evidence.txt"
  : >"$MIC_EVIDENCE"
  arm_key_cleanup e2e_microphone_denied_then_recover
  KEY_CLEANUP_HOOK=mic_restore
  fresh_app_state
  settings_reset
  quit_system_settings
  mic_evidence "vm: $VM"
  mic_evidence "macOS: $(vm 'sw_vers -productVersion; sw_vers -buildVersion' | paste -sd ' ' -)"
  mic_evidence "$(vm "codesign -dv --verbose=2 $(q "$VM_APP") 2>&1" | grep -E '^(Identifier|Authority|TeamIdentifier|CDHash)=')"

  expect "the app's microphone permission is reset in this clone" mic_reset
  textedit_fresh
  set_clipboard "$MIC_SENTINEL"
  launch --mock-text "$MOCK_TEXT"
  expect "the app starts (no audio bypass)" wait_for_log 'INFO .*ready'
  expect "with no debug audio file" \
    sh -c "! printf '%s' \"\$1\" | grep -q 'debug-audio-file'" _ "$(app_log)"

  # 1. The first press asks; the user says no.
  launch --press
  expect "the press says the microphone is missing" \
    wait_for_log "dictation failed error=Microphone permission is missing" 10
  expect "macOS asks, and Don’t Allow is pressed" mic_prompt_answer "Don’t Allow"
  expect "the app got the refusal (the denial stage was reached)" \
    wait_for_log 'microphone prompt answered granted=false' 10
  # E2E_INJECT_FAILURE=after_mic_denied dies here, to see the cleanup grant
  # the microphone again.
  inject_failure after_mic_denied
  launch --release
  switch=$(mic_switch) || switch="unreadable"
  mic_evidence "switch after the refusal: $switch"
  expect "System Settings lists the app with its switch off" test "$switch" = unchecked
  quit_system_settings

  # 2. Pressing again: the error, nothing else.
  launch --press
  expect "a second press shows the actionable error" \
    wait_for_log_count "shown to the user error=$MIC_ERROR" 2 10
  expect "the overlay renders it" overlay_shows "shown: $MIC_ERROR"
  expect "and the tray's tooltip has it" tray_shows "$TRAY_ERROR_TITLE" "$TRAY_TOOLTIP: $MIC_ERROR"
  expect "macOS doesn't ask again" no_mic_prompt
  launch --release
  sleep 2
  expect "the app never left Idle (no phase logged)" test -z "$(logged_phases)"
  expect "nothing was pasted" textedit_is_empty
  expect "the clipboard is untouched" test "$(clipboard)" = "$MIC_SENTINEL"
  expect "Settings opens" open_settings
  expect "and closes" close_settings
  expect "one app is running" app_count_is 1

  # 3. Allowed in System Settings.
  switch=$(mic_switch) || switch="unreadable"
  log "  the app's switch in System Settings: $switch"
  expect "the switch is turned on in System Settings" \
    job_says checked ax check-set "$MIC_SETTINGS_APP" "$MIC_SETTINGS_WINDOW" "$MIC_TOGGLE" on
  switch=$(mic_switch) || switch="unreadable"
  mic_evidence "switch after allowing it: $switch"
  expect "and reads back on" test "$switch" = checked
  quit_system_settings
  # What the running app does now: macOS doesn't tell it (T4b), so this is
  # only recorded; the error's advice is to reopen the app.
  said=$(log_count "shown to the user error=$MIC_ERROR")
  launch --press
  sleep 2
  launch --release
  sleep 1
  if [ "$(log_count "shown to the user error=$MIC_ERROR")" -gt "$said" ]; then
    log "  without a relaunch the app still lacks the microphone (as documented)"
    mic_evidence "grant without a relaunch: still missing"
  else
    log "  without a relaunch the app recorded (macOS told it this time)"
    mic_evidence "grant without a relaunch: took effect"
  fi
  expect "Quit from the tray ends the app by itself" quit_from_tray
  clear_app_log
  launch --mock-text "$MOCK_TEXT"
  expect "the app restarts" wait_for_log 'INFO .*ready'
  textedit_fresh
  set_clipboard "$MIC_SENTINEL"
  if mic_dictation_records; then
    MIC_GRANTED=1
    mic_evidence "grant after a relaunch: took effect"
  fi
  expect "after the relaunch a dictation records the microphone" test "$MIC_GRANTED" = 1
  expect "the clipboard is the user's again" test "$(clipboard)" = "$MIC_SENTINEL"
  log "  evidence: $(paste -sd '|' "$MIC_EVIDENCE")"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_microphone_denied_then_recover
  collect_log e2e_microphone_denied_then_recover
  key_scenario_end
}
