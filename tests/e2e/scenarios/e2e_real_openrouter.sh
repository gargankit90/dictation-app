# M4.4 criterion 1 (opt-in): a spoken sentence goes through the real OpenRouter
# API and comes out in TextEdit. Skipped, not failed, unless OPENROUTER_API_KEY is
# set in the environment of the run:
#
#   OPENROUTER_API_KEY=sk-or-... tests/e2e/run.sh e2e_real_openrouter
#
# The VM makes the speech (`say`, 16 kHz mono WAV) and the app "records" it
# with --debug-audio-file. The key reaches the app the way a user's does: in
# the VM's login keychain (apikey_key_store, in _key_helpers.sh), removed as
# soon as the dictation is over, and on the way out of a failed run. It's
# never written to the repo, a log, a report or a `bash -x` trace (only the
# helpers expand it, with tracing off), and the app never logs it.

E2E_SPOKEN_SENTENCE="The quick brown fox jumps over the lazy dog."

e2e_real_openrouter() {
  if ! apikey_key_present; then
    SCENARIO_SKIPPED=1
    log "  skipped: OPENROUTER_API_KEY isn't set (opt-in; this calls the real OpenRouter API)"
    return 0
  fi
  fresh_app_state

  local wav="$VM_FIXTURES/spoken.wav"
  expect "the VM makes a 16 kHz WAV of the sentence with say" \
    vm "say -o $(q "$wav") --file-format=WAVE --data-format=LEI16@16000 $(q "$E2E_SPOKEN_SENTENCE") && test -s $(q "$wav")"

  apikey_guard_start
  apikey_key_store
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$wav"
  expect "the app starts" wait_for_log 'INFO .*ready'
  expect "the engine comes from the settings and a key" \
    wait_for_log 'engine settings provider=openrouter .*has_key=true'
  apikey_textedit_new

  expect "the dictation runs through and ends Idle" apikey_dictate 2
  # The key is only needed until the request is done.
  expect "the key is removed from the VM's keychain" apikey_key_remove

  local text phases
  text=$(apikey_textedit_text)
  log "  TextEdit holds: $text"
  expect "the text in TextEdit fuzzily matches the spoken sentence" \
    apikey_fuzzy_match "$E2E_SPOKEN_SENTENCE" "$text"

  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "the full state sequence is logged in order" \
    test "$phases" = "Recording Transcribing Inserting Idle"
  expect "no error was shown to the user" apikey_log_lacks 'shown to the user'
  expect "the transcript is not in the log" apikey_log_lacks 'quick brown'
  expect "the key is not in the log" apikey_log_lacks_key

  collect_log e2e_real_openrouter
  apikey_textedit_quit
  apikey_guard_stop
}
