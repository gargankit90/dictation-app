# M4.4 criterion 2: an invalid API key shows the error, pastes nothing and
# leaves the app usable. This makes a real request to Groq (with a key that
# can't work), so the VM needs internet access; it needs no key from you.
#
# The wrong key is saved the way a user's is (the VM's login keychain), the
# app dictates a tone through the real engine, and Groq answers 401. Then the
# app is restarted with --transcriber mock, which is what the other
# scenarios use, and a dictation pastes.

E2E_BAD_KEY=gsk_invalid_test_key

e2e_error_bad_key() {
  fresh_app_state
  groq_key_store "$E2E_BAD_KEY"
  groq_textedit_new

  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_TONE"
  expect "the app starts" wait_for_log 'INFO .*ready'
  expect "the engine comes from the settings" wait_for_log 'engine settings provider=groq .*has_key=true'

  expect "the dictation ends Idle" groq_dictate 2
  expect "the invalid key is reported to the user" \
    wait_for_log 'shown to the user error=Transcription failed: The provider rejected your API key'
  local phases
  phases=$(logged_phases | paste -sd ' ' -)
  log "  phases logged: $phases"
  expect "nothing was inserted (no Inserting phase)" \
    test "$phases" = "Recording Transcribing Idle"
  expect "nothing was pasted" test -z "$(groq_textedit_text | tr -d '[:space:]')"

  # Usable again at once: another press and release get through the same
  # states and report the same error, instead of hanging or crashing.
  expect "a second dictation also ends Idle" groq_dictate 2
  expect "and reports the error again" \
    test "$(app_log | grep -c 'shown to the user error=')" = 2
  expect "the key is not in the log" groq_log_lacks "$E2E_BAD_KEY"
  collect_log e2e_error_bad_key

  # The mock still works: restart on it and paste.
  groq_key_remove
  fresh_app_state
  groq_textedit_new
  launch --debug-audio-file "$VM_TONE"
  expect "the app restarts with the mock" wait_for_log 'engine transcriber="mock"'
  expect "a dictation with the mock ends Idle" groq_dictate 2
  expect "the mock's text is in TextEdit" \
    sh -c "printf '%s' \"\$1\" | grep -q 'This is a test dictation'" _ "$(groq_textedit_text)"
  phases=$(logged_phases | paste -sd ' ' -)
  expect "the mock run goes through Inserting" \
    test "$phases" = "Recording Transcribing Inserting Idle"
  groq_textedit_quit
}
