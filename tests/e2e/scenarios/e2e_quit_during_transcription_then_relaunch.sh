# T4a, #115, M5.8 #4: the user quits from the tray while the provider has the
# request and hasn't answered.
#
# The app must exit by itself within QUIT_BOUND seconds (no kill), having
# dropped the request: the provider sees the connection close before it
# lets the reply go, never gets a retry, and the reply it sends afterwards
# pastes nothing (TextEdit stays empty, the clipboard is the user's). A
# relaunch then dictates normally. The unit-level twin is
# `pipeline_shutdown_closes_real_http_without_retry_or_insert`
# (src-tauri/src/engine.rs).

# How long Quit may take with a request in flight. The request's own timeout
# is 30 s, so a Quit that waited for it would miss this.
QUIT_BOUND=10

quit_within_bound() {
  QUIT_TIMEOUT=$QUIT_BOUND quit_from_tray
}

e2e_quit_during_transcription_then_relaunch() {
  local start
  arm_key_cleanup e2e_quit_during_transcription_then_relaunch
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"
  provider_key_store "$PROVIDER_GOOD_KEY"
  expect "the app starts on the local provider" launch_on_provider
  textedit_fresh
  set_clipboard "$PROVIDER_SENTINEL"

  provider_gate_close
  launch --press
  sleep 1
  launch --release
  expect "the provider has the request and holds the reply" provider_wait_event '^held n=1'
  expect "the app is transcribing" test "$(phases_since 0)" = "Recording Transcribing"

  start=$SECONDS
  expect "Quit in the tray ends the app by itself within ${QUIT_BOUND}s" quit_within_bound
  log "  Quit took about $((SECONDS - start)) s"
  expect "the app is gone" app_count_is 0
  expect "the log says Quit came from the tray" wait_for_log 'quit from the tray' 1
  expect "and that the pipeline stopped" wait_for_log 'INFO [a-z_:]+: stopped( |$)' 1
  expect "the request's connection was closed" provider_wait_event '^closed n=1' 5

  # The reply the user would have had pasted, too late.
  expect "the held reply is let go" provider_release
  expect "the provider sent it" provider_wait_event '^replied n=1 status=200' 5
  sleep 3
  expect "no retry reached the provider" provider_requests_are 1
  expect "nothing was inserted" test "$(phases_since 0)" = "Recording Transcribing"
  expect "TextEdit got nothing (read back exactly)" textedit_is ""
  expect "the clipboard is untouched" test "$(clipboard)" = "$PROVIDER_SENTINEL"
  collect_log e2e_quit_during_transcription_then_relaunch-before-relaunch

  # Start again: a dictation works.
  clear_app_log
  expect "the app starts again" launch_on_provider
  textedit_fresh
  expect "a dictation after the relaunch ends Idle" dictate_once 1
  expect "it pasted the transcript" textedit_is "$(esc_b64 "$PROVIDER_TEXT")"
  expect "two requests in all" provider_requests_are 2

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_quit_during_transcription_then_relaunch
  collect_log e2e_quit_during_transcription_then_relaunch
  key_scenario_end
}
