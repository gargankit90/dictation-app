# T4a, #116: Esc and extra hotkey presses in the real app, with real key
# events (Terminal posts them; Accessibility is Terminal's in the VM).
#
# 1. Right Option down; once the app logs Recording, Esc (with Right Option
#    held, as the keyboard sends it), then Right Option up: back to Idle with
#    no Transcribing, no request to the provider, nothing pasted, the
#    clipboard untouched. A fresh hold then dictates.
# 2. A hold whose reply the provider holds (the gate): while it is pending
#    (the provider has the request, the app is Transcribing) another hold and
#    an Esc. Today's policy: a press while busy is ignored and Esc doesn't
#    cancel a transcription. Nothing new starts (no Recording, no second
#    request); releasing the reply pastes the transcript exactly once, and
#    nothing follows it. The next hold dictates normally.
#
# The provider answers each dictation with its own word, so a paste from the
# wrong dictation, or a second one, shows in TextEdit's exact text.

# A Terminal job: Right Option down, then (once the app's log shows one more
# Recording than before, within 10 s) Esc with Right Option held, then
# Right Option up. Option+Esc is also TextEdit's "complete" shortcut: it
# opens a word-completion list whose modal loop holds up TextEdit's Apple
# Events (and so every read of its text) until it closes, so a plain Esc
# closes it, as a user would; the app, back in Idle, ignores that one.
# Prints "posted down", "pressed 1", "posted up", "pressed 1". Releases
# the key itself however it ends, like key_hold_job.
hold_escape_release() {
  local key
  key="osascript -l JavaScript $(q "$VM_RIGHT_OPTION")"
  vm_terminal "open -a TextEdit; sleep 1; down=0
    release() { if [ \$down = 1 ]; then $key up; fi; }
    trap release EXIT; trap 'exit 1' INT TERM HUP
    recordings() {
      sh -c 'cat \"\$0\"/\"\$1\".log* 2>/dev/null' $(q "$VM_LOG_DIR" "$APP_NAME") |
        grep -c 'state changed phase=Recording'
    }
    before=\$(recordings)
    down=1; $key down || exit
    i=0
    while [ \"\$(recordings)\" -le \"\$before\" ]; do
      i=\$((i + 1)); [ \$i -le 100 ] || { echo 'no Recording within 10 s' >&2; exit 1; }
      sleep 0.1
    done
    osascript -l JavaScript $(q "$VM_KEYS") right-option+escape || exit
    sleep 0.3; $key up || exit; down=0
    sleep 0.3; osascript -l JavaScript $(q "$VM_KEYS") escape"
}

# hold_pastes <word> <want>: a 1.5 s Right Option hold, the provider answering
# <word>; one more dictation back in Idle, and TextEdit holds exactly <want>.
hold_pastes() {
  local before
  provider_reply "$(esc_b64 "$1")"
  before=$(idle_count)
  job_says "posted down
posted up" real_key_hold 1.5 || return 1
  wait_for_idle_count $((before + 1)) 15 || {
    log "  no dictation after the hold"
    return 1
  }
  textedit_is "$(esc_b64 "$2")"
}

e2e_cancel_and_busy_presses() {
  local mark
  arm_key_cleanup e2e_cancel_and_busy_presses
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"
  provider_key_store "$PROVIDER_GOOD_KEY"
  expect "the app starts on the local provider" launch_on_provider
  expect "the real hotkey is on" hotkey_is_on
  textedit_fresh
  set_clipboard "$PROVIDER_SENTINEL"

  # 1. Esc while recording.
  expect "Right Option down, Esc while recording, Right Option up" \
    job_says "posted down
pressed 1
posted up
pressed 1" hold_escape_release
  expect "the app is back in Idle" wait_for_idle_count 1 10
  sleep 2
  expect "the recording was dropped: Recording then Idle, nothing else" \
    test "$(phases_since 0)" = "Recording Idle"
  expect "no request reached the provider" provider_requests_are 0
  expect "TextEdit got nothing (read back exactly)" textedit_is ""
  expect "the clipboard is untouched" test "$(clipboard)" = "$PROVIDER_SENTINEL"
  expect "a fresh hold then dictates" hold_pastes first first
  expect "with one request" provider_requests_are 1

  # 2. Extra presses and Esc while a reply is pending.
  textedit_fresh
  provider_reply "$(esc_b64 second)"
  provider_gate_close
  mark=$(phase_mark)
  expect "a hold whose reply is held" job_says "posted down
posted up" real_key_hold 1.5
  expect "the provider has its request and holds the reply" provider_wait_event '^held n=2'
  expect "the app is transcribing" test "$(phases_since "$mark")" = "Recording Transcribing"
  # What a later request would get (the held one keeps "second"), so a
  # queued dictation would show.
  provider_reply "$(esc_b64 queued)"
  expect "another hold while transcribing" job_says "posted down
posted up" real_key_hold 0.8
  expect "and Esc" job_says "pressed 1" keys escape
  sleep 2
  expect "neither started anything" test "$(phases_since "$mark")" = "Recording Transcribing"
  expect "nor sent a request" provider_requests_are 2
  expect "the reply is released" provider_release
  expect "the dictation ends Idle" wait_for_idle_count 3 15
  sleep 3
  expect "it pasted once and nothing followed" \
    test "$(phases_since "$mark")" = "Recording Transcribing Inserting Idle"
  expect "TextEdit holds exactly the original dictation" textedit_is "$(esc_b64 second)"
  expect "still two requests" provider_requests_are 2
  expect "the clipboard is the user's again" test "$(clipboard)" = "$PROVIDER_SENTINEL"

  # The next hold works.
  textedit_fresh
  expect "the next hold dictates" hold_pastes third third
  expect "three requests in all" provider_requests_are 3

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_cancel_and_busy_presses
  collect_log e2e_cancel_and_busy_presses
  key_scenario_end
}
