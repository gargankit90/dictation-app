# M5.2 criterion 3: a first run with no saved key. Onboarding shows at
# launch, the key step refuses a key the provider rejects and accepts a
# made-up key the local provider (T4a, _provider_helpers.sh) takes, and the
# test dictation pastes into the onboarding window's own field. Done hides
# the window; "Set up dictation…" in the tray brings it back at the step the
# user was on. Neither key nor the transcript is ever in the app's log.
#
# The golden image grants the app the microphone and Accessibility, so
# onboarding opens at the key step. The launch records the tone and runs the
# real engine pointed at the local provider; --debug-onboarding makes that
# scripted launch decide about onboarding as a person's launch does (a test
# launch otherwise never shows it at launch). The keys are typed into the
# window's password field as a person types them (ax.js secret-set).

ONBOARDING_WINDOW="Set up"

e2e_onboarding_first_run() {
  arm_key_cleanup e2e_onboarding_first_run
  KEY_CLEANUP_HOOK=provider_cleanup
  fresh_app_state
  settings_reset
  expect "the local provider starts" provider_start "$PROVIDER_GOOD_KEY"

  expect "the app starts on the local provider" launch_on_provider --debug-onboarding
  expect "with no key saved" wait_for_log 'engine settings provider=groq .*has_key=false'
  expect "onboarding shows at launch, at the key step" \
    wait_for_log 'onboarding shown at launch step=Key' 20
  expect "its window is on screen" ax_ok wait-window "$APP_NAME" "$ONBOARDING_WINDOW" 10
  expect "it asks for the API key" ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Add your API key" 10

  # A key the provider rejects: the user stays on the step, told why, and
  # nothing is saved (a saved key rebuilds the engine).
  local rebuilt
  rebuilt=$(log_count 'engine rebuilt')
  expect "type a key the provider rejects" \
    ax_ok secret-set "$APP_NAME" "$ONBOARDING_WINDOW" onboarding-key "$PROVIDER_BAD_KEY"
  expect "press Test and continue" ax_ok press "$APP_NAME" "$ONBOARDING_WINDOW" "Test and continue"
  expect "the window says the key was rejected" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "rejected this API key" 15
  expect "the test reached the provider with that key" provider_request_has 1 auth=bad
  expect "the user is still on the key step" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Add your API key" 5
  expect "nothing was saved" test "$(log_count 'engine rebuilt')" = "$rebuilt"

  # The made-up key the provider takes.
  expect "type the key the provider takes" \
    ax_ok secret-set "$APP_NAME" "$ONBOARDING_WINDOW" onboarding-key "$PROVIDER_GOOD_KEY"
  expect "press Test and continue" ax_ok press "$APP_NAME" "$ONBOARDING_WINDOW" "Test and continue"
  expect "the window moves on to the test dictation" \
    ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Try it" 15
  expect "the test reached the provider with the typed key" provider_request_has 2 auth=good
  # Saved to the keychain; the cleanup removes it.
  GROQ_KEY_STORED=1
  expect "the key was saved and the engine rebuilt from it" \
    wait_for_log_count 'engine rebuilt' $((rebuilt + 1)) 10
  expect "with the key" wait_for_log 'engine settings provider=groq .*has_key=true'

  # The test dictation, into the window's own field.
  expect "the cursor is in the test field, the window in front" \
    ax_ok field-set "$APP_NAME" "$ONBOARDING_WINDOW" try-field ""
  expect "a dictation ends Idle" dictate_once 1
  expect "it was sent with the saved key" provider_request_has 3 auth=good
  expect "the provider answered it" provider_wait_event '^replied n=3 status=200'
  expect "the transcript is in the onboarding field" \
    job_says "$PROVIDER_TEXT" ax field-get "$APP_NAME" "$ONBOARDING_WINDOW" try-field
  expect "the window says it works, naming the stages the app reported" ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "It works:" 10

  expect "press Finish" ax_ok press "$APP_NAME" "$ONBOARDING_WINDOW" Finish
  expect "the window hides" ax_ok wait-no-window "$APP_NAME" "$ONBOARDING_WINDOW" 10
  expect "the app logs that onboarding finished" wait_for_log 'onboarding finished'

  # Reopened from the tray, at the step the user was on.
  expect "Set up dictation… in the tray opens it again" \
    ax_ok tray-click "$APP_NAME" "Set up"
  expect "its window is on screen" ax_ok wait-window "$APP_NAME" "$ONBOARDING_WINDOW" 10
  expect "at the test dictation" ax_ok wait-text "$APP_NAME" "$ONBOARDING_WINDOW" "Try it" 10
  expect "closing it hides it" ax_ok close "$APP_NAME" "$ONBOARDING_WINDOW"
  expect "it is gone" ax_ok wait-no-window "$APP_NAME" "$ONBOARDING_WINDOW" 10

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
