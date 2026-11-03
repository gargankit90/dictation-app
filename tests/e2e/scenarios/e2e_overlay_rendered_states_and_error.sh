# T4b, #119: what the overlay really shows, read from its WebView's
# accessibility tree (ax.js overlay), not from the log's "overlay shown"
# lines, which only say what the app sent. On the local provider (T4a), whose
# gate holds a reply so Transcribing stays up as long as needed.
#
# 1. A press delivered right as the app starts (the first launch's --press):
#    the overlay renders Recording. An event sent before the page's listener
#    is registered is lost, which left this first overlay blank (#119); the
#    page now says when it listens and the app sends the view again. Then
#    Transcribing while the reply is held, then hidden and off screen once
#    the dictation is pasted.
# 2. A second dictation the same way.
# 3. The provider rejects the key (a 401): the overlay renders the error
#    text, it stays up for its time (ERROR_VISIBLE_FOR, 4 s, from the log's
#    timestamps) with nothing else in between, then hides and leaves the
#    screen.
# 4. Another failed dictation, and a press right after its error shows: the
#    recording replaces the error and stays up past the error's 4 s (its
#    timer must not hide the recording); with the right key back, that
#    dictation pastes and the overlay hides.
#
# The panel is checked on screen (CGWindowList, e2e_overlay_focus.sh's
# overlay_on_screen) whenever it shows something, and off screen when hidden.

# The overlay's text for the provider's 401 (TranscribeError::BadKey's user
# message in DictationError's "Transcription failed: …"). The "!" icon is
# aria-hidden, so it isn't in the text.
OVERLAY_BAD_KEY="shown: The provider rejected your API key. Check or re-enter it in Settings. key_rejected"
# The error's time on screen (crates/app/src/overlay.rs ERROR_VISIBLE_FOR),
# and how far the log's show-to-hide gap may stray from it.
OVERLAY_ERROR_MS=4000
OVERLAY_ERROR_SLACK_MS=600

# The overlay's lines in the log after the first <n>, space-separated:
# recording, transcribing, error, hidden.
overlay_log_since() {
  app_log | sed -nE 's/.*overlay (shown state="?([a-z]+)"?|(hidden)).*/\2\3/p' |
    tail -n +$(($1 + 1)) | paste -sd ' ' -
}

overlay_log_mark() {
  app_log | grep -cE 'overlay (shown state|hidden)' || true
}

# overlay_watch_start <seconds>: watches the overlay in the background
# (overlay_watch) for <seconds>; overlay_watch_end waits for it and puts what
# it saw in OVERLAY_WATCHED. Not in a $(...): only this shell can wait for
# the job.
OVERLAY_WATCH_PID=""
OVERLAY_WATCH_FILE=""
OVERLAY_WATCHED=""
overlay_watch_start() {
  OVERLAY_WATCH_FILE=$(mktemp "${TMPDIR:-/tmp}/e2e-overlay-watch.XXXXXX")
  overlay_watch "$1" >"$OVERLAY_WATCH_FILE" 2>&1 &
  OVERLAY_WATCH_PID=$!
  # Time for Terminal to start the job. Its first sample must be what shows
  # now (overlay_watch_went checks), so a late start fails, never passes.
  sleep 3
}

overlay_watch_end() {
  local status=0
  wait "$OVERLAY_WATCH_PID" || status=$?
  [ "$status" = 0 ] || log "  the overlay watch failed (exit status $status)"
  OVERLAY_WATCHED=$(cat "$OVERLAY_WATCH_FILE")
  rm -f "$OVERLAY_WATCH_FILE"
  OVERLAY_WATCH_PID=""
}

# overlay_watch_went <watch> <state>...: the watch saw exactly these states
# in this order, ignoring Transcribing (a 401 comes back too fast for it to
# be sampled every time).
overlay_watch_went() {
  local watch=$1 got want
  shift
  got=$(printf '%s\n' "$watch" | cut -d' ' -f2- | grep -vxF "$OVERLAY_TRANSCRIBING" | paste -sd '|' -)
  want=$(printf '%s\n' "$@" | paste -sd '|' -)
  log "  the overlay showed: $(printf '%s\n' "$watch" | paste -sd '|' -)"
  [ "$got" = "$want" ] && return 0
  log "  that is [$got], not [$want]"
  return 1
}

