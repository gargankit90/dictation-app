# U8 criteria 7 and 8, TOS1: all four steps with the local model and a real
# `say` clip (--debug-audio-file, the real on-device engine). Choosing the model
# starts the download and moves on, the strip shows it on the later steps, and
# after quitting mid-download setup opens at the first step that isn't done
# (Welcome counts as passed once settings exist); back on Speech, choosing the
# model again picks the download up. Try it waits for the model, then a
# dictation lands in the practice box, and Done ends setup (the window hides,
# there is no Ready step). The Speech card shows the whole download, 4.4 GB.
#
# The download comes from the stand-in model server (_model_server_helpers.sh),
# which holds it part way until the scenario lets it finish. So nothing here
# races a download that's already over: it used to delete the models folder while
# a fast one was being verified, which failed it with "Couldn't save". No
# internet; the model and its SHA-256 check are real.

FOUR_WINDOW="Set up"
FOUR_MODELS="/Users/$VM_USER/Library/Application Support/com.dictationapp.desktop/models"

four_launch() {
  E2E_TRANSCRIBER=settings launch --debug-audio-file "$VM_FIXTURES/speech-16k.wav" --debug-onboarding \
    --debug-model-url "$MODEL_SERVER_URL"
}

four_box_has_phrase() {
  ax field-get "$APP_NAME" "$FOUR_WINDOW" try-field | grep -qi "dictation test"
}

# A Terminal job gets 30 s unless told otherwise; the download takes minutes.
four_model_ready() {
  VM_TERMINAL_TIMEOUT=1510 ax_ok wait-no-text "$APP_NAME" "$FOUR_WINDOW" "Getting the model ready" 1500
}

# The server's answer about the earlier part, from the log. The note's wording
# follows this answer and is tested in logic.test.ts.
four_download_picked_up() {
  wait_for_log 'model download answered resume=(Continued|Restarted)' 30
}

e2e_onboarding_four_steps() {
  fresh_app_state
  settings_reset
  vm "rm -rf $(q "$FOUR_MODELS")"
  obs_appearance light
  expect "the stand-in model server starts" model_server_start
  four_launch
  expect "onboarding shows at launch, at Welcome" \
    wait_for_log 'onboarding shown at launch step=Welcome' 30
  expect "its window is on screen" ax_ok wait-window "$APP_NAME" "$FOUR_WINDOW" 10
  expect "Welcome" ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Talk instead of type" 10
  expect "the rail has the four steps" ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Permissions" 5
  expect "press Get started" ax_ok press "$APP_NAME" "$FOUR_WINDOW" "Get started"

  expect "Speech recommends the English model, with the whole download's size" \
    ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "English · 4.4 GB download (speech + cleanup)" 15
  obs_shot "2-speech-local-light"
  # OpenRouter is the default choice now: pick the model first.
  expect "choose this Mac" ax_ok radio-press "$APP_NAME" "$FOUR_WINDOW" "Use this Mac"
  expect "press Download and continue" \
    ax_ok press "$APP_NAME" "$FOUR_WINDOW" "Download and continue"
  expect "Permissions" \
    ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Records only while you hold your key" 15
  expect "the strip shows the download" ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Downloading on-device models" 15
  expect "the rail says it runs" ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Download running" 5
  expect "the settings choose the model already" settings_file_has transcription local
  expect "some of it is on disk" wait_for_part

  # Quit in the middle: only finishing is kept. With settings saved, Welcome
  # counts as passed, so setup opens at the first step that isn't done: Try it
  # here (the model is chosen and both permissions are allowed). Back to
  # Speech, and choosing the model again picks the download up.
  expect "quit from the tray mid-download" quit_from_tray
  clear_app_log
  four_launch
  expect "it comes back at the first step that isn't done, Try it" \
    wait_for_log 'onboarding shown at launch step=Try' 30
  expect "Try it is on screen" ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Your dictation key" 15
  expect "Back to Permissions" ax_ok press "$APP_NAME" "$FOUR_WINDOW" Back
  expect "Permissions" \
    ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Records only while you hold your key" 15
  expect "Back to Speech" ax_ok press "$APP_NAME" "$FOUR_WINDOW" Back
  expect "Speech" ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "How should Vocet transcribe" 15
  expect "choose this Mac again" ax_ok radio-press "$APP_NAME" "$FOUR_WINDOW" "Use this Mac"
  expect "press Download and continue again" \
    ax_ok press "$APP_NAME" "$FOUR_WINDOW" "Download and continue"
  expect "the download picks up" \
    ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Downloading on-device models" 30
  expect "and the server answered about the earlier part" four_download_picked_up
  expect "the server was asked for the rest only" model_server_saw_resume
  expect "allowed" ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Allowed" 10

  # Try it waits for a model that isn't here: the download is held, so it can be
  # stopped for certain, and only then is what was fetched removed.
  expect "press Cancel in the strip" ax_ok press "$APP_NAME" "$FOUR_WINDOW" Cancel
  expect "the download pauses" ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Download paused" 15
  vm "rm -rf $(q "$FOUR_MODELS")"
  expect "the strip offers to resume" ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "aren't downloaded yet" 15
  expect "press Continue" ax_ok press "$APP_NAME" "$FOUR_WINDOW" Continue
  expect "Try it blocks practice while the model is missing" \
    ax_ok wait-text "$APP_NAME" "$FOUR_WINDOW" "Practice starts once it is" 15
  model_server_release
  expect "press Resume in the strip" ax_ok press "$APP_NAME" "$FOUR_WINDOW" Resume
  expect "the wait moves on" ax_ok wait-no-text "$APP_NAME" "$FOUR_WINDOW" "Practice starts once it is" 15
  expect "the model finishes and practice opens" four_model_ready
  expect "the model loaded" wait_for_log 'on-device model loaded' 120

  # A dictation that lands in the box, then Done. The checkbox is cleared so
  # the run doesn't leave a login item behind (e2e_launch_at_login covers it).
  expect "the cursor is in the practice box" \
    ax_ok field-set "$APP_NAME" "$FOUR_WINDOW" try-field ""
  expect "a dictation ends Idle" dictate_once 2
  expect "the transcript is in the practice box" four_box_has_phrase
  expect "Open Vocet when I log in is on by default" \
    job_says checked ax check-get "$APP_NAME" "$FOUR_WINDOW" ob-login
  obs_shot "4-try-english-light"
  expect "clear Open Vocet when I log in" ax_ok check-set "$APP_NAME" "$FOUR_WINDOW" ob-login off
  expect "press Done" ax_ok press "$APP_NAME" "$FOUR_WINDOW" Done
  expect "the window hides" ax_ok wait-no-window "$APP_NAME" "$FOUR_WINDOW" 10
  expect "the app logs that onboarding finished" wait_for_log 'onboarding finished' 10
  expect "the tray has no Resume setup" tray_has_no_resume_setup

  [ "$SCENARIO_FAILED" = 0 ] || dump_ui e2e_onboarding_four_steps
  collect_log e2e_onboarding_four_steps
  fresh_app_state
  model_server_stop
  settings_reset
  vm "rm -rf $(q "$FOUR_MODELS")"
}

# A part of the model is on disk, within 60 s.
wait_for_part() {
  local i
  for i in $(seq 1 60); do
    if vm "find $(q "$FOUR_MODELS") -type f -size +1000k 2>/dev/null | grep -q ."; then
      return 0
    fi
    sleep 1
  done
  return 1
}
