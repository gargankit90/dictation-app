# T2 criterion 7 (P2-01): a dictation error shows in the menu bar, where the
# user sees it (the tray's title, next to the icon), not only in the log; the
# next recording clears it. The error is an offline one: the real engine
# (`--transcriber settings`, the default) with no API key in the keychain
# fails at once with "Add your API key in Settings.", no network needed.
# Then the app is quit from the tray and started on the mock, and a normal
# dictation pastes with the title still clear.

# What the tray shows for a dictation with no key (DictationError's text
# around TranscribeError::MissingKey's user message).
TRAY_NO_KEY_ERROR="Transcription failed: Add your API key in Settings."

# tray_title_is <text> [seconds]: until the tray's title is exactly <text>
# ("" for none), within [seconds] (default 5). Each read is a Terminal job.
tray_title_is() {
  local want=$1 timeout=${2:-5} start=$SECONDS info title=""
  while :; do
    if info=$(tray_info); then
      title=$(tray_field title "$info")
      if [ "$title" = "$want" ]; then
        log "  tray title: [$title], tooltip: [$(tray_field help "$info")]"
        return 0
      fi
    else
      log "  could not read the tray: $info"
    fi
    [ $((SECONDS - start)) -lt "$timeout" ] || break
    sleep 1
  done
  log "  tray title: [$title], not [$want]"
  return 1
}

e2e_tray_error_clears_on_next_recording() {
  fresh_app_state
  # No key, so the real engine can only fail (a run with --no-reset may have
  # one left; removing none is fine).
  groq_key_remove || log "  note: could not make sure no key is in the VM's keychain"
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_TONE"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  expect "the engine has no key" wait_for_log 'engine settings .*has_key=false' 30
  expect "the tray starts with no title" tray_title_is ""
  textedit_fresh

  expect "the first dictation ends Idle" dictate_once 1
  expect "it failed, shown to the user" \
    wait_for_log "shown to the user error=$TRAY_NO_KEY_ERROR"
  expect "the tray's title shows the error" tray_title_is "$TRAY_NO_KEY_ERROR"
  expect "nothing was pasted" textedit_is_empty

  # The next recording clears it, while it is still recording.
  local recordings
  recordings=$(logged_phases | grep -c '^Recording$' || true)
  launch --press
  expect "the next recording starts" \
    wait_for_log_count 'state changed phase=Recording' $((recordings + 1)) 10
  expect "the tray's title is cleared as soon as it records" tray_title_is ""
  local before
  before=$(idle_count)
  launch --release
  expect "that dictation ends Idle too" wait_for_idle_count $((before + 1))
  expect "and, still without a key, shows the error again" tray_title_is "$TRAY_NO_KEY_ERROR"
  collect_log e2e_tray_error_clears_on_next_recording-no-key

  # A dictation that works: quit, start on the mock, paste. No error then.
  expect "Quit from the tray ends the app by itself" quit_from_tray
  clear_app_log
  launch_full
  expect "the app restarts on the mock" wait_for_log 'engine transcriber="mock"'
  expect "with no title in the tray" tray_title_is ""
  textedit_fresh
  expect "a dictation on the mock ends Idle" dictate_once 1
  local text
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "it pasted" test "$text" = "$MOCK_TEXT"
  expect "and the tray still shows no error" tray_title_is ""
  expect "no error was shown" test "$(log_count 'shown to the user')" = 0

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_tray_error_clears_on_next_recording
  collect_log e2e_tray_error_clears_on_next_recording
  textedit_close
  close_terminal
  fresh_app_state
}
