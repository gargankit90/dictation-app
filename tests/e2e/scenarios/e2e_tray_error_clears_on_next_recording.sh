# T2 criterion 7 (P2-01): a dictation error shows in the menu bar, where the
# user sees it (a short title next to the tray icon, the same for every error
# so the icon always fits (#151), and the whole message in its tooltip), not
# only in the log; the next recording clears both. The error is an offline
# one: the real engine
# (`--transcriber settings`, the default) with no API key in the keychain
# fails at once with "Add your API key in Settings.", no network needed.
# Then the app is quit from the tray and started on the mock, and a normal
# dictation pastes with the title still clear.

# The error of a dictation with no key (DictationError's text around
# TranscribeError::MissingKey's user message).
# What the tray shows (the code table, ErrorCode::user_text).
TRAY_NO_KEY_SHOWN="No API key saved. Add one in Settings → Speech, or use the model on this Mac."
# What the log says (DictationError's Display).
TRAY_NO_KEY_ERROR="Transcription failed: Add your API key in Settings."
# The tray's title for any error (DICTATION_FAILED in crates/app/src/tray.rs).
TRAY_ERROR_TITLE="⚠ Dictation failed"
# Its tooltip when there is no error.
TRAY_TOOLTIP="Vocet"

# tray_shows <title> <tooltip> [seconds]: until the tray's title is exactly
# <title> ("" for none) and its tooltip exactly <tooltip>, within [seconds]
# (default 5). Each read is a Terminal job.
tray_shows() {
  local want=$1 want_help=$2 timeout=${3:-5} start=$SECONDS info title="" help=""
  while :; do
    if info=$(tray_info); then
      title=$(tray_field title "$info")
      help=$(tray_field help "$info")
      if [ "$title" = "$want" ] && [ "$help" = "$want_help" ]; then
        log "  tray title: [$title], tooltip: [$help]"
        return 0
      fi
    else
      log "  could not read the tray: $info"
    fi
    [ $((SECONDS - start)) -lt "$timeout" ] || break
    sleep 1
  done
  log "  tray title: [$title], tooltip: [$help]; not [$want], [$want_help]"
  return 1
}

# The tray with no error: no title, the plain tooltip.
tray_is_clear() {
  tray_shows "" "$TRAY_TOOLTIP" "$@"
}

# The tray showing the no-key error: the short title, the message in the
# tooltip.
tray_shows_no_key_error() {
  tray_shows "$TRAY_ERROR_TITLE" "$TRAY_TOOLTIP: $TRAY_NO_KEY_SHOWN" "$@"
}

e2e_tray_error_clears_on_next_recording() {
  fresh_app_state
  # No key, so the real engine can only fail (a run with --no-reset may have
  # one left; removing none is fine).
  groq_key_remove || log "  note: could not make sure no key is in the VM's keychain"
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_TONE"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  expect "the engine has no key" wait_for_log 'engine settings .*has_key=false' 30
  expect "the tray starts with no title" tray_is_clear
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
  expect "the tray is cleared as soon as it records" tray_is_clear
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