# overlay_watch_lasted <watch> <after> [state] [end ms]: how long (ms) the
# watch saw [state] (default <after>) from its first sample after <after>
# until the next change, or until [end ms] if it never changed.
overlay_watch_lasted() {
  printf '%s\n' "$1" | awk -v after="$2" -v state="${3:-$2}" -v end="${4:-}" '
    { ms = $1; sub(/^[0-9]+ /, "") }
    start == "" && seen && $0 == state { start = ms; next }
    start == "" && $0 == after { seen = 1; if (state == after) { start = ms; next } }
    start != "" && stop == "" { stop = ms }
    END {
      if (start == "") exit 1
      if (stop == "") { if (end == "") exit 1; stop = end }
      print stop - start
    }'
}

# near_ms <ms> <want>: within OVERLAY_ERROR_SLACK_MS of <want>.
near_ms() {
  [ -n "$1" ] && [ "$1" -ge $(($2 - OVERLAY_ERROR_SLACK_MS)) ] &&
    [ "$1" -le $(($2 + OVERLAY_ERROR_SLACK_MS)) ]
}

# overlay_dictation <n>: --press, Recording rendered and on screen, --release
# with the reply held, Transcribing rendered and on screen, the reply let
# go: the app is Idle for the <n>th time, the overlay hidden and off screen.
overlay_dictation() {
  local n=$1
  provider_gate_close
  launch --press
  expect "dictation $n: the overlay renders Recording" overlay_shows "$OVERLAY_RECORDING"
  expect "dictation $n: the panel is on screen" overlay_on_screen
  sleep 1
  launch --release
  expect "dictation $n: the provider holds the reply" provider_wait_event "^held n=$n"
  expect "dictation $n: the overlay renders Transcribing while it waits" overlay_shows "$OVERLAY_TRANSCRIBING"
  expect "dictation $n: the panel is still on screen" overlay_on_screen
  expect "dictation $n: TextEdit is in front again for the paste" textedit_front
  provider_release
  expect "dictation $n: back in Idle" wait_for_idle_count "$n" 15
  expect "dictation $n: the overlay is hidden" overlay_shows "$OVERLAY_HIDDEN"
  expect "dictation $n: the panel is off screen" overlay_off_screen
}

