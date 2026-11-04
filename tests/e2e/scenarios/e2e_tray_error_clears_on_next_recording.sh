# T2 criterion 7 (P2-01), as U5 reshaped it: a dictation error shows in the
# menu bar, where the user sees it, not only in the log: the tray never has a
# text title (the icon gets a dot instead), the menu's first line says what
# went wrong ("! ..."), the next line is the one fix, and the tooltip has the
# sentence; the next recording clears all of it. The error is an offline
# one: the real engine
# (`--transcriber settings`, the default) with no API key in the keychain
# fails at once with "Add your API key in Settings.", no network needed.
# Then the app is quit from the tray and started on the mock, and a normal
# dictation pastes with the title still clear.

# The error of a dictation with no key (DictationError's text around
# TranscribeError::MissingKey's user message).
# What the tray's status line says (the code table's message).
TRAY_NO_KEY_SHOWN="No API key is saved."
# What the log says (DictationError's Display).
TRAY_NO_KEY_ERROR="Transcription failed: Add your API key in Settings."
# The tray's tooltip, with nothing wrong, and its status line.
TRAY_TOOLTIP="Vocet"
TRAY_READY_STATUS="✓ Ready · Hold Right ⌥"

# tray_shows <status> <tooltip> [seconds]: until the tray has no title at
# all, its tooltip is exactly <tooltip> and the first line of its open menu
# is exactly <status>, within [seconds] (default 5). Each read is a Terminal
# job. TRAY_FIX, if set, must also be the menu's second line (the fix row).
tray_shows() {
  local want=$1 want_help=$2 timeout=${3:-5} start=$SECONDS info menu title="" help="" first="" second=""
  while :; do
    if info=$(tray_info) && menu=$(tray_menu); then
      title=$(tray_field title "$info")
      help=$(tray_field help "$info")
      # The status lines are disabled items: ax.js marks them "(off) ".
      first=$(printf '%s\n' "$menu" | sed -n 1p | sed 's/^(off) //')
      second=$(printf '%s\n' "$menu" | sed -n 2p)
      if [ -z "$title" ] && [ "$help" = "$want_help" ] && [ "$first" = "$want" ] &&
        { [ -z "${TRAY_FIX-}" ] || [ "$second" = "$TRAY_FIX" ]; }; then
        log "  tray title: [$title], tooltip: [$help], status: [$first], next: [$second]"
        return 0
      fi
    else
      log "  could not read the tray: $info"
    fi
    [ $((SECONDS - start)) -lt "$timeout" ] || break
    sleep 1
  done
  log "  tray title: [$title], tooltip: [$help], status: [$first], next: [$second]; not [$want], [$want_help] ${TRAY_FIX-}"
  return 1
}

# env_tray_fix <fix label> <command...>: runs the command (tray_shows) with
# the fix row it must find as the menu's second line.
env_tray_fix() {
  local TRAY_FIX=$1
  shift
  "$@"
}

# The tray with nothing wrong: no title, "Vocet: Ready", the ready status.
tray_is_clear() {
  TRAY_FIX="" tray_shows "$TRAY_READY_STATUS" "Vocet: Ready" "$@"
}

# The tray showing the no-key error: the sentence in the status line and the
# tooltip, "Open Speech settings…" as the one fix.
tray_shows_no_key_error() {
  TRAY_FIX="Open Speech settings…" \
    tray_shows "! $TRAY_NO_KEY_SHOWN" "Vocet: $TRAY_NO_KEY_SHOWN" "$@"
}

e2e_tray_error_clears_on_next_recording() {
  fresh_app_state
  # No key, so the real engine can only fail (a run with --no-reset may have
  # one left; removing none is fine).
  groq_key_remove || log "  note: could not make sure no key is in the VM's keychain"
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_TONE"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  expect "the engine has no key" wait_for_log 'engine settings .*has_key=false' 30
  # U5: with no key the engine is not ready, so the menu says so from the
  # start (the old title only appeared after an error).
  expect "the tray starts with no title, saying the key is missing" tray_shows_no_key_error
  textedit_fresh

  expect "the first dictation ends Idle" dictate_once 1
  expect "it failed, shown to the user" \
    wait_for_log "shown to the user error=$TRAY_NO_KEY_ERROR"
  expect "the tray shows the error (short title, message in the tooltip)" tray_shows_no_key_error
  expect "nothing was pasted" textedit_is_empty

  # The next recording clears it, while it is still recording.
  local recordings
  recordings=$(logged_phases | grep -c '^Recording$' || true)
  launch --press
  expect "the next recording starts" \
    wait_for_log_count 'state changed phase=Recording' $((recordings + 1)) 10
  local before
  before=$(idle_count)
  launch --release
  expect "that dictation ends Idle too" wait_for_idle_count $((before + 1))
  expect "and, still without a key, shows the error again" tray_shows_no_key_error
  collect_log e2e_tray_error_clears_on_next_recording-no-key

  # A dictation that works: quit, start on the mock, paste. No error then.
  expect "Quit from the tray ends the app by itself" quit_from_tray
  clear_app_log
  launch_full
  expect "the app restarts on the mock" wait_for_log 'engine transcriber="mock"'
  expect "with no error in the tray" tray_is_clear
  textedit_fresh
  expect "a dictation on the mock ends Idle" dictate_once 1
  local text
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "it pasted" test "$text" = "$MOCK_TEXT"
  expect "and the tray still shows no error" tray_is_clear
  expect "no error was shown" test "$(log_count 'shown to the user')" = 0

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_tray_error_clears_on_next_recording
  collect_log e2e_tray_error_clears_on_next_recording
  textedit_close
  close_terminal
  fresh_app_state
}
