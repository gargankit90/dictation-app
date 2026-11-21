# QT1: the cleanup pass, end to end on the local provider, asserted through
# TextEdit's exact content (not the log).
#
# A: the speech reply is "hello world"; the cleanup reply is a rewrite. What
# lands in TextEdit is the rewrite, not the transcript, and the app logged one
# info line `cleanup outcome=applied duration_ms=<n>` (no text).
#
# B: the cleanup reply is held past the 1.5 s deadline. The raw transcript is
# pasted at the deadline (well inside CLEANUP_PASTE_BOUND), with the held
# request still open at the provider side or hung up by the app; the line says
# `outcome=fallback_timeout`. Then the hold is lifted, and the next dictation
# is cleaned: no wait is left over from the cancelled one.

CLEANED_TEXT="Hello, world."
# The deadline is 1.5 s; this is generous for the dictation around it.
# The VM's dictation (record, transcribe, settle) takes 6-7 s under load, so the
# bound is on the app's own logged duration, not on wall time.
CLEANUP_PASTE_BOUND_MS=2500

# cleanup_reply <base64 of UTF-8 text>: what the cleanup answers with.
cleanup_reply() {
  printf '%s' "$1" | vm "base64 -D >$(q "$VM_PROVIDER_DIR/cleanup-text")"
}

cleanup_gate_close() {
  vm "touch $(q "$VM_PROVIDER_DIR/cleanup-gate")"
}

cleanup_gate_open() {
  vm "rm -f $(q "$VM_PROVIDER_DIR/cleanup-gate")"
}

# The app's info lines for the cleanup, outcomes in order, space-separated.
cleanup_outcomes() {
  app_log | sed -n 's/.* cleanup outcome="\{0,1\}\([a-z_]*\).*/\1/p' | paste -sd ' ' -
}

# The first cleanup line's duration in ms.
cleanup_first_ms() {
  app_log | sed -n 's/.* cleanup outcome=.* duration_ms=\([0-9]*\).*/\1/p' | head -n 1
}

e2e_cleanup_applied() {
  arm_key_cleanup e2e_cleanup_applied
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"
  provider_key_store "$PROVIDER_GOOD_KEY"
  cleanup_reply "$(esc_b64 "$CLEANED_TEXT")"
  expect "the app starts on the local provider" launch_on_provider
  textedit_fresh
  expect "a dictation ends Idle" dictate_once 1
  expect "TextEdit holds the cleaned reply, not the transcript" \
    textedit_is "$(esc_b64 "$CLEANED_TEXT")"
  expect "the cleanup was applied" test "$(cleanup_outcomes)" = applied
  expect "its log line has a duration" wait_for_log ' cleanup outcome="?applied"? duration_ms=[0-9]+' 1
  expect "the log has no text" test "$(app_log | grep -c 'Hello, world')" = 0

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_cleanup_applied
  collect_log e2e_cleanup_applied
  key_scenario_end
}

e2e_cleanup_deadline() {
  local start
  arm_key_cleanup e2e_cleanup_deadline
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"
  provider_key_store "$PROVIDER_GOOD_KEY"
  cleanup_reply "$(esc_b64 "$CLEANED_TEXT")"
  expect "the app starts on the local provider" launch_on_provider
  textedit_fresh

  cleanup_gate_close
  start=$SECONDS
  expect "a dictation ends Idle" dictate_once 1
  log "  the dictation took about $((SECONDS - start)) s"
  expect "the cleanup was held at the provider" provider_wait_event '^cleanup_held' 5
  expect "the raw transcript was pasted" textedit_is "$(esc_b64 "$PROVIDER_TEXT")"
  expect "the cleanup gave up at the deadline (app-side, not wall clock)" \
    test "$(cleanup_first_ms)" -le "$CLEANUP_PASTE_BOUND_MS"
  expect "the outcome is the timeout" test "$(cleanup_outcomes)" = fallback_timeout

  # The cancelled cleanup leaves nothing behind.
  cleanup_gate_open
  expect "the next dictation ends Idle" dictate_once 1
  expect "it was cleaned, after the first paste (smart space)" \
    textedit_is "$(esc_b64 "$PROVIDER_TEXT $CLEANED_TEXT")"
  expect "the outcomes are the timeout, then applied" \
    test "$(cleanup_outcomes)" = "fallback_timeout applied"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_cleanup_deadline
  collect_log e2e_cleanup_deadline
  key_scenario_end
}
