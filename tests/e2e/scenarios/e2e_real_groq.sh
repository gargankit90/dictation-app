# M4.4 criterion 1 (opt-in): a spoken sentence goes through the real Groq
# API and comes out in TextEdit. Skipped, not failed, unless GROQ_API_KEY is
# set in the environment of the run:
#
#   GROQ_API_KEY=gsk_... tests/e2e/run.sh e2e_real_groq
#
# The VM makes the speech (`say`, 16 kHz mono WAV) and the app "records" it
# with --debug-audio-file. The key reaches the app the way a user's does: in
# the VM's login keychain (groq_key_store, in _groq_helpers.sh), removed as
# soon as the dictation is over. It's never written to the repo, a log or a
# report, and the app never logs it.

E2E_GROQ_SENTENCE="The quick brown fox jumps over the lazy dog."

e2e_real_groq() {
  if [ -z "${GROQ_API_KEY:-}" ]; then
    SCENARIO_SKIPPED=1
    log "  skipped: GROQ_API_KEY isn't set (opt-in; this calls the real Groq API)"
    return 0
  fi
  fresh_app_state

  local wav="$VM_FIXTURES/spoken.wav"
  expect "the VM makes a 16 kHz WAV of the sentence with say" \
    vm "say -o $(q "$wav") --file-format=WAVE --data-format=LEI16@16000 $(q "$E2E_GROQ_SENTENCE") && test -s $(q "$wav")"

  groq_key_store "$GROQ_API_KEY"
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$wav"
  expect "the app starts" wait_for_log 'INFO .*ready'
  expect "the engine comes from the settings and a key" \
    wait_for_log 'engine settings provider=groq .*has_key=true'
  groq_textedit_new

  expect "the dictation runs through and ends Idle" groq_dictate 2
  # The key is only needed until the request is done.
  groq_key_remove

  local text phases
  text=$(groq_textedit_text)
  log "  TextEdit holds: $text"
  expect "the text in TextEdit fuzzily matches the spoken sentence" \
    groq_fuzzy_match "$E2E_GROQ_SENTENCE" "$text"

  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "the full state sequence is logged in order" \
    test "$phases" = "Recording Transcribing Inserting Idle"
  expect "no error was shown to the user" groq_log_lacks 'shown to the user'
  expect "the transcript is not in the log" groq_log_lacks 'quick brown'
  expect "the key is not in the log" groq_log_lacks_key

  collect_log e2e_real_groq
  groq_textedit_quit
}
