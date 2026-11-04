# U4: the recording pill's states, read from the overlay's WebView (ax.js
# overlay) and the log, with a screenshot of each for the PR. The file source
# plays its level in real time, over and over (FileAudioSource), so:
#
# 1. Speech (the speech fixture, so the bars move): the pill renders
#    Listening with its hint while held, and still Listening after 2.5 s
#    (speech never says "No sound"). On release the mock pastes, and the
#    pill renders "Paste sent", for 800 ms by the log, then hides.
# 2. The silence fixture: the pill renders Listening, then "No sound" with
#    "Check your microphone" about 1.5 s after the press (the file source
#    has no device name). Nothing is pasted, so on release it goes
#    Transcribing, then hides without "Paste sent".
#
# The meter's bars carry no text (aria-hidden); the screenshots show them.

# The pill's Paste sent time (crates/app/src/overlay.rs PASTE_SENT_VISIBLE_FOR) and
# the No sound wait (dictation_core::level::NO_SOUND_AFTER), with slack for
# the log's timestamps and a 50 ms level interval.
PILL_PASTE_SENT_MS=800
PILL_NO_SOUND_MS=1500
PILL_SLACK_MS=300

# pill_near <ms> <want>: within PILL_SLACK_MS of <want>.
pill_near() {
  [ -n "$1" ] && [ "$1" -ge $(($2 - PILL_SLACK_MS)) ] && [ "$1" -le $(($2 + PILL_SLACK_MS)) ]
}

# The overlay's log lines after the first <n>, space-separated.
pill_log_since() {
  app_log | sed -nE 's/.*overlay (shown state="?([a-z_]+)"?|(hidden)).*/\2\3/p' |
    tail -n +$(($1 + 1)) | paste -sd ' ' -
}

# TextEdit holds some text (the mock's).
pill_textedit_has_text() {
  local text
  text=$(textedit_text) || return 1
  [ -n "$(printf '%s' "$text" | tr -d '[:space:]')" ]
}

pill_log_mark() {
  app_log | grep -cE 'overlay (shown state|hidden)' || true
}

# Screenshots e2e_pill_states.paste_sent.<n> 0.3, 0.6, 0.9 and 1.2 s from now,
# in the background: one call takes about a second, longer than Paste sent
# shows, so one of them catches it (the PR shows that one).
pill_paste_sent_screenshots() {
  local n
  for n in 1 2 3 4; do
    (sleep "$(echo "$n * 0.3" | bc)" && screenshot "e2e_pill_states.paste_sent.$n") &
  done
}

e2e_pill_states() {
  local mark gap
  fresh_app_state
  launch_full "$VM_FIXTURES/speech-16k.wav"
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh

  # 1. Sound: Listening, then Paste sent, then hidden.
  mark=$(pill_log_mark)
  launch --press
  expect "the pill renders Listening with its hint" overlay_shows "$OVERLAY_RECORDING"
  sleep 0.5
  screenshot e2e_pill_states.listening
  sleep 2
  expect "speech keeps it Listening" overlay_shows "$OVERLAY_RECORDING" 1
  # Reading the overlay runs a Terminal job, which comes to the front.
  expect "TextEdit is in front again for the paste" textedit_front
  launch --release
  pill_paste_sent_screenshots
  expect "the dictation gets back to Idle" wait_for_idle_count 1
  expect "the pill renders Paste sent" wait_for_log 'overlay shown state="?paste_sent' 5
  wait
  expect "then hides" wait_for_log_count 'overlay hidden' 1 5
  gap=$(log_gap_ms 'overlay shown state="?paste_sent' 'overlay (hidden|shown)') || gap=""
  log "  Paste sent showed for ${gap:-?} ms by the log"
  expect "Paste sent stayed up for ${PILL_PASTE_SENT_MS} ms" pill_near "$gap" "$PILL_PASTE_SENT_MS"
  expect "the pill went recording, transcribing, paste_sent, hidden" \
    test "$(pill_log_since "$mark")" = "recording transcribing paste_sent hidden"
  # The pill says the paste was sent, not that it landed (U4 review): the
  # inserter can't tell. Whether the text landed is e2e_mock_paste's check;
  # here a Terminal job reads the overlay mid-recording, and the first paste
  # after that has sometimes missed TextEdit (#239). Logged for the record.
  if pill_textedit_has_text; then log "  the text landed in TextEdit"; else log "  note: TextEdit is empty after the paste (see #239)"; fi
  collect_log e2e_pill_states.sound

  # 2. Silence: Listening, then No sound, then hidden without Paste sent.
  fresh_app_state
  launch_full "$VM_SILENCE"
  expect "the app starts again and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh
  mark=$(pill_log_mark)
  launch --press
  # Listening by the log: reading the overlay starts a Terminal job, which
  # on a busy VM can take longer than the 1.5 s before No sound.
  expect "the pill shows Listening first" wait_for_log 'overlay shown state="?recording' 5
  expect "then renders No sound with its hint" overlay_shows "$OVERLAY_NO_SOUND" 5
  screenshot e2e_pill_states.no_sound
  gap=$(log_gap_ms 'overlay shown state="?recording' 'overlay (hidden|shown)') || gap=""
  log "  No sound came ${gap:-?} ms after Listening by the log"
  expect "No sound came after about ${PILL_NO_SOUND_MS} ms of silence" \
    pill_near "$gap" "$PILL_NO_SOUND_MS"
  launch --release
  expect "the silent dictation gets back to Idle" wait_for_idle_count 1
  expect "the pill hides" overlay_shows "$OVERLAY_HIDDEN"
  expect "the pill went recording, no_sound, transcribing, hidden (no paste_sent)" \
    test "$(pill_log_since "$mark")" = "recording no_sound transcribing hidden"
  expect "nothing was pasted" textedit_is_empty

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_pill_states
  collect_log e2e_pill_states
}
