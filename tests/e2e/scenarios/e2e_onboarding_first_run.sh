# U8 criterion 8 (M5.2 criterion 3 before it): a first run with no saved key,
# down the own-key path. Onboarding shows at launch at Welcome, with the events
# switch on. In Speech, Test & Save refuses a key the provider rejects, leaving
# Continue locked, and accepts a made-up key the local provider (T4a,
# _provider_helpers.sh) takes. Permissions shows both allowed, Try it's
# dictation pastes into the practice box, Ready shows the real states, and
# Start dictating hides the window and takes "Resume setup…" off the tray.
# "Run setup again" in Settings, Help goes through the steps from Welcome. Neither
# key nor the transcript is ever in the app's log.
#
# The golden image grants the app the microphone and Accessibility. The launch
# records the tone and runs the real engine pointed at the local provider;
# --debug-onboarding makes that scripted launch decide about onboarding as a
# person's launch does (a test launch otherwise never shows it at launch). The
# keys are typed into the window's password field as a person types them
# (ax.js secret-set).

ONBOARDING_WINDOW="Set up"

e2e_onboarding_first_run() {
  arm_key_cleanup e2e_onboarding_first_run
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"

  expect "the app starts on the local provider" launch_on_provider --debug-onboarding
  expect "with no key saved" wait_for_log 'engine settings provider=groq .*has_key=false'
  expect "onboarding shows at launch, at Welcome" \
    wait_for_log 'onboarding shown at launch step=Welcome' 20
  expect "its window is on screen" ax_ok wait-window "$APP_NAME" "$ONBOARDING_WINDOW" 10
  expect "it says what the app does" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Talk instead of type" 10
  expect "the events switch is on" \
    job_says checked ax check-get "$APP_NAME" "$ONBOARDING_WINDOW" ob-share
  expect "press Continue" ax_ok press "$APP_NAME" "$ONBOARDING_WINDOW" Continue
  expect "Speech asks for the language" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Which language" 10
  expect "the local model is recommended" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Parakeet English" 10
  expect "choose my own key" \
    ax_ok radio-press "$APP_NAME" "$ONBOARDING_WINDOW" "Use my own Groq or OpenAI key"
  expect "Continue waits for Test & Save, and says so" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Continue unlocks after Test" 10

  # A key the provider rejects: the user stays on the step, told why, and
  # nothing is saved (a saved key rebuilds the engine).
  local rebuilt
  rebuilt=$(log_count 'engine rebuilt')
  expect "type a key the provider rejects" \
    ax_ok secret-set "$APP_NAME" "$ONBOARDING_WINDOW" api-key "$PROVIDER_BAD_KEY"
  expect "press Test & Save" ax_ok press "$APP_NAME" "$ONBOARDING_WINDOW" "Test & Save"
  expect "the window says the key was rejected" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "rejected this API key" 15
  expect "the test reached the provider with that key" provider_request_has 1 auth=bad
  expect "Continue is still locked" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Continue unlocks after Test" 5
  expect "nothing was saved" test "$(log_count 'engine rebuilt')" = "$rebuilt"

  # The made-up key the provider takes.
  expect "type the key the provider takes" \
    ax_ok secret-set "$APP_NAME" "$ONBOARDING_WINDOW" api-key "$PROVIDER_GOOD_KEY"
  expect "press Test & Save" ax_ok press "$APP_NAME" "$ONBOARDING_WINDOW" "Test & Save"
  expect "the window says it is connected" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Connected" 15
  expect "the test reached the provider with the typed key" provider_request_has 2 auth=good
  # Saved to the keychain; the cleanup removes it.
  GROQ_KEY_STORED=1
  expect "the key was saved and the engine rebuilt from it" \
    wait_for_log_count 'engine rebuilt' $((rebuilt + 1)) 10
  expect "with the key" wait_for_log 'engine settings provider=groq .*has_key=true'
  expect "press Continue" ax_ok press "$APP_NAME" "$ONBOARDING_WINDOW" Continue

  # The golden image has both permissions.
  expect "Permissions shows both cards" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Records only while you hold your key" 15
  expect "allowed" ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Allowed" 10
  expect "press Continue" ax_ok press "$APP_NAME" "$ONBOARDING_WINDOW" Continue
  expect "the window moves on to the practice box" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Your dictation key" 15

  # The test dictation, into the window's own field.
  expect "the cursor is in the practice box, the window in front" \
    ax_ok field-set "$APP_NAME" "$ONBOARDING_WINDOW" try-field ""
  expect "a dictation ends Idle" dictate_once 1
  expect "it was sent with the saved key" provider_request_has 3 auth=good
  expect "the provider answered it" provider_wait_event '^replied n=3 status=200'
  expect "the transcript is in the practice box" \
    job_says "$PROVIDER_TEXT" ax field-get "$APP_NAME" "$ONBOARDING_WINDOW" try-field
  expect "the window says it works, once it landed there" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "It works." 10
  expect "press Continue" ax_ok press "$APP_NAME" "$ONBOARDING_WINDOW" Continue

  expect "Ready shows the key" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "(hold)" 10
  expect "and that Groq is connected" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Groq · ✓ Connected" 10
  expect "and the history limit" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Keeps your last 25 dictations on this Mac" 10
  expect "the events switch is there, on" \
    job_says checked ax check-get "$APP_NAME" "$ONBOARDING_WINDOW" ob-share-ready
  expect "launch at login is off" \
    job_says unchecked ax check-get "$APP_NAME" "$ONBOARDING_WINDOW" ob-login
  expect "the tray offers Resume setup until it is finished" tray_has_resume_setup
  expect "press Start dictating" ax_ok press "$APP_NAME" "$ONBOARDING_WINDOW" "Start dictating"
  expect "the window hides" ax_ok wait-no-window "$APP_NAME" "$ONBOARDING_WINDOW" 10
  expect "the app logs that onboarding finished" wait_for_log 'onboarding finished'

  # U5, U8: with setup done the tray no longer lists "Resume setup…"; setup is
  # reached from Settings, Help, and starts again from Welcome.
  expect "the tray has no Resume setup once setup is done" tray_has_no_resume_setup
  expect "Settings… in the tray opens the window" open_settings
  expect "open the Help section" ax_ok radio-press "$APP_NAME" "$SETTINGS_WINDOW" Help
  expect "Help offers Run setup again" \
    ax_ok wait-text "$APP_NAME" "$SETTINGS_WINDOW" "Go through the first-run steps again" 10
  expect "Run setup again opens it" \
    ax_ok press "$APP_NAME" "$SETTINGS_WINDOW" "Start…"
  expect "its window is on screen" ax_ok wait-window "$APP_NAME" "$ONBOARDING_WINDOW" 10
  expect "at Welcome" ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Talk instead of type" 10
  expect "closing it hides it" ax_ok close "$APP_NAME" "$ONBOARDING_WINDOW"
  expect "it is gone" ax_ok wait-no-window "$APP_NAME" "$ONBOARDING_WINDOW" 10
  expect "the tray still has no Resume setup" tray_has_no_resume_setup

  expect "the log never holds the transcript" \
    sh -c "! printf '%s' \"\$1\" | grep -qF \"\$2\"" _ "$(app_log)" "$PROVIDER_TEXT"
  expect "the log never holds the rejected key" \
    sh -c "! printf '%s' \"\$1\" | grep -qF \"\$2\"" _ "$(app_log)" "$PROVIDER_BAD_KEY"
  expect "the log never holds the saved key" \
    sh -c "! printf '%s' \"\$1\" | grep -qF \"\$2\"" _ "$(app_log)" "$PROVIDER_GOOD_KEY"
  expect "one app is running, never relaunched" \
    test "$(app_count)/$(log_count 'INFO .*starting')" = "1/1"

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_onboarding_first_run
  collect_log e2e_onboarding_first_run
  key_scenario_end
}
