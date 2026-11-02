# M5.9 criterion 5 (#172, the owner's reproduction): a transcript with no
# letters or digits is never pasted. A short, near-silent recording came
# back from the provider as "." and that "." was pasted over the word the
# user had selected. Here the mock transcriber answers "." for a dictation
# made while "alpha" is selected in TextEdit: TextEdit still holds "alpha",
# the clipboard holds what it held before, and no error is shown.

# The app's log has no error shown to the user.
no_error_shown() {
  local shown
  shown=$(app_log | grep 'shown to the user' || true)
  [ -z "$shown" ] && return 0
  log "  shown: $shown"
  return 1
}

e2e_no_words_transcript() {
  local old="e2e-old-clipboard-$$" text phases mark
  arm_key_cleanup e2e_no_words_transcript left-command
  fresh_app_state
  launch --debug-audio-file "$VM_TONE" --mock-text "."
  expect "the app starts and logs ready" wait_for_log 'INFO .*ready'
  textedit_fresh
  textedit_set alpha
  expect "left ⌘+A selects all of TextEdit's text" chord_in_textedit left-command a 0.1 0.1
  set_clipboard "$old"

  mark=$(phase_mark)
  expect "the dictation gets back to Idle" dictate_once 1
  sleep 1
  phases=$(phases_since "$mark")
  log "  phases: [$phases]"
  expect "it was transcribed (the mock answered)" test "${phases#*Transcribing}" != "$phases"
  case " $phases " in
    *" Inserting "*) expect "nothing was inserted" false ;;
    *) expect "nothing was inserted" true ;;
  esac
  text=$(textedit_trimmed)
  log "  TextEdit holds: [$text]"
  expect "the selection was not replaced" test "$text" = alpha
  expect "the clipboard is unchanged" test "$(clipboard)" = "$old"
  expect "no error was shown" no_error_shown
  collect_log e2e_no_words_transcript
  key_scenario_end
}