e2e_overlay_rendered_states_and_error() {
  local mark lifetime watch
  arm_key_cleanup e2e_overlay_rendered_states_and_error
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"
  provider_key_store "$PROVIDER_GOOD_KEY"
  textedit_fresh

  # 1. A press as the app starts: the first overlay event comes before the
  # page can have registered its listener.
  provider_gate_close
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_TONE" \
    --debug-provider-url "$PROVIDER_URL" --press
  expect "the press at launch is delivered" \
    wait_for_log 'test hook event=Pressed from="this launch" delivered=true'
  expect "the overlay renders Recording for a press right at launch (#119)" \
    overlay_shows "$OVERLAY_RECORDING"
  expect "the panel is on screen" overlay_on_screen
  log "  $(app_log | grep -m1 'overlay page listening' || echo 'no "overlay page listening" line')"
  expect "the engine is built from the key" wait_for_log 'engine rebuilt'
  launch --release
  expect "the provider holds the reply" provider_wait_event '^held n=1'
  expect "the overlay renders Transcribing while it waits" overlay_shows "$OVERLAY_TRANSCRIBING"
  expect "the panel is still on screen" overlay_on_screen
  expect "TextEdit is in front again for the paste" textedit_front
  provider_release
  expect "back in Idle" wait_for_idle_count 1 15
  expect "the overlay is hidden" overlay_shows "$OVERLAY_HIDDEN"
  expect "the panel is off screen" overlay_off_screen
  expect "the first dictation pasted" textedit_is "$(esc_b64 "$PROVIDER_TEXT")"

  # 2. A later dictation.
  textedit_fresh
  overlay_dictation 2
  expect "the second dictation pasted" textedit_is "$(esc_b64 "$PROVIDER_TEXT")"

  # 3. A rejected key: the provider now takes another one, so the app's is
  # wrong (a 401), offline. The overlay is watched from while it records
  # (so the watch has surely started) until well after the error's time.
  expect "the provider restarts with another key" provider_start "$PROVIDER_BAD_KEY"
  textedit_fresh
  mark=$(overlay_log_mark)
  launch --press
  expect "the overlay renders Recording" overlay_shows "$OVERLAY_RECORDING"
  overlay_watch_start 12
  launch --release
  expect "the dictation fails and is back in Idle" wait_for_idle_count 3 15
  expect "the provider rejected the key" provider_request_has 1 auth=bad
  expect "the overlay renders the error's text" overlay_shows "$OVERLAY_BAD_KEY"
  expect "the panel is on screen with the error" overlay_on_screen
  overlay_watch_end
  watch=$OVERLAY_WATCHED
  expect "the watch saw Recording, the error, then hidden, and nothing after" \
    overlay_watch_went "$watch" "$OVERLAY_RECORDING" "$OVERLAY_BAD_KEY" "$OVERLAY_HIDDEN"
  lifetime=$(overlay_watch_lasted "$watch" "$OVERLAY_BAD_KEY") || lifetime=""
  log "  the error was rendered for ${lifetime:-?} ms"
  expect "the error stayed up for its time (${OVERLAY_ERROR_MS} ms) on screen" \
    near_ms "$lifetime" "$OVERLAY_ERROR_MS"
  lifetime=$(log_gap_ms 'overlay shown state="?error' 'overlay (hidden|shown)') || lifetime=""
  log "  and for ${lifetime:-?} ms by the log"
  expect "and in the log" near_ms "$lifetime" "$OVERLAY_ERROR_MS"
  expect "the panel is off screen once the error is gone" overlay_off_screen
  expect "the overlay went recording, transcribing, error, hidden" \
    test "$(overlay_log_since "$mark")" = "recording transcribing error hidden"
  expect "nothing was pasted" textedit_is ""

  # 4. A new recording while the error shows replaces it, and the error's
  # timer doesn't hide it: watched from the failing dictation's recording
  # until 8 s or more after its error showed.
  mark=$(overlay_log_mark)
  launch --press
  expect "the overlay renders Recording" overlay_shows "$OVERLAY_RECORDING"
  overlay_watch_start 14
  launch --release
  expect "another failed dictation" wait_for_idle_count 4 15
  expect "its error shows" wait_for_log_count 'overlay shown state="?error' 2 10
  launch --press
  expect "the press right away records" wait_for_log_count 'state changed phase=Recording' 5 10
  overlay_watch_end
  watch=$OVERLAY_WATCHED
  expect "the watch saw Recording, the error, then Recording until its end" \
    overlay_watch_went "$watch" "$OVERLAY_RECORDING" "$OVERLAY_BAD_KEY" "$OVERLAY_RECORDING"
  lifetime=$(overlay_watch_lasted "$watch" "$OVERLAY_BAD_KEY" "$OVERLAY_RECORDING" 14000) || lifetime=""
  log "  the new recording was up for ${lifetime:-?} ms of the watch"
  expect "Recording stayed up past the error's time" \
    test "${lifetime:-0}" -gt $((OVERLAY_ERROR_MS + OVERLAY_ERROR_SLACK_MS))
  expect "the overlay still renders Recording" overlay_shows "$OVERLAY_RECORDING"
  expect "the panel is on screen" overlay_on_screen
  expect "the provider takes the app's key again" provider_start "$PROVIDER_GOOD_KEY"
  expect "TextEdit is in front again for the paste" textedit_front
  launch --release
  expect "that dictation ends Idle" wait_for_idle_count 5 15
  expect "the overlay is hidden" overlay_shows "$OVERLAY_HIDDEN"
  expect "the panel is off screen" overlay_off_screen
  expect "the overlay went recording, transcribing, error, recording, transcribing, hidden" \
    test "$(overlay_log_since "$mark")" = "recording transcribing error recording transcribing hidden"
  expect "it pasted" textedit_is "$(esc_b64 "$PROVIDER_TEXT")"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_overlay_rendered_states_and_error
  collect_log e2e_overlay_rendered_states_and_error
  key_scenario_end
}
