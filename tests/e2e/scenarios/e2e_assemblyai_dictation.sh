# EXPERIMENT (exp/assemblyai): the Vocet Cloud provider, AssemblyAI's
# Dictation API, on the local provider (_provider_helpers.sh), which also
# speaks `POST …/v1/transcribe` with the bare key in `Authorization` and
# answers with a verbatim `text` ("um <reply>") and the rewrite
# `llm_response` (<reply>).
#
# The settings choose the provider (assemblyai) and the made-up token is put
# in the keychain item of its own through the app (--debug-key-provider=
# assemblyai), so no real token is involved. A dictation must reach the
# provider as `format=dictation` with the good token and no language, paste
# exactly the rewrite (not the verbatim text), and leave the clipboard alone;
# a relaunch does the same. The token is in no file of the app's.

AAI_TOKEN=t4aassemblyaitoken3c9e
AAI_TOKEN_STORED=0
AAI_SETTINGS_VM="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/settings.json"

# aai_token_store: the token into the Vocet Cloud keychain item (stdin).
aai_token_store() {
  AAI_TOKEN_STORED=1
  printf '%s\n' "$AAI_TOKEN" |
    _groq_vm_with_password "$(q "$GROQ_VM_APP_BINARY") --debug-set-api-key-stdin --debug-key-provider=assemblyai" >/dev/null
}

aai_token_remove() {
  _groq_vm_with_password "$(q "$GROQ_VM_APP_BINARY") --debug-delete-api-key --debug-key-provider=assemblyai" </dev/null >/dev/null &&
    AAI_TOKEN_STORED=0
}

aai_cleanup() {
  local ok=0
  provider_cleanup || ok=1
  if [ "$AAI_TOKEN_STORED" = 1 ] && ! aai_token_remove; then
    log "  cleanup: the Vocet Cloud token may still be in the keychain"
    ok=1
  fi
  return "$ok"
}

e2e_assemblyai_dictation() {
  local pid
  arm_key_cleanup e2e_assemblyai_dictation
  KEY_CLEANUP_HOOK=aai_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$AAI_TOKEN"
  expect "the settings choose Vocet Cloud" \
    vm "mkdir -p \"\$(dirname $(q "$AAI_SETTINGS_VM"))\" && printf '%s' '{\"version\":2,\"provider\":\"assemblyai\",\"model\":\"assemblyai-dictation\"}' >$(q "$AAI_SETTINGS_VM")"
  expect "the token is stored under its own keychain item" aai_token_store

  expect "the app starts on the local provider" launch_on_provider
  expect "with Vocet Cloud selected and a saved token" \
    wait_for_log 'engine settings provider=assemblyai .*has_key=true'
  pid=$(app_pid)
  textedit_fresh
  set_clipboard "$PROVIDER_SENTINEL"

  expect "a dictation ends Idle" dictate_once 1
  expect "it reached the Dictation endpoint with the good token" provider_request_has 1 auth=good
  expect "as a Dictation API request without a language" \
    provider_request_has 1 "model=- language=-"
  expect "carrying audio" sh -c "! provider_events | grep -Eq '^request n=1 .*audio_bytes=0\$'"
  expect "the provider answered it" provider_wait_event '^replied n=1 status=200'
  expect "TextEdit holds exactly the rewrite, not the verbatim text" \
    textedit_is "$(esc_b64 "$PROVIDER_TEXT")"
  expect "it went through Inserting" \
    test "$(phases_since 0)" = "Recording Transcribing Inserting Idle"
  expect "the clipboard is the user's again" test "$(clipboard)" = "$PROVIDER_SENTINEL"
  expect "the log says the rewrite was used, without the text" \
    sh -c "app_log | grep -q 'assemblyai text.*source=\"rewrite\"\\|source=rewrite' && ! app_log | grep -q 'hello world'"
  expect "no file holds the token" no_file_holds "$AAI_TOKEN"

  expect "Quit from the tray ends the app by itself" quit_from_tray
  clear_app_log
  expect "the app starts again" launch_on_provider
  textedit_fresh
  expect "a dictation after the relaunch ends Idle" dictate_once 1
  expect "it reached the provider with the token again" provider_request_has 2 auth=good
  expect "TextEdit holds the rewrite again" textedit_is "$(esc_b64 "$PROVIDER_TEXT")"
  expect "two requests in all (no retry)" provider_requests_are 2
  expect "no file holds the token after the relaunch" no_file_holds "$AAI_TOKEN"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_assemblyai_dictation
  collect_log e2e_assemblyai_dictation
}
