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
  dictate_once 1 || ok=1
  text=$(textedit_trimmed)
  if [ "$text" != "$MOCK_TEXT" ]; then
    log "  $v: TextEdit holds [$text], not the dictation (the paste needs Accessibility)"
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
  if grep -qF "$PERM_MIC_MISSING" <<<"$since" || grep -qF "$PERM_MIC_PROMPT" <<<"$since"; then
    log "  $v: the microphone isn't granted (an error or a prompt in the log)"
    ok=1
  fi
  return "$ok"
}
