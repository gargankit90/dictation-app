# Helpers for the scenarios that check the app's two permissions (M5.4):
# e2e_dmg_install (the tester guide's section 3) and
# e2e_update_keeps_permissions (before and after an update). Sourced by
# run.sh with the scenarios (every scenarios/*.sh), but defines no scenario.
#
# The golden image grants Microphone and Accessibility to the app signed
# `dictation-app Dev` (tests/e2e/README.md, "macOS notes"), as a tester's Mac
# does once they allowed both. These check that the grants reach the build
# that is installed now.

# What the app logs when the microphone isn't granted, or asks for it.
PERM_MIC_MISSING="Microphone permission is missing"
PERM_MIC_PROMPT="waiting for the microphone prompt"

# The log since the last `starting version="<v>"`.
log_since_start() {
  app_log | awk -v want="starting version=\"$1\"" 'index($0, want) { buf = "" } { buf = buf $0 "\n" } END { printf "%s", buf }'
}

# Keeps the app log of one launch inside permissions_hold, so a failure
# leaves the log of the failing launch, not only that of the last one.
PERM_LAUNCH_N=0
perm_keep_log() {
  PERM_LAUNCH_N=$((PERM_LAUNCH_N + 1))
  app_log >"$OUT_DIR/permissions-$1-launch$PERM_LAUNCH_N.app.log" 2>&1 || true
}

# TextEdit is ready for a paste: frontmost, with its document window on
# screen (the window server's list, through ax.js; an ssh session has no
# Accessibility to ask TextEdit itself). A cold TextEdit (the first launch on
# a freshly reset VM) can be frontmost before its window is up, and a paste in
# that gap goes nowhere. Waits for the real state, not a sleep.
textedit_ready() {
  local i
  if ! ax wait-window TextEdit "" 20 >/dev/null 2>&1; then
    log "  TextEdit has no window on screen after 20 s"
    return 1
  fi
  for i in $(seq 1 20); do
    if vm_osascript 'tell application "System Events" to get name of first process whose frontmost is true' 2>/dev/null |
      grep -qx TextEdit; then
      return 0
    fi
    vm_osascript 'tell application "TextEdit" to activate' >/dev/null 2>&1 || true
    sleep 0.5
  done
  log "  TextEdit is not the frontmost app"
  return 1
}

# Both permissions hold for the running version <v>: the real hotkey is on
# (Accessibility), a dictation pastes into TextEdit (Accessibility), and a
# recording from the microphone (no WAV) meets no permission error or prompt.
permissions_hold() {
  local v=$1 since text ok=0
  fresh_app_state || return 1
  launch_full
  wait_for_log "INFO .*starting version=\"$v\"" || {
    log "  the app didn't start as $v"
    return 1
  }
  wait_for_log 'INFO .*ready' || return 1
  if ! hotkey_is_on; then
    log "  $v: the hotkey is off (Accessibility missing)"
    ok=1
  fi
  textedit_fresh || ok=1
  textedit_ready || ok=1
  dictate_once 1 || ok=1
  text=$(textedit_trimmed)
  perm_keep_log "$v"
  if [ "$text" != "$MOCK_TEXT" ]; then
    log "  $v: TextEdit holds [$text], not the dictation (the paste needs Accessibility)"
    screenshot "permissions-$v-textedit"
    ok=1
  fi
  textedit_close
  # The microphone, as e2e_real_mic: no WAV, so the permission is checked.
  fresh_app_state || return 1
  launch
  wait_for_log "INFO .*starting version=\"$v\"" || return 1
  wait_for_log 'INFO .*ready' || return 1
  dictate_once 1 || ok=1
  since=$(log_since_start "$v")
  perm_keep_log "$v"
  if grep -qF "$PERM_MIC_MISSING" <<<"$since" || grep -qF "$PERM_MIC_PROMPT" <<<"$since"; then
    log "  $v: the microphone isn't granted (an error or a prompt in the log)"
    ok=1
  fi
  return "$ok"
}
