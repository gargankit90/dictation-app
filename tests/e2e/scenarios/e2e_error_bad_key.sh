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
  groq_guard_start
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
  expect "the wrong key is removed from the VM's keychain" groq_key_remove
  fresh_app_state
  launch --debug-audio-file "$VM_TONE"
  expect "the app restarts with the mock" wait_for_log 'engine transcriber="mock"'
  # TextEdit in front after the launch, not before: once a scenario has
  # quit the app from its tray menu, the VM can leave TextEdit frontmost
  # but its window not key after a later cold launch, and the paste goes
  # nowhere (T2 VM probe: 9 of 9 missed that way after a tray Quit, 3 of 3
  # landed on a fresh VM, and 3 of 3 landed with TextEdit put in front
  # again after the launch). A second launch (--press) doesn't do this.
  groq_textedit_new
  # #203: it failed once at the check below after this restart. Wait until
  # TextEdit is frontmost with an empty document before the press, and log
  # the frontmost app if it never is.
  expect "TextEdit is frontmost with an empty document" textedit_ready
  expect "a dictation with the mock ends Idle" groq_dictate 2
  expect "the mock's text is in TextEdit" \
    sh -c "printf '%s' \"\$1\" | grep -q 'This is a test dictation'" _ "$(groq_textedit_text)"
  # expect only records the failure; say what was in front when it did.
  if [ "${SCENARIO_FAILED:-0}" = 1 ]; then
    log "  frontmost app after the paste: $(frontmost_app)"
  fi
  phases=$(logged_phases | paste -sd ' ' -)
  expect "the mock run goes through Inserting" \
    test "$phases" = "Recording Transcribing Inserting Idle"
  groq_textedit_quit
  groq_guard_stop
}
