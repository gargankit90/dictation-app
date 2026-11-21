# Q3: Connect OpenRouter, end to end on the fake provider (_provider_helpers.sh,
# local-provider.pl). The test build is pointed at the fake with
# --debug-provider-url, so "Connect OpenRouter" signs in at <fake>/auth (which
# plays the sign-in page and calls the app's loopback listener back with the
# attempt's state) and exchanges the code at <fake>/auth/keys (which checks the
# PKCE verifier against the challenge). The key is made up and never printed:
#
#  1. A fresh install's Speech step has OpenRouter selected, and Continue opens
#     "Connect OpenRouter". One press of it connects: the fake saw a valid PKCE
#     exchange, the key is in the VM's keychain (the engine is rebuilt with it).
#  2. A dictation works with that key (the fake saw auth=good) and pastes.
#  3. A 402 from the fake shows "Your OpenRouter credits ran out." in the tray
#     with "Add credits…" as the fix.
#  4. Settings, Speech, Manage…, Disconnect removes the key (the engine has none).
#  5. Neither the log nor any file holds the key.

CONNECT_KEY=q3connectkey4e9d7a1c
CONNECT_WINDOW="Set up"

# The provider's own events say the attempt went through, in order.
connect_provider_saw() {
  provider_wait_event "$1" 15
}

# The control file with the key the exchange hands out. It is removed once the
# exchange is done, so no file left holds the key.
connect_key_file() {
  printf '%s' "$CONNECT_KEY" | vm "cat >$(q "$VM_PROVIDER_DIR/connect-key")"
}

# The latest "engine settings" line says no key.
connect_engine_has_no_key() {
  app_log | grep 'engine settings provider=openrouter' | tail -1 | grep -q 'has_key=false'
}

e2e_connect_openrouter() {
  arm_key_cleanup e2e_connect_openrouter
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the fake provider starts, accepting the made-up key" provider_start "$CONNECT_KEY"
  connect_key_file

  # 1. Onboarding defaults to OpenRouter; Connect OpenRouter connects.
  expect "the app starts on the fake provider, in setup" launch_on_provider --debug-onboarding
  expect "no key is saved yet" wait_for_log 'engine settings provider=openrouter .*has_key=false' 30
  expect "onboarding shows at Welcome" \
    wait_for_log 'onboarding shown at launch step=Welcome' 30
  expect "press Get started" ax_ok press "$APP_NAME" "$CONNECT_WINDOW" "Get started"
  expect "Speech offers OpenRouter" \
    ax_ok wait-text "$APP_NAME" "$CONNECT_WINDOW" "Use OpenRouter" 15
  # Nothing is picked by hand: OpenRouter is the default.
  expect "press Continue" ax_ok press "$APP_NAME" "$CONNECT_WINDOW" Continue
  expect "the next screen is Connect OpenRouter" \
    ax_ok wait-text "$APP_NAME" "$CONNECT_WINDOW" "Connect OpenRouter" 10
  expect "Continue is locked until it is connected" \
    ax_ok wait-text "$APP_NAME" "$CONNECT_WINDOW" "Continue unlocks once OpenRouter" 10
  local rebuilt
  rebuilt=$(log_count 'engine rebuilt')
  expect "press Connect OpenRouter" \
    ax_ok press-first "$APP_NAME" "$CONNECT_WINDOW" "Connect OpenRouter"
  expect "the window says OpenRouter is connected" \
    ax_ok wait-text "$APP_NAME" "$CONNECT_WINDOW" "Connected to OpenRouter" 30
  API_KEY_STORED=1
  expect "the fake saw the sign-in, the callback and a valid PKCE exchange" \
    connect_provider_saw '^exchange pkce=ok code=ok'
  expect "the sign-in came back to the app's listener (200)" \
    connect_provider_saw '^auth-callback status=200'
  expect "the key was saved and the engine rebuilt from it" \
    wait_for_log_count 'engine rebuilt' $((rebuilt + 1)) 15
  expect "the engine has the key" wait_for_log 'engine settings provider=openrouter .*has_key=true' 15
  vm "rm -f $(q "$VM_PROVIDER_DIR/connect-key")"
  expect "Continue is open" ax_ok press "$APP_NAME" "$CONNECT_WINDOW" Continue
  expect "it moves to Permissions" \
    ax_ok wait-text "$APP_NAME" "$CONNECT_WINDOW" "Records only while you hold your key" 15

  # The key is in the keychain, not a file: a relaunch reads it from there.
  expect "Quit from the tray ends the app" quit_from_tray
  clear_app_log
  expect "the app starts again on the fake provider" launch_on_provider
  expect "it finds the saved key in the keychain" \
    wait_for_log 'engine settings provider=openrouter .*has_key=true' 30

  # 2. A dictation with it.
  textedit_fresh
  set_clipboard "$PROVIDER_SENTINEL"
  expect "a dictation ends Idle" dictate_once 1
  expect "the fake saw the connected key" provider_request_has 1 auth=good
  expect "TextEdit holds the transcript" textedit_is "$(esc_b64 "$PROVIDER_TEXT")"

  # 3. A 402: out of credits.
  vm "printf 402 >$(q "$VM_PROVIDER_DIR/force-status")"
  textedit_fresh
  expect "a dictation on an empty account ends Idle" dictate_once 1
  expect "the fake answered it 402" provider_wait_event '^replied n=2 status=402'
  expect "the user is told the credits ran out" \
    wait_for_log 'shown to the user error=Transcription failed: Your OpenRouter credits ran out'
  expect "nothing was pasted" textedit_is_empty
  expect "the tray says so and offers Add credits…" \
    env_tray_fix "Add credits…" tray_shows \
    "! Your OpenRouter credits ran out." "Vocet: Your OpenRouter credits ran out."
  vm "rm -f $(q "$VM_PROVIDER_DIR/force-status")"

  # 4. Disconnect removes the key.
  expect "Settings… in the tray opens the window" open_settings
  expect "the Speech section opens" ax_ok radio-press "$APP_NAME" "$SETTINGS_WINDOW" Speech
  expect "Manage… opens the sheet" ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" "Manage…"
  expect "the sheet has Add credits" \
    ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Add credits" 10
  rebuilt=$(log_count 'engine rebuilt')
  expect "press Disconnect" ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" Disconnect
  expect "the sheet says the key is removed" \
    ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Disconnected. The key is removed." 15
  expect "the engine was rebuilt without the key" \
    wait_for_log_count 'engine rebuilt' $((rebuilt + 1)) 15
  expect "it has no key" \
    connect_engine_has_no_key
  API_KEY_STORED=0
  expect "Done closes the sheet" ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" Done
  expect "closing the window hides it" close_settings

  # 5. The key was never printed or written.
  expect "the log never held the key" apikey_log_lacks "$CONNECT_KEY"
  expect "no file holds the key" no_file_holds "$CONNECT_KEY"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_connect_openrouter
  collect_log e2e_connect_openrouter
  textedit_close
}
